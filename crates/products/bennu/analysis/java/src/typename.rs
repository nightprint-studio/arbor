//! One reading of a **written type name**, for everyone who has to turn one into a binary name.
//!
//! There were three copies of this: the inference walk's (`infer::Ctx::simple_to_binary`), the
//! index build's (`bennu_intel::typemap`), and the member-model build's
//! (`bennu_intel::java_index::resolve_binary`). They agreed on the interesting part by accident and
//! drifted on the rest — and the part that mattered was wrong in all three the same way, so the
//! same bug had to be found and fixed three times:
//!
//! > `Outer.Nested` is a nested type named through its outer, NOT an already-qualified name.
//! > Slashing the text wholesale gives a binary name with **no package**, which resolves to
//! > nothing — silently, because "no members" is a normal answer everywhere downstream.
//!
//! What differs between the three callers is genuinely different: which imports they can see, which
//! map holds the project's types, whether they have a live resolver. What does NOT differ is the
//! *order* Java reads a name in, and the two spellings that have to be told apart. That order lives
//! here now; the sources stay with each caller, behind [`NameScope`].

/// Everything that can bind a **simple** type name in one place in the source.
///
/// Each caller answers from what it has — the file's imports, the type it is inside, the project's
/// type map, a live resolver — and [`resolve_written_type`] supplies the part they were all
/// re-implementing.
pub trait NameScope {
    /// The binary name a simple type name denotes here, or `None` when nothing binds it.
    fn simple(&self, name: &str) -> Option<String>;
    /// Whether `binary` names a type this scope can actually see. Decides `Outer.Nested` from
    /// `a.b.C`, so an over-eager `true` sends a package path down the nested branch.
    fn is_type(&self, binary: &str) -> bool;
}

/// The reading of a written type name: a binary name we actually bound, or the admission that
/// nothing in scope binds it.
///
/// This used to be a bare `String`, and the difference between the two was reconstructed
/// afterwards by looking at the string — "contains a slash, therefore resolved". That heuristic is
/// wrong on precisely the case that matters: an unbound `Outer.Nested` was slashed into
/// `Outer/Nested`, which contains a slash, so every caller believed it. A judgement built on a name
/// nothing declares is not conservative — it is a false positive with a plausible shape.
///
/// Making the two states a type moves the question to where the answer is known. A caller that
/// wants the old lossy behaviour still can (see [`TypeName::text`]), but it now says so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeName {
    /// A binary name something in scope bound: `p/Foo`, `java/util/Map$Entry`, `int`, `String[]`.
    Resolved(String),
    /// Nothing in scope bound this name. Carries the source text, for messages and for the callers
    /// whose job is to record that a name went unresolved.
    Unknown(String),
}

impl TypeName {
    /// The binary name, when this is a resolution. `None` is the whole point of the type: a caller
    /// that produces a diagnostic or an edit must have nothing to say here.
    pub fn resolved(self) -> Option<String> {
        match self {
            TypeName::Resolved(b) => Some(b),
            TypeName::Unknown(_) => None,
        }
    }

    /// The text either way — the binary name when bound, the source spelling when not.
    ///
    /// For callers that genuinely have a use for the unbound spelling: an error message, a cache
    /// key, a name-shaped comparison that does not assert the type exists.
    pub fn text(&self) -> &str {
        match self {
            TypeName::Resolved(b) | TypeName::Unknown(b) => b,
        }
    }

    pub fn is_resolved(&self) -> bool {
        matches!(self, TypeName::Resolved(_))
    }
}

/// Split a written type into its ELEMENT text and its array dimension count: `Class<?>[]` →
/// `("Class<?>", 1)`, `int[][]` → `("int", 2)`, `String` → `("String", 0)`.
///
/// The dimensions are written AFTER the type arguments, so anything that splits on `<` first loses
/// them — which is what happened: `Class<?>[]` went into the index as `Class`, indistinguishable
/// from its own element type, and an annotation whose element is declared `Class<?>[]` was judged
/// against a non-array `Class`.
pub fn split_array_dims(text: &str) -> (&str, usize) {
    let mut rest = text.trim();
    let mut dims = 0usize;
    while let Some(head) = rest.strip_suffix(']') {
        // `String [ ]` is legal Java; the whitespace is not part of either bracket.
        let Some(head) = head.trim_end().strip_suffix('[') else { break };
        rest = head.trim_end();
        dims += 1;
    }
    (rest, dims)
}

/// Strip every type-argument list from a written type, keeping the dotted structure:
/// `AbstractMultiset<E>.EntrySet` → `AbstractMultiset.EntrySet`, `Map<K, V>.Entry` → `Map.Entry`,
/// `Outer<A>.Inner<B>` → `Outer.Inner`.
///
/// Splitting at the FIRST `<` — which is what this replaced — throws away everything after the
/// matching `>`, and what lives there is the nested type being named. Guava writes
/// `class EntrySet extends AbstractMultiset<E>.EntrySet`, and the supertype came out as
/// `AbstractMultiset`: an abstract class with six abstract methods, none of which the subclass
/// declares, so all six were reported as unimplemented on a class that implements none of them
/// because it inherits them.
///
/// Borrows when there is nothing to strip, which is the overwhelming majority of type names.
pub fn erase_type_arguments(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains('<') {
        return std::borrow::Cow::Borrowed(text.trim());
    }
    let mut out = String::with_capacity(text.len());
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    std::borrow::Cow::Owned(out.trim().to_string())
}

/// Resolve a type as it is WRITTEN in source (`Map<String, Object>`, `Outer.Nested`, `int[]`) to a
/// binary name. Generic arguments are ignored — a caller that needs them parses them itself and
/// resolves each through here.
///
/// An array resolves to its ELEMENT's binary name plus the `[]` suffixes, which is the spelling
/// bytecode already produces (`java/util/List[]`). The written element used to be kept verbatim, so
/// a source `String[]` and a bytecode `java/lang/String[]` were the same type under two names that
/// never compared equal.
pub fn resolve_written_type(text: &str, scope: &dyn NameScope) -> TypeName {
    let (element, dims) = split_array_dims(text);
    if dims == 0 {
        return resolve_element(element, scope);
    }
    let suffix = "[]".repeat(dims);
    match resolve_element(element, scope) {
        TypeName::Resolved(b) => TypeName::Resolved(format!("{b}{suffix}")),
        // An array is a type whatever its element turns out to be, and every caller downstream only
        // ever tests the `[]` suffix — so an unresolved element must not turn the whole thing into
        // an unknown, or a varargs/array parameter stops being readable at all.
        TypeName::Unknown(t) => TypeName::Resolved(format!("{t}{suffix}")),
    }
}

fn resolve_element(text: &str, scope: &dyn NameScope) -> TypeName {
    let erased = erase_type_arguments(text);
    let base = erased.as_ref();
    if base.is_empty() {
        return TypeName::Unknown(String::new());
    }
    // A primitive is not a name to look up; it is already the answer.
    if is_primitive(base) {
        return TypeName::Resolved(base.to_string());
    }
    let Some((head, rest)) = base.split_once('.') else {
        return match scope.simple(base) {
            // Only the SPELLING is normalised here, never the verdict: a name bound to a type on no
            // classpath we can read stays resolved, it just cannot be confirmed.
            Some(b) => TypeName::Resolved(known_spelling(&b, scope).unwrap_or(b)),
            None => TypeName::Unknown(base.to_string()),
        };
    };
    // `Outer.Nested` vs `a.b.C`: the ONLY thing that tells them apart is whether the head is a name
    // this scope binds to a type. If it is, everything after it names types nested inside it — and
    // the binary must be built on the head's RESOLVED name, or it comes out with no package and
    // matches nothing. If it is not, the head is a package and the whole thing is already qualified.
    //
    // Asking the head is the whole test. Confirming the RESULT with `is_type` (which this used to
    // do) fails wherever the confirmation is not available: at index-build time it can only see the
    // project, so `Map.Entry` — a JDK type — fell through to `Map/Entry`, a name nothing declares.
    if let Some(head_binary) = scope.simple(head) {
        let joined = format!("{head_binary}/{}", rest.replace('.', "/"));
        return TypeName::Resolved(known_spelling(&joined, scope).unwrap_or(joined));
    }
    let slashed = base.replace('.', "/");
    if scope.is_type(&slashed) {
        return TypeName::Resolved(slashed);
    }
    // The head bound to nothing and the whole thing is not a type either. Two readings are left,
    // and they are told apart by where the class boundary falls: `a.b.C.Inner` is the package
    // `a.b`, the class `C`, and its member type. Find the longest prefix that IS a type and let the
    // remaining segments nest inside it — the same rule as above, applied one segment at a time
    // because the head alone did not answer.
    if let Some(found) = longest_type_prefix(&slashed, scope) {
        return TypeName::Resolved(found);
    }
    // Nothing bound any prefix. A LOWERCASE head is a package by every convention Java is written
    // in, so the slashed form is the best reading available and the resolver will say whether it
    // exists. An UPPERCASE head is a type this scope cannot see, and slashing it manufactures a
    // package-less binary that matches nothing — the shape that made every caller believe a guess.
    match head.chars().next() {
        Some(c) if c.is_uppercase() => TypeName::Unknown(base.to_string()),
        _ => TypeName::Resolved(slashed),
    }
}

/// Whether two binary names denote the SAME type, allowing for the two spellings a nested type has.
///
/// `java/util/Map$Entry` (from bytecode) and `java/util/Map/Entry` (from a source import) are one
/// type, and everything downstream compares them as strings. The resolver accepts both when you
/// *look a type up*, which is what hid this: two subsystems each got an answer, and only the
/// comparison between their answers was wrong. Guava's sorted maps produced 32 reports of a return
/// type being incompatible with itself.
///
/// Cheap on the common path: identical strings settle it without allocating, and only a name
/// carrying a `$` or a `/` needs the normalised comparison.
pub fn same_binary_type(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if !a.contains('$') && !b.contains('$') {
        return false;
    }
    a.replace('$', "/") == b.replace('$', "/")
}

/// The spelling of a nested type's binary name that this scope actually knows.
///
/// A nested type has TWO binary spellings and both are in circulation: `a/b/Outer/Inner` is what
/// source produces, `a/b/Outer$Inner` is what bytecode carries. A written name gives no clue which
/// of its dots are package separators and which are nesting — `import java.util.Map.Entry;` slashes
/// to `java/util/Map/Entry`, and the JDK's member index has never heard of it.
///
/// Nothing downstream can recover from that, because both spellings look equally like a resolution:
/// a check comparing a return type it resolved here against one the member index carries was
/// comparing `java/util/Map/Entry` with `java/util/Map$Entry` and reporting the same type as
/// incompatible with itself. Guava's sorted maps gave 32 of those.
///
/// Returns `None` when the scope knows no spelling — the caller then keeps what it had, because a
/// type on no classpath we can read is still a resolution, just not one we can confirm.
pub fn known_spelling(binary: &str, scope: &dyn NameScope) -> Option<String> {
    if scope.is_type(binary) {
        return Some(binary.to_string());
    }
    let segments: Vec<&str> = binary.split('/').collect();
    if segments.len() < 2 {
        return None;
    }
    // Longest package prefix first: the leftmost segments are far likelier to be the package, so
    // `a/b/C$D` is tried before `a/b$C$D`.
    for cut in (1..segments.len()).rev() {
        let candidate = format!(
            "{}/{}",
            segments[..cut].join("/"),
            segments[cut..].join("$")
        );
        if scope.is_type(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// The longest leading run of `a/b/C/D` that names a type, with the rest nested inside it.
///
/// Only reached when neither the head nor the whole name bound, so the extra lookups do not touch
/// the common cases: a simple name, an `Outer.Nested` whose outer is in scope, or a qualified name
/// the resolver knows outright all return before this.
fn longest_type_prefix(slashed: &str, scope: &dyn NameScope) -> Option<String> {
    let segments: Vec<&str> = slashed.split('/').collect();
    // Longest first: `a/b/C` before `a/b`, so a member type nests inside the innermost class that
    // actually exists rather than inside its package.
    for cut in (1..segments.len()).rev() {
        let prefix = segments[..cut].join("/");
        if !scope.is_type(&prefix) {
            continue;
        }
        // Source spelling and JVM spelling both occur — a project type comes from source, a
        // library one from bytecode.
        let rest = &segments[cut..];
        for sep in ['/', '$'] {
            let candidate = format!("{prefix}{sep}{}", rest.join(&sep.to_string()));
            if scope.is_type(&candidate) {
                return Some(candidate);
            }
        }
        return Some(format!("{prefix}/{}", rest.join("/")));
    }
    None
}

/// The primitives, which are their own binary names.
pub fn is_primitive(s: &str) -> bool {
    matches!(
        s,
        "int" | "long" | "short" | "byte" | "char" | "boolean" | "float" | "double" | "void"
    )
}

/// A member type this file INHERITS — `Entry` written bare inside a class that implements
/// `java.util.Map`.
///
/// JLS §8.1.5: a type's member types are inherited by its subtypes, so a nested type of any
/// supertype is in scope under its simple name with no import at all. It is how the JDK's own
/// collections are written, and how Guava writes 33 `Entry<K, V>` in one file — where, without
/// this, `Entry` fell through to whatever the project's flat simple-name map happened to hold and
/// every member read off it was reported missing.
///
/// Searched over the types THIS FILE declares, because that is the scope a bare name in it can
/// inherit from. `None` when nothing in reach declares one, which leaves the caller's other rules
/// to answer.
pub fn inherited_member_type(
    symbols: &crate::symbols::FileSymbols,
    resolver: &dyn crate::seam::TypeResolver,
    simple: &str,
) -> Option<String> {
    for td in &symbols.types {
        let start = td.fqn.replace('.', "/");
        if let Some(hit) = inherited_member_type_of(resolver, &start, simple) {
            return Some(hit);
        }
    }
    None
}

/// `simple` as a member type of `owner` or any of its supertypes — the owner-precise form of
/// [`inherited_member_type`], for a caller that knows which type the name was written inside.
pub fn inherited_member_type_of(
    resolver: &dyn crate::seam::TypeResolver,
    owner: &str,
    simple: &str,
) -> Option<String> {
    crate::hierarchy::walk_up(resolver, &crate::seam::TypeRef::simple(owner), |a| {
        // The source spelling (`p/Outer/Inner`) and the JVM one (`p/Outer$Inner`) both occur: a
        // project type comes from source, a JDK/library one from bytecode.
        let cur = &a.ty.binary_name;
        [format!("{cur}/{simple}"), format!("{cur}${simple}")]
            .into_iter()
            .find(|candidate| resolver.members_of(candidate).is_some())
    })
}

/// The type THIS FILE declares that `simple` denotes when written in `owner`'s scope.
///
/// Java reads a simple type name in the innermost scope that declares one (JLS §6.5.5.1), and the
/// scope of a member type is the **body** of its class (JLS §6.3). Searching a file's declarations
/// flat, by simple name — which every caller here used to do — gets both halves wrong at once:
///
///   * it lets a nested type answer for a name written OUTSIDE it. `class HashCodeBuilder implements
///     Builder<Integer>` declares its own nested `Builder`, and the `implements` clause sits in the
///     class HEADER, not its body — so javac binds it to the same-package `Builder` interface and
///     compiles, while the flat search bound it to the class. Six of commons-lang's classes are
///     written that way, and each was reported as implementing something that is not an interface,
///     taking its overrides and covariant returns down with it: thirteen of that project's
///     seventeen false positives, from one rule.
///   * it lets one nested class answer for a SIBLING's namesake, since neither is more "found"
///     than the other in a flat list.
///
/// `owner` is the innermost enclosing type as a binary name (`p/Outer/Inner`), or `None` for the
/// compilation unit's own scope — which is what a top-level type's header is read in. The climb
/// stops when the next prefix is no longer a type this file declares: what is above the outermost
/// type is the package, whose types are bound by a different rule at a lower precedence.
pub fn declared_type_in_scope(
    symbols: &crate::symbols::FileSymbols,
    owner: Option<&str>,
    simple: &str,
) -> Option<String> {
    let declared = |dotted: &str| symbols.types.iter().any(|t| t.fqn == dotted);
    let mut scope = owner.map(|o| o.replace('/', "."));
    while let Some(s) = scope {
        let candidate = format!("{s}.{simple}");
        if declared(&candidate) {
            return Some(candidate.replace('.', "/"));
        }
        scope = s.rfind('.').map(|i| s[..i].to_string()).filter(|p| declared(p));
    }
    // The compilation unit: a TOP-LEVEL type of this file. Top-level is exactly "no type declared
    // in this file encloses it", which is what makes this the last scope rather than another rung.
    symbols
        .types
        .iter()
        .find(|t| {
            t.name == simple
                && !t.is_anonymous
                && t.fqn.rsplit_once('.').is_none_or(|(outer, _)| !declared(outer))
        })
        .map(|t| t.fqn.replace('.', "/"))
}

/// Whether `binary` is a name we actually RESOLVED, as opposed to the raw token
/// [`resolve_written_type`] falls back to when nothing bound it.
///
/// Structural: a binary name carries its package (`p/Foo`, `java/util/List`), so one without a
/// separator is a type VARIABLE (`T`, and equally `Self`, `Param`) or a class on no classpath we can
/// read. Primitives and arrays are their own binary names. A type in the DEFAULT package looks the
/// same as an unresolved token, so that one case is settled by asking the resolver.
pub fn is_resolved_binary(binary: &str, resolver: &dyn crate::seam::TypeResolver) -> bool {
    if binary.is_empty() {
        return false;
    }
    if is_primitive(binary) || binary.ends_with("[]") || binary.contains('/') {
        return true;
    }
    resolver.members_of(binary).is_some()
}

/// Whether a compilation unit could name `binary` by the **simple name** `simple` — the scoping
/// question, asked of a resolution somebody already made.
///
/// A resolver answers "what would this name MEAN", and it answers generously on purpose: its index
/// is keyed by simple name, so `Foo` finds `com.acme.a.Foo` from anywhere in the project and
/// completion, hover and go-to all want that. javac asks a narrower question — is the name actually
/// **in scope here** (JLS §6.5.5, §7.5) — and it is the only one a "cannot resolve" may be built on.
/// Without it, a class used across a package boundary with no `import` read as perfectly fine in
/// Bennu and was rejected by the compiler, which is the failure a validator exists to prevent.
///
/// The test is written as "does any of the four things that put a simple name in scope produce this
/// exact binary", rather than by taking `binary` apart: `a/b/Outer/Inner` cannot be split into
/// package and class without already knowing the answer.
///
///   * a single-type import of it — `import a.b.Foo;`, and `import static a.b.Outer.Inner;`, the one
///     static form that binds a type;
///   * an import-on-demand of its package — `import a.b.*;`;
///   * the compilation unit's own package, `package` being its dotted name (`None` / empty = the
///     default package, where a bare `Foo` is a sibling);
///   * `java.lang`, imported for you.
///
/// What it deliberately does NOT cover: a type declared in the file, a type parameter, and a member
/// type inherited from a supertype (JLS §8.1.5). Those are in scope with no import and nothing to
/// import, and the caller must have excluded them already — this function would say `false` for all
/// three.
pub fn simple_name_reaches(
    binary: &str,
    simple: &str,
    package: Option<&str>,
    imports: &[crate::symbols::Import],
) -> bool {
    // Compared with `same_binary_type` because an import slashes to `a/b/Outer/Inner` while the
    // resolver may have handed back the bytecode spelling `a/b/Outer$Inner`.
    scope_candidates(simple, package, imports)
        .iter()
        .any(|c| same_binary_type(&c.binary, binary))
}

/// What a simple name MEANS in a compilation unit: the first of its [`scope_candidates`] that is
/// certain, or that `exists` confirms.
///
/// `None` is not "no such type" — only "nothing in this file's scope binds it". A caller that wants
/// a project-wide guess after that makes it knowingly, and after this, never instead of it.
///
/// `exists` is whatever the caller can see — a resolver's `members_of`, a class index. It is asked
/// in precedence order and stops at the first yes, so a name the file imports explicitly costs no
/// lookup at all.
pub fn bind_simple_name(
    simple: &str,
    package: Option<&str>,
    imports: &[crate::symbols::Import],
    exists: &dyn Fn(&str) -> bool,
) -> Option<String> {
    scope_candidates(simple, package, imports)
        .into_iter()
        .find(|c| c.confirmed_by(exists))
        .map(|c| c.binary)
}

/// Which of Java's scoping routes a [`ScopeCandidate`] came from, in precedence order.
///
/// Public so a caller that cannot use every tier can say which it leaves out, rather than writing
/// its own loop over the imports: `IndexResolver` is not told the file's package and drops
/// [`ScopeKind::OwnPackage`], and splits the rest between the project index and the classpath.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    /// `import a.b.Foo;`
    SingleImport,
    /// A type of the file's own package — or of the default package, for a file with none.
    OwnPackage,
    /// `import static a.b.Outer.Inner;` — a type only when the path really is one.
    StaticImport,
    /// `import a.b.*;`
    OnDemand,
    /// `import static a.b.Outer.*;` — `Outer`'s static member types (JLS §7.5.4).
    StaticOnDemand,
    /// `java.lang`, imported for you.
    JavaLang,
}

/// One way a simple name can come into scope — see [`scope_candidates`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeCandidate {
    pub binary: String,
    pub kind: ScopeKind,
}

impl ScopeCandidate {
    /// Bound whether or not anything can confirm the type is there.
    ///
    /// A single-type import says what the name means, and a class that does not exist is the
    /// import's own error, reported against the import — not a reason to bind the name to something
    /// else. The curated `java.lang` names are certain too, so a caller whose view cannot read the
    /// JDK still binds `String` to what it is.
    pub fn is_certain(&self) -> bool {
        match self.kind {
            ScopeKind::SingleImport => true,
            ScopeKind::JavaLang => self
                .binary
                .strip_prefix("java/lang/")
                .is_some_and(|simple| java_lang_implicit(simple).is_some()),
            _ => false,
        }
    }

    /// Whether this candidate binds, given what `exists` can see.
    ///
    /// A static import-on-demand names a TYPE and imports its members, so the path has to be a type
    /// before a member type is looked for inside it: `import static java.util.*;` names nothing, and
    /// reading `java/util/LinkedHashMap` out of it would bind a class through an import that does
    /// not exist.
    pub fn confirmed_by(&self, exists: &dyn Fn(&str) -> bool) -> bool {
        if self.is_certain() {
            return true;
        }
        match self.kind {
            ScopeKind::StaticOnDemand => {
                self.binary.rsplit_once('/').is_some_and(|(owner, _)| exists(owner))
                    && exists(&self.binary)
            }
            _ => exists(&self.binary),
        }
    }
}

/// Every binary a simple name COULD denote in a compilation unit, in the precedence Java gives them
/// (JLS §6.4.1): a single-type import shadows the package's own types, which shadow every
/// import-on-demand, `java.lang` included.
///
/// The ONE statement of that rule: [`simple_name_reaches`] asks whether a binary is among these,
/// [`bind_simple_name`] takes the first one that exists. It used to be written out wherever it was
/// needed — the validator, go-to into a library, the import intention, the Struts property
/// navigator — and the copies did not agree. Go-to asked the project-wide simple-name map before the
/// file's own wildcards, so a star-imported library type opened nothing whenever the project
/// declared a class of the same simple name in any package at all.
pub fn scope_candidates(
    simple: &str,
    package: Option<&str>,
    imports: &[crate::symbols::Import],
) -> Vec<ScopeCandidate> {
    let slashed = |dotted: &str| dotted.replace('.', "/");
    let mut out = Vec::new();
    // `import a.b.Foo;` — the path IS the binary.
    for imp in imports.iter().filter(|i| !i.star && !i.static_) {
        if imp.simple_name() == Some(simple) {
            out.push(ScopeCandidate { binary: slashed(&imp.path), kind: ScopeKind::SingleImport });
        }
    }
    // The file's own package — or the default package, where a sibling has no prefix at all.
    let own = match package.map(str::trim).filter(|p| !p.is_empty()) {
        Some(pkg) => format!("{}/{simple}", slashed(pkg)),
        None => simple.to_string(),
    };
    out.push(ScopeCandidate { binary: own, kind: ScopeKind::OwnPackage });
    // `import static a.b.Outer.Inner;` — the one static form that binds a TYPE, and only when the
    // path really is one: it is just as often a method or a constant.
    for imp in imports.iter().filter(|i| !i.star && i.static_) {
        if imp.simple_name() == Some(simple) {
            out.push(ScopeCandidate { binary: slashed(&imp.path), kind: ScopeKind::StaticImport });
        }
    }
    // `import a.b.*;`, then `import static a.b.Outer.*;`.
    for (static_, kind) in [(false, ScopeKind::OnDemand), (true, ScopeKind::StaticOnDemand)] {
        for imp in imports.iter().filter(|i| i.star && i.static_ == static_) {
            out.push(ScopeCandidate { binary: format!("{}/{simple}", slashed(&imp.path)), kind });
        }
    }
    out.push(ScopeCandidate { binary: format!("java/lang/{simple}"), kind: ScopeKind::JavaLang });
    out
}

/// The `java.lang` types that are implicitly imported (JLS §7.3), as a binary name.
///
/// A curated set, not the whole package: a bare name that is NOT here stays unresolved rather than
/// being mapped to a `java/lang/…` that may not exist. Every caller checks its own project's types
/// FIRST, so a project class named `Exception` still wins.
///
/// One list, because three callers each had their own and they disagreed — so whether
/// `throws IllegalStateException` resolved depended on which of them asked.
pub fn java_lang_implicit(name: &str) -> Option<String> {
    const NAMES: &[&str] = &[
        // The ubiquitous value / wrapper types.
        "String",
        "Object",
        "Integer",
        "Long",
        "Boolean",
        "Double",
        "Float",
        "Character",
        "Byte",
        "Short",
        "Number",
        "CharSequence",
        "StringBuilder",
        "StringBuffer",
        "Math",
        "System",
        "Enum",
        "Record",
        "Class",
        "Thread",
        "Iterable",
        "Comparable",
        "Runnable",
        "Cloneable",
        "AutoCloseable",
        "Void",
        // The throwable hierarchy — what a `throws` clause and a `catch` are made of.
        "Throwable",
        "Exception",
        "Error",
        "RuntimeException",
        "InterruptedException",
        "ClassNotFoundException",
        "CloneNotSupportedException",
        "NoSuchMethodException",
        "NoSuchFieldException",
        "IllegalAccessException",
        "InstantiationException",
        "ReflectiveOperationException",
        "NullPointerException",
        "IllegalArgumentException",
        "IllegalStateException",
        "IndexOutOfBoundsException",
        "ArrayIndexOutOfBoundsException",
        "StringIndexOutOfBoundsException",
        "ClassCastException",
        "NumberFormatException",
        "UnsupportedOperationException",
        "ArithmeticException",
        "NegativeArraySizeException",
        "ArrayStoreException",
        "SecurityException",
        "IllegalMonitorStateException",
        "IllegalThreadStateException",
        "EnumConstantNotPresentException",
        "TypeNotPresentException",
        "AssertionError",
        "StackOverflowError",
        "OutOfMemoryError",
        "NoClassDefFoundError",
        "ExceptionInInitializerError",
        "LinkageError",
        "VirtualMachineError",
    ];
    NAMES.contains(&name).then(|| format!("java/lang/{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scope;
    impl NameScope for Scope {
        fn simple(&self, name: &str) -> Option<String> {
            match name {
                "Outer" => Some("p/Outer".to_string()),
                "Payload" => Some("p/Payload".to_string()),
                _ => None,
            }
        }
        fn is_type(&self, binary: &str) -> bool {
            matches!(
                binary,
                "p/Outer"
                    | "p/Payload"
                    | "p/Outer/Nested"
                    | "p/Outer/Nested/Deep"
                    | "q/Host"
                    | "q/Host$Inner"
                    | "r/Thing"
                    | "r/Thing/Part"
            )
        }
    }

    fn bin(text: &str) -> Option<String> {
        resolve_written_type(text, &Scope).resolved()
    }

    #[test]
    fn a_simple_name_goes_through_the_scope() {
        assert_eq!(bin("Payload").as_deref(), Some("p/Payload"));
    }

    #[test]
    fn a_nested_type_keeps_its_package() {
        assert_eq!(bin("Outer.Nested").as_deref(), Some("p/Outer/Nested"));
        assert_eq!(
            bin("Outer.Nested.Deep").as_deref(),
            Some("p/Outer/Nested/Deep")
        );
    }

    #[test]
    fn a_qualified_name_is_slashed() {
        assert_eq!(bin("java.util.List").as_deref(), Some("java/util/List"));
    }

    #[test]
    fn generics_are_dropped_and_arrays_and_primitives_pass_through() {
        assert_eq!(
            bin("Outer.Nested<String>").as_deref(),
            Some("p/Outer/Nested")
        );
        assert_eq!(bin("int").as_deref(), Some("int"));
        assert_eq!(bin("String[]").as_deref(), Some("String[]"));
    }

    /// The state that did not exist. An unbound simple name used to come back as the raw token,
    /// and every caller re-derived "was that a resolution?" from the shape of the string.
    #[test]
    fn an_unbound_simple_name_is_unknown_and_keeps_its_text() {
        let t = resolve_written_type("Mystery", &Scope);
        assert!(!t.is_resolved());
        assert_eq!(t.text(), "Mystery");
        assert_eq!(t.resolved(), None);
    }

    /// The case the old string heuristic got exactly backwards: `Outer/Nested` contains a slash,
    /// so "contains a slash therefore resolved" believed a name nothing declares.
    #[test]
    fn an_unbound_nested_name_is_unknown_rather_than_a_packageless_binary() {
        let t = resolve_written_type("Absent.Nested", &Scope);
        assert!(!t.is_resolved());
        assert_eq!(t.text(), "Absent.Nested");
    }

    /// A lowercase head is a package by every convention Java is written in, so a qualified name
    /// we cannot confirm is still the best reading — the resolver says whether it exists.
    #[test]
    fn an_unconfirmable_qualified_name_still_reads_as_a_package_path() {
        assert_eq!(bin("com.absent.Thing").as_deref(), Some("com/absent/Thing"));
    }

    /// One type, two spellings, and everything downstream compares strings.
    #[test]
    fn the_two_spellings_of_a_nested_type_are_the_same_type() {
        assert!(same_binary_type(
            "java/util/Map$Entry",
            "java/util/Map/Entry"
        ));
        assert!(same_binary_type("p/A", "p/A"));
        assert!(!same_binary_type("java/util/Map$Entry", "p/Multiset$Entry"));
        assert!(!same_binary_type("p/A", "p/B"));
    }

    /// The same type has two binary spellings, and an import path gives no clue which dots are
    /// package separators. The scope knows `q/Host$Inner`; slashing the import gives
    /// `q/Host/Inner`, and both look equally like a resolution to everything downstream.
    #[test]
    fn a_nested_type_takes_the_spelling_the_scope_knows() {
        assert_eq!(
            known_spelling("q/Host/Inner", &Scope).as_deref(),
            Some("q/Host$Inner")
        );
        // A spelling the scope already knows is left exactly as it is.
        assert_eq!(
            known_spelling("p/Outer/Nested", &Scope).as_deref(),
            Some("p/Outer/Nested")
        );
        // Nothing known under any spelling: the caller keeps what it had.
        assert_eq!(known_spelling("x/Y/Z", &Scope), None);
    }

    /// `q.Host.Inner` is a package, a class, and its member type — the class boundary is not after
    /// the first segment, so asking only the head could never find it.
    #[test]
    fn the_class_boundary_is_found_anywhere_in_a_qualified_name() {
        assert_eq!(bin("q.Host.Inner").as_deref(), Some("q/Host$Inner"));
        assert_eq!(bin("r.Thing.Part").as_deref(), Some("r/Thing/Part"));
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;
    use crate::symbols::Import;

    fn imp(path: &str, star: bool, static_: bool) -> Import {
        Import { span: None, path: path.to_string(), star, static_ }
    }

    fn reaches(binary: &str, simple: &str, package: Option<&str>, imports: &[Import]) -> bool {
        simple_name_reaches(binary, simple, package, imports)
    }

    #[test]
    fn a_single_type_import_puts_the_name_in_scope() {
        let imports = [imp("com.acme.util.Helper", false, false)];
        assert!(reaches("com/acme/util/Helper", "Helper", Some("com.acme.web"), &imports));
        // …and only that name: the import says nothing about a different type of the same package.
        assert!(!reaches("com/acme/util/Other", "Other", Some("com.acme.web"), &imports));
    }

    #[test]
    fn a_nested_type_reaches_under_either_spelling() {
        // The import slashes to `a/b/Outer/Inner`; bytecode carries `a/b/Outer$Inner`.
        let imports = [imp("a.b.Outer.Inner", false, false)];
        assert!(reaches("a/b/Outer$Inner", "Inner", Some("p"), &imports));
        assert!(reaches("a/b/Outer/Inner", "Inner", Some("p"), &imports));
    }

    #[test]
    fn an_import_on_demand_covers_its_whole_package() {
        let imports = [imp("com.acme.util", true, false)];
        assert!(reaches("com/acme/util/Helper", "Helper", Some("com.acme.web"), &imports));
        assert!(reaches("com/acme/util/Other", "Other", Some("com.acme.web"), &imports));
        // A package it does not name is still out of scope.
        assert!(!reaches("com/acme/dao/Helper", "Helper", Some("com.acme.web"), &imports));
        // A static-import-on-demand brings member types in too (JLS §7.5.4).
        let statics = [imp("a.b.Outer", true, true)];
        assert!(reaches("a/b/Outer$Inner", "Inner", Some("p"), &statics));
    }

    #[test]
    fn the_files_own_package_needs_no_import() {
        assert!(reaches("com/acme/Sibling", "Sibling", Some("com.acme"), &[]));
        assert!(!reaches("com/acme/util/Sibling", "Sibling", Some("com.acme"), &[]));
        // The default package: no prefix at all, and a packaged type is NOT reachable from it.
        assert!(reaches("Sibling", "Sibling", None, &[]));
        assert!(reaches("Sibling", "Sibling", Some(""), &[]));
        assert!(!reaches("com/acme/Sibling", "Sibling", None, &[]));
    }

    #[test]
    fn java_lang_is_in_scope_and_nothing_else_of_the_jdk_is() {
        assert!(reaches("java/lang/String", "String", Some("com.acme"), &[]));
        assert!(!reaches("java/util/Set", "Set", Some("com.acme"), &[]));
        assert!(reaches(
            "java/util/Set",
            "Set",
            Some("com.acme"),
            &[imp("java.util.Set", false, false)]
        ));
    }

    // ── bind_simple_name: the same candidates, taken in precedence order ────────────────────

    fn bind(simple: &str, package: Option<&str>, imports: &[Import], known: &[&str]) -> Option<String> {
        bind_simple_name(simple, package, imports, &|b| known.contains(&b))
    }

    /// The go-to that broke: a wildcard-imported library type binds even when the project declares a
    /// class of the same simple name somewhere else — that class is simply not in scope here.
    #[test]
    fn a_star_import_binds_what_its_package_holds() {
        let imports = [imp("org.springframework.stereotype", true, false)];
        let known = ["org/springframework/stereotype/Service", "com/acme/other/Service"];
        assert_eq!(
            bind("Service", Some("com.acme.web"), &imports, &known).as_deref(),
            Some("org/springframework/stereotype/Service"),
        );
    }

    #[test]
    fn precedence_is_import_then_package_then_on_demand() {
        let known = ["com/acme/web/Riga", "com/acme/dto/Riga", "com/lib/Riga"];
        // The package beats a wildcard (JLS §6.4.1)…
        let star_only = [imp("com.lib", true, false)];
        assert_eq!(
            bind("Riga", Some("com.acme.web"), &star_only, &known).as_deref(),
            Some("com/acme/web/Riga"),
        );
        // …and a single-type import beats the package.
        let with_single = [imp("com.lib", true, false), imp("com.acme.dto.Riga", false, false)];
        assert_eq!(
            bind("Riga", Some("com.acme.web"), &with_single, &known).as_deref(),
            Some("com/acme/dto/Riga"),
        );
    }

    /// An explicit import says what the name means whether or not anything can confirm the class.
    #[test]
    fn a_single_type_import_binds_without_asking() {
        let imports = [imp("org.absent.Gone", false, false)];
        assert_eq!(bind("Gone", Some("p"), &imports, &[]).as_deref(), Some("org/absent/Gone"));
    }

    /// From the library-view go-to this replaced: a sibling in the buffer's own package needs no
    /// import, and a name the classpath does not have is never invented.
    #[test]
    fn a_sibling_binds_only_when_it_is_there() {
        let known = ["org/springframework/context/MessageSource"];
        let ctx = Some("org.springframework.context");
        assert_eq!(
            bind("MessageSource", ctx, &[], &known).as_deref(),
            Some("org/springframework/context/MessageSource"),
        );
        assert_eq!(bind("Nonexistent", ctx, &[], &known), None);
        assert_eq!(bind("MessageSource", Some("com.acme"), &[], &known), None);
        assert_eq!(bind("MessageSource", None, &[], &known), None);
        assert_eq!(bind("MessageSource", Some(""), &[], &known), None);
    }

    /// A static import-on-demand names a TYPE; one naming a package imports nothing.
    #[test]
    fn a_static_wildcard_binds_only_through_a_type() {
        let known = ["a/b/Outer", "a/b/Outer/Inner", "java/util/LinkedHashMap"];
        let through_type = [imp("a.b.Outer", true, true)];
        assert_eq!(bind("Inner", Some("p"), &through_type, &known).as_deref(), Some("a/b/Outer/Inner"));
        let through_package = [imp("java.util", true, true)];
        assert_eq!(bind("LinkedHashMap", Some("p"), &through_package, &known), None);
    }

    #[test]
    fn java_lang_binds_even_when_the_jdk_cannot_be_read() {
        assert_eq!(bind("String", Some("p"), &[], &[]).as_deref(), Some("java/lang/String"));
        // …but a class of that name in the file's own package shadows it.
        assert_eq!(bind("String", Some("p"), &[], &["p/String"]).as_deref(), Some("p/String"));
    }
}

#[cfg(test)]
mod inherited_tests {
    use super::*;
    use crate::seam::{ClassMembers, TypeResolver};
    use std::collections::HashMap;
    use std::sync::Arc;

    struct R(HashMap<String, ClassMembers>);
    impl TypeResolver for R {
        fn members_of(&self, b: &str) -> Option<Arc<ClassMembers>> {
            self.0.get(b).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, _n: &str, _i: &[crate::symbols::Import]) -> Option<String> {
            None
        }
        fn is_project_type(&self, b: &str) -> bool {
            self.0.contains_key(b)
        }
    }

    fn cm(superclass: Option<&str>, interfaces: &[&str]) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: superclass.map(crate::seam::TypeRef::simple),
            interfaces: interfaces.iter().map(|s| crate::seam::TypeRef::simple(*s)).collect(),
            methods: Vec::new(),
            fields: Vec::new(),
            flags: Default::default(),
        }
    }

    /// `Entry` written bare inside a class that implements `java.util.Map` — the JDK spells its
    /// nested type with `$`, a project one with `/`, and both have to be found.
    #[test]
    fn a_member_type_of_a_supertype_is_in_scope() {
        let mut m = HashMap::new();
        m.insert("p/MyMap".to_string(), cm(None, &["java/util/Map"]));
        m.insert("java/util/Map".to_string(), cm(None, &[]));
        m.insert("java/util/Map$Entry".to_string(), cm(None, &[]));
        let r = R(m);
        let symbols = crate::symbols::extract_symbols("package p;\nclass MyMap {}\n");
        assert_eq!(
            inherited_member_type(&symbols, &r, "Entry").as_deref(),
            Some("java/util/Map$Entry")
        );
        assert_eq!(inherited_member_type(&symbols, &r, "Nope"), None);
    }

    #[test]
    fn the_walk_reaches_through_a_superclass_chain_and_stops_on_a_cycle() {
        let mut m = HashMap::new();
        m.insert("p/A".to_string(), cm(Some("p/B"), &[]));
        m.insert("p/B".to_string(), cm(Some("p/A"), &[])); // a cycle
        m.insert("p/B/Helper".to_string(), cm(None, &[]));
        let r = R(m);
        let symbols = crate::symbols::extract_symbols("package p;\nclass A {}\n");
        assert_eq!(
            inherited_member_type(&symbols, &r, "Helper").as_deref(),
            Some("p/B/Helper")
        );
    }
}

    /// The array dimensions come after the type arguments, so a reader that splits on `<` first
    /// loses them — `Class<?>[]` went into the index as `Class`.
    #[test]
    fn array_dimensions_survive_a_generic_argument_list() {
        assert_eq!(split_array_dims("Class<?>[]"), ("Class<?>", 1));
        assert_eq!(split_array_dims("Map<String, int[]>[][]"), ("Map<String, int[]>", 2));
        assert_eq!(split_array_dims("int[][]"), ("int", 2));
        assert_eq!(split_array_dims("String [ ]"), ("String", 1));
        assert_eq!(split_array_dims("String"), ("String", 0));
        assert_eq!(split_array_dims("List<int[]>"), ("List<int[]>", 0));
    }

    /// A nested type named through a PARAMETERISED qualifier. Splitting at the first `<` threw the
    /// nested name away, so guava's `extends AbstractMultiset<E>.EntrySet` resolved to
    /// `AbstractMultiset` — and its six abstract methods were all reported as unimplemented.
    #[test]
    fn a_type_argument_list_does_not_swallow_the_nested_name() {
        assert_eq!(erase_type_arguments("AbstractMultiset<E>.EntrySet"), "AbstractMultiset.EntrySet");
        assert_eq!(erase_type_arguments("Map<K, V>.Entry"), "Map.Entry");
        assert_eq!(erase_type_arguments("Outer<A>.Inner<B>"), "Outer.Inner");
        assert_eq!(erase_type_arguments("List<Map<K, V>>"), "List");
        assert_eq!(erase_type_arguments("String"), "String");
    }

    /// An array resolves to its ELEMENT's binary plus the suffix — the spelling bytecode already
    /// uses, so a source `String[]` and a bytecode `java/lang/String[]` finally compare equal.
    #[test]
    fn an_array_resolves_its_element() {
        struct Scope;
        impl NameScope for Scope {
            fn simple(&self, name: &str) -> Option<String> {
                match name {
                    "String" => Some("java/lang/String".into()),
                    "Class" => Some("java/lang/Class".into()),
                    _ => None,
                }
            }
            fn is_type(&self, _binary: &str) -> bool {
                false
            }
        }
        assert_eq!(
            resolve_written_type("Class<?>[]", &Scope).text(),
            "java/lang/Class[]"
        );
        assert_eq!(
            resolve_written_type("String[]", &Scope).text(),
            "java/lang/String[]"
        );
        assert_eq!(resolve_written_type("int[][]", &Scope).text(), "int[][]");
        // An element nothing binds still leaves an ARRAY: every caller downstream only tests the
        // suffix, and answering `Unknown` here would make a varargs parameter unreadable.
        let t = resolve_written_type("T[]", &Scope);
        assert!(t.is_resolved() && t.text() == "T[]", "{t:?}");
    }
