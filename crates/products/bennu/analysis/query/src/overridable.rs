//! Which methods the class under the caret can override — the list the "Implement / override
//! methods" dialog shows.
//!
//! The resolver half of the feature: it needs the whole supertype hierarchy, which is exactly what
//! this crate has. What a generated method *looks like* is the other half and lives in
//! `bennu-intentions` as a pure transform.
//!
//! ## What is offered, and what is not
//!
//! A method is offered when Java would actually let you override it, which rules out more than it
//! sounds:
//!
//! - **`static`, `final`, `private`** — none of them can be overridden. A `static` method with the
//!   same signature *hides* rather than overrides, which is a different thing and a bug when it was
//!   not meant.
//! - **Constructors and static initialisers** — members of the class file, not of the class.
//! - **Package-private from another package** — invisible here, so not overridable here. Same rule
//!   the completion popup uses, from the same place ([`crate::access`]).
//! - **Already declared by this class** — matched on name AND parameter types, because that is what
//!   overriding means. Adding a second `speak()` beside the one you wrote does not compile, and
//!   offering it is how a generator produces a file that no longer builds.
//!
//! Abstract methods come first: they are the ones the compiler will demand, and a dialog that opens
//! with them selected is the difference between "implement this interface" being one gesture and
//! being a hunt through forty inherited members.

use std::collections::{HashMap, HashSet};

use bennu_java::prelude::{
    enclosing_type_binary, ClassMembers, Member, MemberKind, TypeRef, TypeResolver, Visibility,
};

use crate::access::{same_package, same_top_level};
use crate::member_text::{parameters, render_signature, render_type, simple_of};

/// One method the caret's class could override.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overridable {
    pub name: String,
    /// `(written type, parameter name)` in declaration order.
    pub params: Vec<(String, String)>,
    /// The return type as it should be written; `"void"` for none.
    pub return_type: String,
    /// `"public"` | `"protected"` | `"package"`.
    pub visibility: String,
    /// The supertype declares it abstract — the compiler will demand it.
    pub is_abstract: bool,
    /// Declared checked exceptions, as written type names.
    pub throws: Vec<String>,
    /// Dotted FQCN of the type that declares it — what the dialog groups by.
    pub declaring_type: String,
    /// A readable one-line signature for the dialog row.
    pub signature: String,
    /// Every type the generated method mentions, as a JVM binary name — the return, the
    /// parameters (and their generic arguments) and the throws. Carried so the generator can add
    /// the imports the new method needs: written with simple names and no import, an override of
    /// `List<Order> load() throws SQLException` compiles nowhere.
    pub types: Vec<String>,
}

/// Every method the type enclosing byte `offset` in `source` can override, abstract ones first.
///
/// `None` of the work needs the file to be saved: the enclosing type is read off the buffer, so the
/// list is right for the class as it stands right now — including a supertype you have just added
/// to the `extends` clause.
///
/// Empty when the caret is not inside a type, or the type cannot be resolved (a cold index) — both
/// benign, and the dialog says it has nothing rather than guessing.
pub fn overridable_at(
    source: &str,
    offset: usize,
    resolver: &dyn TypeResolver,
) -> Vec<Overridable> {
    let Some(binary) = enclosing_type_binary(source, offset) else {
        return Vec::new();
    };
    let Some(own) = resolver.members_of(&binary) else {
        return Vec::new();
    };

    // What this class already declares, keyed the way overriding is defined: name AND parameter
    // types. Keying on the name alone would hide every overload you have not written yet.
    let declared: HashSet<String> = own
        .methods
        .iter()
        .map(signature_key)
        .collect();

    let mut out: Vec<Overridable> = Vec::new();
    let mut seen: HashSet<String> = declared.clone();
    let mut visited: HashSet<String> = HashSet::from([binary.clone()]);

    for supertype in supertypes(&own) {
        collect(resolver, &supertype, &binary, &mut out, &mut seen, &mut visited);
    }

    // What the compiler will demand, first — and within each group, the shape a reader scans:
    // by declaring type, then by name.
    out.sort_by(|a, b| {
        b.is_abstract
            .cmp(&a.is_abstract)
            .then(a.declaring_type.cmp(&b.declaring_type))
            .then(a.name.cmp(&b.name))
            .then(a.params.len().cmp(&b.params.len()))
    });
    out
}

/// The superclass + interfaces of `cm`, in the order the walk should take them.
fn supertypes(cm: &ClassMembers) -> Vec<String> {
    cm.superclass
        .iter()
        .chain(cm.interfaces.iter())
        .map(|t| t.binary_name.clone())
        .collect()
}

fn collect(
    resolver: &dyn TypeResolver,
    binary: &str,
    site: &str,
    out: &mut Vec<Overridable>,
    seen: &mut HashSet<String>,
    visited: &mut HashSet<String>,
) {
    if !visited.insert(binary.to_string()) {
        return;
    }
    let Some(cm) = resolver.members_of(binary) else {
        return;
    };
    let declaring_type = binary.replace('/', ".");

    for m in &cm.methods {
        if m.kind != MemberKind::Method || !can_be_overridden(m, binary, site) {
            continue;
        }
        // An override lower in the hierarchy — or in the class itself — already claimed this
        // signature. Whichever came first is the one that would be overridden.
        if !seen.insert(signature_key(m)) {
            continue;
        }
        let params = parameters(m);
        let mut types: Vec<String> = Vec::new();
        collect_types(&m.return_type, &mut types);
        for p in &m.params {
            collect_types(p, &mut types);
        }
        types.extend(m.throws.iter().cloned());
        out.push(Overridable {
            signature: render_signature(&m.name, &params, &render_type(&m.return_type)),
            name: m.name.clone(),
            params,
            return_type: render_type(&m.return_type),
            visibility: visibility_tag(m.visibility).to_string(),
            // An interface method is abstract unless it is a `default` one — the flag alone is not
            // the whole answer for a type read from source.
            is_abstract: m.is_abstract || (cm.flags.is_interface && !m.is_default && !m.is_static),
            throws: m.throws.iter().map(|t| simple_of(t).to_string()).collect(),
            declaring_type: declaring_type.clone(),
            types,
        });
    }

    for next in supertypes(&cm) {
        collect(resolver, &next, site, out, seen, visited);
    }
}

/// Whether Java would let the class at `site` override `m`, declared in `declaring`.
fn can_be_overridden(m: &Member, declaring: &str, site: &str) -> bool {
    if m.name == "<init>" || m.name == "<clinit>" {
        return false; // members of the class file, not of the class
    }
    if m.is_static || m.is_final {
        return false; // a same-signature static HIDES, and a final one is refused outright
    }
    match m.visibility {
        // Never inherited. Offered only from inside the same top-level class, where it is not an
        // override at all — so never.
        Visibility::Private => false,
        Visibility::Package => same_package(declaring, Some(site)) || same_top_level(declaring, Some(site)),
        _ => true,
    }
}

/// The identity overriding is defined on: the name and the parameter types.
fn signature_key(m: &Member) -> String {
    let mut key = m.name.clone();
    for p in &m.params {
        key.push('(');
        key.push_str(&p.binary_name);
    }
    key
}

/// Every binary name a type reference mentions, itself and its generic arguments. A primitive or
/// an unresolved name (no `/`) is not a type anything can import, so it is left out.
fn collect_types(t: &TypeRef, out: &mut Vec<String>) {
    if t.binary_name.contains('/') {
        // `binary_name` is the element type now; the trim is kept for an index persisted before
        // that was true, which still carries the brackets inside the name.
        let bare = t.binary_name.trim_end_matches("[]").to_string();
        if !out.contains(&bare) {
            out.push(bare);
        }
    }
    for a in &t.type_args {
        collect_types(a, out);
    }
}

fn visibility_tag(v: Visibility) -> &'static str {
    match v {
        Visibility::Public => "public",
        Visibility::Protected => "protected",
        Visibility::Private => "private",
        Visibility::Package => "package",
    }
}

/// Group `items` by declaring type, preserving the order they arrived in — what the dialog renders
/// as one collapsible section per supertype.
pub fn by_declaring_type(items: &[Overridable]) -> Vec<(String, Vec<Overridable>)> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<Overridable>> = HashMap::new();
    for it in items {
        if !groups.contains_key(&it.declaring_type) {
            order.push(it.declaring_type.clone());
        }
        groups.entry(it.declaring_type.clone()).or_default().push(it.clone());
    }
    order
        .into_iter()
        .map(|k| {
            let v = groups.remove(&k).unwrap_or_default();
            (k, v)
        })
        .collect()
}


#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{Member, TypeRef};

    fn method(name: &str, params: &[&str]) -> Member {
        Member::method(
            name,
            TypeRef::simple("void"),
            params.iter().map(|p| TypeRef::simple(*p)).collect(),
        )
    }

    const HERE: &str = "com/acme/Sub";
    const THERE: &str = "com/acme/Base";

    #[test]
    fn an_ordinary_inherited_method_can_be_overridden() {
        assert!(can_be_overridden(&method("speak", &[]), THERE, HERE));
    }

    /// A same-signature `static` HIDES rather than overrides, and a `final` one is refused outright.
    #[test]
    fn static_and_final_methods_cannot_be() {
        assert!(!can_be_overridden(&method("speak", &[]).stat(), THERE, HERE));
        assert!(!can_be_overridden(&method("speak", &[]).final_(), THERE, HERE));
    }

    /// A constructor and a static initialiser are members of the class FILE, not of the class.
    #[test]
    fn constructors_and_initialisers_are_not_methods_to_override() {
        assert!(!can_be_overridden(&method("<init>", &[]), THERE, HERE));
        assert!(!can_be_overridden(&method("<clinit>", &[]), THERE, HERE));
    }

    /// A private method is never inherited, so there is nothing to override.
    #[test]
    fn a_private_method_cannot_be() {
        let m = method("speak", &[]).vis(Visibility::Private);
        assert!(!can_be_overridden(&m, THERE, HERE));
    }

    /// Package-private crosses no package boundary — invisible there, so not overridable there.
    #[test]
    fn a_package_private_method_is_visible_only_in_its_own_package() {
        let m = method("speak", &[]).vis(Visibility::Package);
        assert!(can_be_overridden(&m, "com/acme/Base", "com/acme/Sub"));
        assert!(!can_be_overridden(&m, "com/acme/Base", "com/other/Sub"));
    }

    /// Overriding is defined on the name AND the parameter types — which is what leaves the
    /// overloads you have not written yet on offer.
    #[test]
    fn the_signature_key_separates_overloads() {
        let a = signature_key(&method("of", &["java/lang/String"]));
        let b = signature_key(&method("of", &["int"]));
        let c = signature_key(&method("of", &["java/lang/String"]));
        assert_ne!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn the_signature_key_separates_arities() {
        assert_ne!(signature_key(&method("of", &[])), signature_key(&method("of", &["int"])));
    }
}
