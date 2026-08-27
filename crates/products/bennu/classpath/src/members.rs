//! The **shared-seam** member index: [`TypeRef`] / [`Member`] / [`ClassMembers`] /
//! [`MemberIndex`], decoded from a `.class` byte slice.
//!
//! This is the structured (non-rendering) view that `bennu-java` and `bennu-intel`
//! consume — the counterpart to [`crate::meta`]'s human-readable [`ClassMeta`]
//! (`crate::meta::ClassMeta`). Where `meta` renders each member to a Java-like
//! string for display, this module keeps types **structured**: every return type,
//! field type and parameter is a [`TypeRef`] that carries generics (seam caveat C2 —
//! generics carry-through), so `List<Foo>.iterator()` resolves to `Iterator<Foo>`
//! once the caller substitutes `E := Foo`. Type variables (`TE;`) surface as a
//! [`TypeRef`] whose `binary_name` is the variable's name (`"E"`), which is exactly
//! what the substitution step keys on.
//!
//! Decoding path: `cafebabe::parse_class` → for each member, decode the generic
//! `Signature` attribute via [`crate::sig`] into a [`TypeRef`]; when a member has no
//! `Signature` attribute (non-generic), fall back to the erased descriptor.

use cafebabe::attributes::{AttributeData, AttributeInfo};
use cafebabe::descriptors::{FieldDescriptor, FieldType, ReturnDescriptor};
use cafebabe::{parse_class, ClassFile, FieldInfo, MethodInfo};
use serde::{Deserialize, Serialize};

use crate::sig::{ClassType, TypeArg, TypeSig};

/// A reference to a type, carrying its generic arguments.
///
/// `binary_name` is the internal/binary name with **slashes** (`java/util/List`),
/// so it can be fed straight back into [`MemberIndex::members_of`]. Three shapes the
/// seam relies on:
///
/// - **Class**: `binary_name` is the slash form (`java/util/List`), `type_args` its
///   applied arguments.
/// - **Type variable**: `binary_name` is the bare variable name (`"E"`, `"T"`,
///   `"K"`), `type_args` empty. The consumer substitutes it against the receiver's
///   type arguments (generics carry-through).
/// - **Primitive / void / array**: `binary_name` is a readable token (`"int"`,
///   `"void"`, `"java/util/List[]"`); these are terminal (no members to resolve).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeRef {
    /// Binary name with slashes (`java/util/List`), a type-variable name (`E`), or a
    /// primitive/void/array token.
    pub binary_name: String,
    /// Applied type arguments, in order. Empty when raw or non-generic.
    pub type_args: Vec<TypeRef>,
}

impl TypeRef {
    /// A plain (non-generic) reference to `binary_name`.
    pub fn plain(binary_name: impl Into<String>) -> Self {
        Self { binary_name: binary_name.into(), type_args: Vec::new() }
    }
}

/// Whether a [`Member`] is a method or a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemberKind {
    Method,
    Field,
}

/// Java member visibility, decoded from the access flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Protected,
    /// No explicit modifier — package-private.
    Package,
    Private,
}

/// One resolved method or field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// The member's simple name (`iterator`, `size`, `MAX_VALUE`).
    pub name: String,
    pub kind: MemberKind,
    /// Return type (methods) or field type (fields). Void methods → [`TypeRef`] with
    /// `binary_name == "void"`.
    pub return_type: TypeRef,
    /// Parameter types, in order. Always empty for fields.
    pub params: Vec<TypeRef>,
    pub is_static: bool,
    /// `ACC_ABSTRACT` — an abstract method (no body). Always `false` for fields. Lets a consumer
    /// tell which supertype methods a concrete class must implement.
    #[serde(default)]
    pub is_abstract: bool,
    /// An interface `default` method (a concrete instance method declared in an interface). `false`
    /// for everything else. Distinguishes a satisfied interface method from an abstract one.
    #[serde(default)]
    pub is_default: bool,
    /// `ACC_FINAL` — a `final` method (can't be overridden) or `final` field (can't be reassigned).
    /// Lets a consumer flag an illegal override / reassignment.
    #[serde(default)]
    pub is_final: bool,
    pub visibility: Visibility,
    /// The raw source string this member was decoded from: the generic `Signature`
    /// attribute when present, else the erased JVM descriptor. Kept verbatim so a
    /// consumer can render a precise detail line or re-decode if needed.
    pub raw_signature: String,
    /// The checked exceptions this method declares it `throws` — binary names with slashes
    /// (`java/io/IOException`). From the `Exceptions` attribute, EXCEPT that a type-variable throws
    /// (`<X extends Throwable> … throws X`, encoded in the generic `Signature`) is dropped — its actual
    /// thrown type is bound by the caller and may be unchecked (see [`signature_throws`]). Empty for
    /// fields and for a method with no `throws` clause. Lets a consumer flag an unhandled/undeclared
    /// checked exception at a call site. `#[serde(default)]` so an index persisted before this field
    /// existed still deserializes (empty).
    #[serde(default)]
    pub throws: Vec<String>,
}

/// Class-level access flags a checker needs (extend-final / extend-record / implement-abstract),
/// decoded from `ClassAccessFlags` + the `Record`/`PermittedSubclasses` attributes. A single struct
/// so [`ClassMembers`] grows by one field, not six.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ClassFlags {
    pub is_interface: bool,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_enum: bool,
    pub is_annotation: bool,
    /// A `record` (has a `Record` attribute — records are implicitly final and un-extendable).
    pub is_record: bool,
    /// A `sealed` class/interface (has a `PermittedSubclasses` attribute).
    pub is_sealed: bool,
}

/// A class's resolvable surface: its supertypes (for inherited-member walking) and
/// its declared members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassMembers {
    /// Superclass binary name with slashes (`java/lang/Object`), or `None` for
    /// `java/lang/Object` / interfaces without an explicit superclass in the file.
    pub superclass: Option<String>,
    /// Directly-implemented interface binary names (slashes).
    pub interfaces: Vec<String>,
    pub methods: Vec<Member>,
    pub fields: Vec<Member>,
    /// Class-level access flags (interface / abstract / final / record / sealed / …). `#[serde(default)]`
    /// so an index persisted before this field existed still deserializes (flags all `false`).
    #[serde(default)]
    pub flags: ClassFlags,
    /// The class's declared generic type-parameter NAMES, in order (`Map<K,V>` → `["K","V"]`,
    /// `Pair<L,R>` → `["L","R"]`), decoded from the class `Signature` attribute. Empty for a
    /// non-generic class (or one whose signature we couldn't decode). Lets a consumer map a method's
    /// type-variable return (`R`) to the receiver's actual type argument by POSITION, exactly instead
    /// of by naming convention. `#[serde(default)]` so a pre-existing persisted index still loads.
    #[serde(default)]
    pub type_params: Vec<String>,
}

/// Look up a class's [`ClassMembers`] by binary name (`java/util/ArrayList`).
pub trait MemberIndex {
    /// The member index for `binary_name`, or `None` when the class is not resolvable
    /// (absent from every source). Never errors on a missing class — that is a
    /// normal, non-fatal state (docs §8).
    fn members_of(&self, binary_name: &str) -> Option<ClassMembers>;

    /// The annotations written on the TYPE itself — `@Repeatable` / `@Target` / `@Retention` on an
    /// annotation type, `@Entity` on a JPA class.
    ///
    /// A SEPARATE lookup rather than a field on [`ClassMembers`], and deliberately so: that struct is
    /// memoized for every class the resolver touches, while this is asked about the handful of types
    /// a check actually names. Default `None` — an index with no class source cannot answer, and
    /// `None` must be read as "not known", never as "has none".
    fn class_annotations(&self, _binary_name: &str) -> Option<crate::annotations::ClassAnnotations> {
        None
    }
}

/// Adapt any [`ClassSource`](crate::source::ClassSource) into a [`MemberIndex`]: the
/// bridge from [`resolve_jdk_classpath`](crate::jdk::resolve_jdk_classpath) (which
/// yields a `ClassSource`) to the seam trait that `bennu-java`/`bennu-intel`
/// consume. A malformed class or an I/O error resolves to `None` (absent), keeping
/// `members_of` total.
pub struct SourceMemberIndex<S: crate::source::ClassSource> {
    source: S,
}

impl<S: crate::source::ClassSource> SourceMemberIndex<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }

    /// The wrapped source (e.g. to add more probe modules or chain further).
    pub fn source(&self) -> &S {
        &self.source
    }
}

impl<S: crate::source::ClassSource> MemberIndex for SourceMemberIndex<S> {
    fn class_annotations(&self, binary_name: &str) -> Option<crate::annotations::ClassAnnotations> {
        crate::annotations::class_annotations_of(&self.source, binary_name)
    }

    fn members_of(&self, binary_name: &str) -> Option<ClassMembers> {
        match self.source.class_bytes(binary_name) {
            Ok(Some(bytes)) => parse_class_members(&bytes).ok(),
            _ => None,
        }
    }
}

// A boxed `ClassSource` is itself a `ClassSource`, so `SourceMemberIndex<Box<dyn
// ClassSource>>` works directly over the output of `resolve_jdk_classpath`.
impl crate::source::ClassSource for Box<dyn crate::source::ClassSource> {
    fn class_bytes(&self, binary_name: &str) -> Result<Option<Vec<u8>>, String> {
        (**self).class_bytes(binary_name)
    }
    fn class_names(&self) -> Vec<String> {
        (**self).class_names()
    }
}

// ── decode: cafebabe ClassFile → ClassMembers ────────────────────────────────

/// Decode a `.class` byte slice into the seam [`ClassMembers`]. Errors (readably,
/// never panics) only on malformed bytecode — a missing/corrupt class is a normal,
/// non-fatal state per docs §8.
pub fn parse_class_members(bytes: &[u8]) -> Result<ClassMembers, String> {
    let parsed: ClassFile =
        parse_class(bytes).map_err(|e| format!("cafebabe parse failed: {e}"))?;

    // `java/lang/Object` (and interfaces, whose super is implicitly Object) may have
    // no super_class entry; carry `None` so the inherited-walk terminates.
    let superclass = parsed.super_class.as_ref().map(|c| c.to_string());
    let interfaces = parsed.interfaces.iter().map(|i| i.to_string()).collect();

    let flags = decode_class_flags(&parsed);
    let methods = parsed
        .methods
        .iter()
        .map(|m| decode_method(m, flags.is_interface))
        .collect();
    let fields = parsed.fields.iter().map(decode_field).collect();
    // The class-level generic signature carries the declared type-parameter names (`<K,V>`); decode
    // them so a consumer can map a method's type-variable return to the receiver's Nth type argument.
    let type_params = signature_attr(&parsed.attributes)
        .and_then(|raw| crate::sig::parse_class(raw).ok())
        .map(|cs| cs.type_params.into_iter().map(|tp| tp.name).collect())
        .unwrap_or_default();

    Ok(ClassMembers { superclass, interfaces, methods, fields, flags, type_params })
}

/// Just the class-level flags — what kind of type this is — without decoding a single member.
///
/// [`parse_class_members`] allocates a `Member` per method and per field, which is the right cost
/// when the members are the answer and pure waste when the question is only "is this an interface".
/// The library-class navigator asks that of a few hundred classes per keystroke, so it gets its own
/// entry point rather than paying for a member list it throws away.
pub fn parse_class_flags(bytes: &[u8]) -> Result<ClassFlags, String> {
    let parsed: ClassFile =
        parse_class(bytes).map_err(|e| format!("cafebabe parse failed: {e}"))?;
    Ok(decode_class_flags(&parsed))
}

/// Decode the class-level flags a checker needs. `record`/`sealed` come from attributes (there is no
/// `ACC_RECORD`); the rest are access-flag bits.
fn decode_class_flags(parsed: &ClassFile) -> ClassFlags {
    use cafebabe::ClassAccessFlags as F;
    let af = parsed.access_flags;
    let is_record = parsed.attributes.iter().any(|a| matches!(a.data, AttributeData::Record(_)));
    let is_sealed =
        parsed.attributes.iter().any(|a| matches!(a.data, AttributeData::PermittedSubclasses(_)));
    ClassFlags {
        is_interface: af.contains(F::INTERFACE),
        is_abstract: af.contains(F::ABSTRACT),
        is_final: af.contains(F::FINAL),
        is_enum: af.contains(F::ENUM),
        is_annotation: af.contains(F::ANNOTATION),
        is_record,
        is_sealed,
    }
}

/// Pull the raw `Signature` attribute string out of a cafebabe attribute list.
/// (`AttributeInfo` carries its own lifetime, so elision can't infer the borrow —
/// the explicit `'a` is required.)
fn signature_attr<'a>(attrs: &'a [AttributeInfo]) -> Option<&'a str> {
    attrs.iter().find_map(|a| match &a.data {
        AttributeData::Signature(s) => Some(s.as_ref()),
        _ => None,
    })
}

/// The declared checked exceptions from a method's `Exceptions` attribute — binary/internal names
/// with slashes (`java/io/IOException`), or empty when the attribute is absent (no `throws` clause).
fn exceptions_attr(attrs: &[AttributeInfo]) -> Vec<String> {
    attrs
        .iter()
        .find_map(|a| match &a.data {
            AttributeData::Exceptions(list) => Some(list.iter().map(|c| c.to_string()).collect()),
            _ => None,
        })
        .unwrap_or_default()
}

fn method_visibility(flags: cafebabe::MethodAccessFlags) -> Visibility {
    if flags.contains(cafebabe::MethodAccessFlags::PUBLIC) {
        Visibility::Public
    } else if flags.contains(cafebabe::MethodAccessFlags::PROTECTED) {
        Visibility::Protected
    } else if flags.contains(cafebabe::MethodAccessFlags::PRIVATE) {
        Visibility::Private
    } else {
        Visibility::Package
    }
}

fn field_visibility(flags: cafebabe::FieldAccessFlags) -> Visibility {
    if flags.contains(cafebabe::FieldAccessFlags::PUBLIC) {
        Visibility::Public
    } else if flags.contains(cafebabe::FieldAccessFlags::PROTECTED) {
        Visibility::Protected
    } else if flags.contains(cafebabe::FieldAccessFlags::PRIVATE) {
        Visibility::Private
    } else {
        Visibility::Package
    }
}

/// The declared checked exceptions for a method whose generic `Signature` is present. The JVM only
/// encodes a `throws` clause in the Signature when at least one thrown type is a **type variable** or
/// a parameterized type (JVMS §4.7.9.1); a plain non-generic `throws` lives ONLY in the `Exceptions`
/// attribute. So:
///   * `ms.throws` empty → the throws clause (if any) is non-generic → use the erased `Exceptions`
///     list verbatim (`Thread.sleep() throws InterruptedException` is kept);
///   * `ms.throws` non-empty → keep only the CONCRETE class throws and DROP every type-variable one
///     (`<X extends Throwable> T orElseThrow(Supplier<? extends X>) throws X`, as `Optional`): a
///     type-variable throws is bound to whatever the CALLER supplies — often an *unchecked* exception
///     (`orElseThrow(() -> new SomeRuntimeException())`) — so we can't soundly assert it's a checked
///     exception. Dropping it yields at worst a false negative; keeping it (as the erased `Throwable`)
///     produced the false "unhandled checked exception" this fixes. A concrete `throws IOException`
///     alongside a type-variable one is still kept.
fn signature_throws(ms: &crate::sig::MethodSig, erased: &[String]) -> Vec<String> {
    if ms.throws.is_empty() {
        return erased.to_vec();
    }
    ms.throws
        .iter()
        .filter(|t| !matches!(t, crate::sig::TypeSig::TypeVar(_)))
        .map(|t| type_ref_from_sig(t).binary_name)
        .collect()
}

fn decode_method(m: &MethodInfo, class_is_interface: bool) -> Member {
    use cafebabe::MethodAccessFlags as MF;
    let is_static = m.access_flags.contains(MF::STATIC);
    let is_abstract = m.access_flags.contains(MF::ABSTRACT);
    let is_final = m.access_flags.contains(MF::FINAL);
    let visibility = method_visibility(m.access_flags);
    // A concrete instance method inside an interface is a `default` method (JLS §9.4). `<clinit>`
    // (the static initialiser) is neither, but it's static so already excluded.
    let is_default = class_is_interface && !is_abstract && !is_static;
    // Declared checked exceptions (`Exceptions` attribute) — independent of the Signature/erased path.
    let throws = exceptions_attr(&m.attributes);

    // Prefer the generic Signature; fall back to the erased descriptor.
    if let Some(raw) = signature_attr(&m.attributes) {
        if let Ok(ms) = crate::sig::parse_method(raw) {
            return Member {
                name: m.name.to_string(),
                kind: MemberKind::Method,
                return_type: type_ref_from_sig(&ms.result),
                params: ms.params.iter().map(type_ref_from_sig).collect(),
                is_static,
                is_abstract,
                is_default,
                is_final,
                visibility,
                raw_signature: raw.to_string(),
                throws: signature_throws(&ms, &throws),
            };
        }
    }
    // Erased fallback: build TypeRefs from the plain descriptor.
    let (return_type, params, raw) = erased_method(m);
    Member {
        name: m.name.to_string(),
        kind: MemberKind::Method,
        return_type,
        params,
        is_static,
        is_abstract,
        is_default,
        is_final,
        visibility,
        raw_signature: raw,
        throws,
    }
}

fn decode_field(f: &FieldInfo) -> Member {
    let is_static = f.access_flags.contains(cafebabe::FieldAccessFlags::STATIC);
    let is_final = f.access_flags.contains(cafebabe::FieldAccessFlags::FINAL);
    let visibility = field_visibility(f.access_flags);

    if let Some(raw) = signature_attr(&f.attributes) {
        if let Ok(ts) = crate::sig::parse_field(raw) {
            return Member {
                name: f.name.to_string(),
                kind: MemberKind::Field,
                return_type: type_ref_from_sig(&ts),
                params: Vec::new(),
                is_static,
                is_abstract: false,
                is_default: false,
                is_final,
                visibility,
                raw_signature: raw.to_string(),
                throws: Vec::new(),
            };
        }
    }
    let raw = f.descriptor.to_string();
    Member {
        name: f.name.to_string(),
        kind: MemberKind::Field,
        return_type: type_ref_from_descriptor(&f.descriptor),
        params: Vec::new(),
        is_static,
        is_abstract: false,
        is_default: false,
        is_final,
        visibility,
        raw_signature: raw,
        throws: Vec::new(),
    }
}

// ── TypeSig (generic) → TypeRef ──────────────────────────────────────────────

/// Convert a decoded generic [`TypeSig`](crate::sig::TypeSig) into a seam [`TypeRef`].
///
/// - Class types keep their binary name (with slashes restored) and recurse into
///   type arguments; wildcards (`?`, `? extends X`, `? super X`) collapse to their
///   bound (`X`) or to `java/lang/Object` when unbounded — Phase-1 completion needs
///   a concrete class to look members up on, not variance.
/// - Type variables become a bare-name [`TypeRef`] (`E`) for later substitution.
/// - Arrays render `elem[]` as a terminal token (arrays expose only `length` +
///   `Object` methods; Phase 1 does not walk into them).
/// - Primitives/void become readable terminal tokens.
fn type_ref_from_sig(t: &TypeSig) -> TypeRef {
    match t {
        TypeSig::Base(c) => TypeRef::plain(base_name(*c)),
        TypeSig::Void => TypeRef::plain("void"),
        TypeSig::TypeVar(name) => TypeRef::plain(name.clone()),
        TypeSig::Array(inner) => {
            TypeRef::plain(format!("{}[]", type_ref_from_sig(inner).binary_name))
        }
        TypeSig::Class(ct) => class_type_to_ref(ct),
    }
}

fn class_type_to_ref(ct: &ClassType) -> TypeRef {
    // `sig` normalises the package separator to '.'; the seam wants slashes so the
    // name feeds straight back into `members_of`. Inner classes decoded via '.' in
    // the signature (rare) are joined with '$' (the binary form).
    let mut binary_name = ct.name.replace('.', "/");
    for (iname, _) in &ct.inners {
        binary_name.push('$');
        binary_name.push_str(iname);
    }
    // Type args: take the outermost application. Inner-class args are uncommon in
    // Phase-1 targets; if present we still surface the outer args, which is what the
    // element-type carry-through uses.
    let type_args = ct.args.iter().map(type_arg_to_ref).collect();
    TypeRef { binary_name, type_args }
}

fn type_arg_to_ref(a: &TypeArg) -> TypeRef {
    match a {
        TypeArg::Exact(t) => type_ref_from_sig(t),
        TypeArg::Extends(t) => type_ref_from_sig(t),
        TypeArg::Super(t) => type_ref_from_sig(t),
        // Unbounded `?` has no usable element type; Object is the safe upper bound.
        TypeArg::Unbounded => TypeRef::plain("java/lang/Object"),
    }
}

// ── FieldDescriptor (erased) → TypeRef ───────────────────────────────────────

fn type_ref_from_descriptor(d: &FieldDescriptor) -> TypeRef {
    let base = field_type_name(&d.field_type);
    if d.dimensions == 0 {
        TypeRef::plain(base)
    } else {
        let mut s = base;
        for _ in 0..d.dimensions {
            s.push_str("[]");
        }
        TypeRef::plain(s)
    }
}

fn field_type_name(t: &FieldType) -> String {
    match t {
        FieldType::Byte => "byte".to_string(),
        FieldType::Char => "char".to_string(),
        FieldType::Double => "double".to_string(),
        FieldType::Float => "float".to_string(),
        FieldType::Integer => "int".to_string(),
        FieldType::Long => "long".to_string(),
        FieldType::Short => "short".to_string(),
        FieldType::Boolean => "boolean".to_string(),
        // ClassName Derefs to the binary (slash) name already.
        FieldType::Object(c) => c.to_string(),
    }
}

fn erased_method(m: &MethodInfo) -> (TypeRef, Vec<TypeRef>, String) {
    let ret = match &m.descriptor.return_type {
        ReturnDescriptor::Void => TypeRef::plain("void"),
        ReturnDescriptor::Return(d) => type_ref_from_descriptor(d),
    };
    let params = m.descriptor.parameters.iter().map(type_ref_from_descriptor).collect();
    let raw = m.descriptor.to_string();
    (ret, params, raw)
}

fn base_name(c: char) -> &'static str {
    match c {
        'B' => "byte",
        'C' => "char",
        'D' => "double",
        'F' => "float",
        'I' => "int",
        'J' => "long",
        'S' => "short",
        'Z' => "boolean",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jdk::resolve_jdk_classpath;

    // ── Pure decode helpers (no JDK needed) ──────────────────────────────────

    #[test]
    fn typevar_becomes_bare_name_ref() {
        // `TE;` → TypeRef { binary_name: "E", type_args: [] }.
        let sig = crate::sig::parse_field("TE;").unwrap();
        let r = type_ref_from_sig(&sig);
        assert_eq!(r.binary_name, "E");
        assert!(r.type_args.is_empty());
    }

    #[test]
    fn class_type_restores_slashes_and_carries_args() {
        // `Ljava/util/Iterator<TE;>;` → java/util/Iterator<E>.
        let sig = crate::sig::parse_field("Ljava/util/Iterator<TE;>;").unwrap();
        let r = type_ref_from_sig(&sig);
        assert_eq!(r.binary_name, "java/util/Iterator");
        assert_eq!(r.type_args.len(), 1);
        assert_eq!(r.type_args[0].binary_name, "E");
    }

    #[test]
    fn unbounded_wildcard_collapses_to_object() {
        // `Ljava/lang/Class<*>;` → Class<Object>.
        let sig = crate::sig::parse_field("Ljava/lang/Class<*>;").unwrap();
        let r = type_ref_from_sig(&sig);
        assert_eq!(r.binary_name, "java/lang/Class");
        assert_eq!(r.type_args[0].binary_name, "java/lang/Object");
    }

    #[test]
    fn array_typevar_is_terminal_token() {
        // `[TT;` → T[].
        let sig = crate::sig::parse_field("[TT;").unwrap();
        let r = type_ref_from_sig(&sig);
        assert_eq!(r.binary_name, "T[]");
        assert!(r.type_args.is_empty());
    }

    // ── JDK-backed integration (skip when the level isn't installed) ─────────

    fn method<'a>(cm: &'a ClassMembers, name: &str) -> Option<&'a Member> {
        cm.methods.iter().find(|m| m.name == name)
    }

    /// Assert the members-of shape shared by every JDK (structure, not container).
    fn assert_common_shape(idx: &dyn MemberIndex, label: &str) {
        // java/util/List: iterator() -> Iterator<E>, get(int) -> E.
        let list = idx.members_of("java/util/List").unwrap_or_else(|| panic!("{label}: List"));
        let it = method(&list, "iterator").unwrap();
        assert_eq!(it.return_type.binary_name, "java/util/Iterator", "{label}: List.iterator ret");
        assert_eq!(it.return_type.type_args[0].binary_name, "E", "{label}: List.iterator elem");
        assert_eq!(method(&list, "get").unwrap().return_type.binary_name, "E", "{label}: List.get");
        // The class-level generic signature yields the declared type-parameter name(s).
        assert_eq!(list.type_params, vec!["E".to_string()], "{label}: List<E> type_params");

        // java/util/ArrayList: superclass populated for inherited-member walking.
        let al =
            idx.members_of("java/util/ArrayList").unwrap_or_else(|| panic!("{label}: ArrayList"));
        assert_eq!(al.superclass.as_deref(), Some("java/util/AbstractList"), "{label}: AL super");
        assert!(al.interfaces.iter().any(|i| i == "java/util/List"), "{label}: AL impl List");

        // java/util/Map: get(Object) -> V, declared as Map<K,V>.
        let map = idx.members_of("java/util/Map").unwrap_or_else(|| panic!("{label}: Map"));
        assert_eq!(method(&map, "get").unwrap().return_type.binary_name, "V", "{label}: Map.get");
        assert_eq!(
            map.type_params,
            vec!["K".to_string(), "V".to_string()],
            "{label}: Map<K,V> type_params in order"
        );

        // java/util/Optional: get() -> T, map(...) -> Optional<U>.
        let opt = idx.members_of("java/util/Optional").unwrap_or_else(|| panic!("{label}: Optional"));
        assert_eq!(method(&opt, "get").unwrap().return_type.binary_name, "T", "{label}: Opt.get");
        let omap = method(&opt, "map").unwrap();
        assert_eq!(omap.return_type.binary_name, "java/util/Optional", "{label}: Opt.map ret");
        assert_eq!(omap.return_type.type_args[0].binary_name, "U", "{label}: Opt.map<U>");
        assert!(omap.raw_signature.contains("Optional<TU;>"), "{label}: raw carries generics");

        // java/lang/String: erased-primitive path + supertypes.
        let s = idx.members_of("java/lang/String").unwrap_or_else(|| panic!("{label}: String"));
        assert_eq!(s.superclass.as_deref(), Some("java/lang/Object"), "{label}: String super");
        assert!(
            s.interfaces.iter().any(|i| i == "java/lang/CharSequence"),
            "{label}: String impl CharSequence"
        );
        assert_eq!(
            method(&s, "charAt").unwrap().return_type.binary_name,
            "char",
            "{label}: String.charAt -> char"
        );

        // java/lang/Object terminates the walk.
        let obj = idx.members_of("java/lang/Object").unwrap_or_else(|| panic!("{label}: Object"));
        assert!(obj.superclass.is_none(), "{label}: Object has no super");
        // Declared checked exceptions come off the `Exceptions` attribute: every `Object.wait(…)`
        // overload throws InterruptedException. A regression guard that `throws` is decoded (it feeds
        // the checked-exception checks and the decompiled-stub `throws` clause).
        assert!(
            method(&obj, "wait").is_some_and(|w| w.throws.iter().any(|t| t == "java/lang/InterruptedException")),
            "{label}: Object.wait() must declare `throws InterruptedException`"
        );

        // javax.crypto is part of the platform: `jce.jar` on JDK 8, module `java.base` on JDK 9+.
        // A regression guard that the JDK-8 boot-jar set (not just rt.jar) is loaded.
        assert!(
            idx.members_of("javax/crypto/Cipher").is_some(),
            "{label}: javax.crypto.Cipher must resolve (jce.jar on 8 / java.base on 9+)"
        );
    }

    #[test]
    fn jdk8_members() {
        let Ok(src) = resolve_jdk_classpath("1.8") else {
            eprintln!("SKIP jdk8_members: no JDK 8 installed");
            return;
        };
        assert_common_shape(&SourceMemberIndex::new(src), "JDK8");
    }

    #[test]
    fn jdk21_members() {
        let Ok(src) = resolve_jdk_classpath("21") else {
            eprintln!("SKIP jdk21_members: no JDK 21 installed");
            return;
        };
        assert_common_shape(&SourceMemberIndex::new(src), "JDK21");
    }

    #[test]
    fn static_and_visibility_flags() {
        let src = resolve_jdk_classpath("1.8")
            .or_else(|_| resolve_jdk_classpath("21"))
            .ok();
        let Some(src) = src else {
            eprintln!("SKIP static_and_visibility_flags: no JDK installed");
            return;
        };
        let idx = SourceMemberIndex::new(src);

        // Optional.of is a public static factory.
        let opt = idx.members_of("java/util/Optional").unwrap();
        let of = opt.methods.iter().find(|m| m.name == "of").unwrap();
        assert!(of.is_static, "Optional.of static");
        assert_eq!(of.visibility, Visibility::Public);

        // Integer.MAX_VALUE is a public static int field.
        let integer = idx.members_of("java/lang/Integer").unwrap();
        let max = integer.fields.iter().find(|f| f.name == "MAX_VALUE").unwrap();
        assert!(max.is_static, "Integer.MAX_VALUE static");
        assert_eq!(max.visibility, Visibility::Public);
        assert_eq!(max.return_type.binary_name, "int");
        assert_eq!(max.kind, MemberKind::Field);
    }

    #[test]
    fn throws_clause_is_decoded() {
        let src = resolve_jdk_classpath("1.8")
            .or_else(|_| resolve_jdk_classpath("21"))
            .ok();
        let Some(src) = src else {
            eprintln!("SKIP throws_clause_is_decoded: no JDK installed");
            return;
        };
        let idx = SourceMemberIndex::new(src);

        // `Thread.sleep(long)` declares `throws InterruptedException` (Exceptions attribute).
        let thread = idx.members_of("java/lang/Thread").unwrap();
        let sleep = thread
            .methods
            .iter()
            .find(|m| m.name == "sleep" && m.params.len() == 1)
            .expect("Thread.sleep(long)");
        assert!(
            sleep.throws.iter().any(|t| t == "java/lang/InterruptedException"),
            "Thread.sleep should declare InterruptedException, got {:?}",
            sleep.throws
        );

        // A method with no `throws` clause has an empty list (e.g. String.length()).
        let string = idx.members_of("java/lang/String").unwrap();
        let length = string.methods.iter().find(|m| m.name == "length").unwrap();
        assert!(length.throws.is_empty(), "String.length throws nothing, got {:?}", length.throws);
    }

    #[test]
    fn signature_throws_drops_type_variables_keeps_concrete() {
        use crate::sig::{ClassType, MethodSig, TypeSig};
        let ioe = || {
            TypeSig::Class(ClassType { name: "java.io.IOException".into(), args: vec![], inners: vec![] })
        };
        let ms = |throws| MethodSig {
            type_params: vec![],
            params: vec![],
            result: TypeSig::Void,
            throws,
        };

        // `<X extends Throwable> … throws X` (Optional.orElseThrow): the erased `Exceptions` attribute
        // is `Throwable`, but the Signature marks it a type variable → dropped (no false "unhandled").
        assert!(
            signature_throws(&ms(vec![TypeSig::TypeVar("X".into())]), &["java/lang/Throwable".into()])
                .is_empty(),
            "a type-variable throws must be dropped"
        );
        // A concrete throws alongside a type-variable one is kept.
        assert_eq!(
            signature_throws(
                &ms(vec![TypeSig::TypeVar("X".into()), ioe()]),
                &["java/lang/Throwable".into(), "java/io/IOException".into()],
            ),
            vec!["java/io/IOException".to_string()],
        );
        // No throws in the Signature (non-generic throws) → fall back to the erased `Exceptions` list.
        assert_eq!(
            signature_throws(&ms(vec![]), &["java/io/IOException".into()]),
            vec!["java/io/IOException".to_string()],
        );
    }
}
