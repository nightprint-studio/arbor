//! Resolver-backed try/catch diagnostics. Three checks, all `"error"`:
//!
//!   * **unreachable catch** — in a `try` with multiple `catch` clauses, a later catch whose
//!     exception type is the same as, or a subtype of, an EARLIER catch's type can never run
//!     (`catch (Exception) … catch (IOException)`);
//!   * **redundant multi-catch** — within ONE `catch (A | B e)`, an alternative that is a subtype
//!     of another alternative (`catch (IOException | Exception e)`);
//!   * **non-AutoCloseable resource** — a `try (T r = …)` whose resource type `T` doesn't implement
//!     `java.lang.AutoCloseable` (nor `java.io.Closeable`, which extends it).
//!
//! Soundness (docs: NEVER a false positive). Every positive conclusion is drawn only over a
//! FULLY-KNOWN hierarchy:
//!
//!   * the subtype tests (checks 1 & 2) call [`reaches`] — which is conservative and short-circuits
//!     an *unknown* class to `true` — but gate it behind [`hierarchy_fully_known`] on the SUBTYPE
//!     candidate. Only when the candidate's whole hierarchy is resolvable can a `true` from `reaches`
//!     be trusted as a real subtype relation (no unknown link could have manufactured it). If either
//!     type is unresolvable, or the candidate's hierarchy has a gap, we SKIP;
//!   * check 3 only flags when the resource type resolves, its ENTIRE supertype hierarchy is known,
//!     and `AutoCloseable`/`Closeable` is DEFINITIVELY absent from it. An unknown supertype might be
//!     `AutoCloseable`, so any hierarchy gap → SKIP. The bare Java-9 form `try (r)` (a pre-existing
//!     variable, no written type) carries no declared type here → SKIP.

use bennu_java::prelude::{FileSymbols, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::check_id::CheckId;
use crate::members::simple_name;
use crate::resolve::type_binary;
use crate::walk::{hierarchy_fully_known, reaches};

/// `java.lang.AutoCloseable` in binary form — the interface a try-with-resources resource must have.
const AUTO_CLOSEABLE: &str = "java/lang/AutoCloseable";
/// `java.io.Closeable` extends `AutoCloseable`; accepted directly so a resolver missing the
/// `Closeable → AutoCloseable` edge (or seeded only with `Closeable`) still validates a `Closeable`.
const CLOSEABLE: &str = "java/io/Closeable";

/// Parse `source` and flag try/catch exception errors.
pub fn exception_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        return Vec::new();
    };
    let symbols = bennu_java::prelude::extract_symbols(source);
    let nodes = crate::check::collect_nodes(tree.root_node());
    exception_errors_in(&nodes, source, &symbols, resolver)
}

/// Tree-driven core: iterates the shared `nodes` + reuses the caller's `symbols`.
pub fn exception_errors_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            // Both grammar spellings for a `try`: a plain `try` (may still carry a
            // `resource_specification`) and a dedicated `try_with_resources_statement`.
            "try_statement" | "try_with_resources_statement" => {
                check_catch_clauses(n, bytes, symbols, resolver, &mut out);
                check_resources(n, bytes, symbols, resolver, &mut out);
            }
            _ => {}
        }
    }
    out
}

// ── checks 1 & 2: catch-clause ordering / multi-catch redundancy ─────────────────────────────────

/// A single catch alternative: its binary name + the type node (for the diagnostic span). The
/// message uses `simple_name(&binary)`, so no written text is retained.
struct CatchType<'t> {
    binary: String,
    node: Node<'t>,
}

/// Walk a `try`'s direct `catch_clause` children, resolve each clause's alternative types, and flag:
///   * (check 2) a multi-catch alternative that is a subtype of another alternative in the SAME clause;
///   * (check 1) a clause alternative already caught by (== or subtype of) an EARLIER clause's type.
fn check_catch_clauses(
    try_node: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    // Types from all earlier clauses, in source order, that RESOLVED (unresolved ones can't shadow
    // — an unknown type might be unrelated, so we never treat it as a catch-all).
    let mut earlier: Vec<CatchType> = Vec::new();

    let mut c = try_node.walk();
    for clause in try_node.named_children(&mut c) {
        if clause.kind() != "catch_clause" {
            continue;
        }
        let alts = clause_types(clause, bytes, symbols, resolver);

        // Check 2: within this one clause, a later alternative that is a subtype of an earlier one.
        for i in 0..alts.len() {
            for j in 0..alts.len() {
                if i == j {
                    continue;
                }
                // alts[i] a PROPER subtype of alts[j] → listing both is redundant, whatever their
                // order in the clause (redundancy is symmetric). Report on the subtype alts[i].
                // Identical types are a different error (a duplicate alternative), so equal binaries
                // are deliberately left alone here.
                if alts[i].binary != alts[j].binary
                    && is_confirmed_subtype(resolver, &alts[i].binary, &alts[j].binary)
                {
                    out.push(CheckId::RedundantMultiCatch.at(
                        alts[i].node,
                        format!(
                            "Multi-catch cannot list `{}` and its supertype `{}` together",
                            simple_name(&alts[i].binary),
                            simple_name(&alts[j].binary),
                        ),
                    ));
                    break;
                }
            }
        }

        // Check 1: each alternative vs every earlier clause's alternatives.
        for alt in &alts {
            for prev in &earlier {
                if is_confirmed_subtype(resolver, &alt.binary, &prev.binary) {
                    out.push(CheckId::UnreachableCatch.at(
                        alt.node,
                        format!(
                            "Unreachable catch: `{}` is already caught by `{}` above",
                            simple_name(&alt.binary),
                            simple_name(&prev.binary),
                        ),
                    ));
                    break; // one shadowing clause is enough
                }
            }
        }
        // This clause's resolved types become "earlier" for the clauses that follow.
        earlier.extend(alts);
    }
}

/// Whether `sub` is CERTAINLY the same as or a subtype of `sup`. `reaches` is conservative (an
/// unknown class short-circuits to `true`), so a bare `reaches` could over-report; we require `sub`'s
/// whole hierarchy to be known so a `true` can't have come from an unknown link — only then is the
/// relation real. Anything less → `false` (SKIP the diagnostic).
fn is_confirmed_subtype(resolver: &dyn TypeResolver, sub: &str, sup: &str) -> bool {
    if sub == sup {
        return true;
    }
    hierarchy_fully_known(resolver, sub) && reaches(resolver, sub, sup)
}

/// The RESOLVED alternative types of one `catch_clause`. A `catch_clause` wraps a
/// `catch_formal_parameter` whose `catch_type` lists 1+ types separated by `|`. Alternatives whose
/// type doesn't resolve are dropped (we can't reason about them → they never shadow / are shadowed).
fn clause_types<'t>(
    clause: Node<'t>,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<CatchType<'t>> {
    let mut out = Vec::new();
    let Some(param) = child_of_kind(clause, "catch_formal_parameter") else { return out };
    let Some(catch_type) = child_of_kind(param, "catch_type") else { return out };
    let mut c = catch_type.walk();
    for ty in catch_type.named_children(&mut c) {
        if !is_type_node(ty.kind()) {
            continue;
        }
        let Ok(text) = ty.utf8_text(bytes) else { continue };
        // Unresolvable → skip this alternative (soundness: an unknown type is not reasoned about).
        if let Some(binary) = type_binary(text, symbols, resolver) {
            out.push(CatchType { binary, node: ty });
        }
    }
    out
}

// ── check 3: try-with-resources resource must be AutoCloseable ────────────────────────────────────

/// For each `resource` under the try's `resource_specification`, flag a resource whose DECLARED type
/// is fully known and definitively not `AutoCloseable`/`Closeable`.
fn check_resources(
    try_node: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    let Some(spec) = child_of_kind(try_node, "resource_specification") else { return };
    let mut c = spec.walk();
    for res in spec.named_children(&mut c) {
        if res.kind() != "resource" {
            continue;
        }
        // A declared resource `T r = …` exposes a `type` field. The bare Java-9 form `try (r)` (a
        // pre-existing variable) has NO `type` — we'd need to infer the variable's type, and any
        // uncertainty there is a false-positive risk, so we SKIP it (sound under-report).
        let Some(type_node) = res.child_by_field_name("type") else { continue };
        let Ok(text) = type_node.utf8_text(bytes) else { continue };
        // A `var` / Lombok `val` resource has no written type — its type comes from the initializer,
        // which this check doesn't infer → SKIP. Crucially this must happen BEFORE resolution: with
        // `import lombok.val;` in the file, `val` resolves to the real `lombok.val` type (not
        // AutoCloseable), which produced the false "resource type `val` must implement AutoCloseable".
        if text == "var" || text == "val" {
            continue;
        }
        // Unresolvable type → SKIP (it might well be AutoCloseable).
        let Some(binary) = type_binary(text, symbols, resolver) else { continue };
        // Only flag when the WHOLE hierarchy is known AND AutoCloseable/Closeable is definitively
        // absent. A gap anywhere means an unknown supertype could be AutoCloseable → SKIP.
        if !hierarchy_fully_known(resolver, &binary) {
            continue;
        }
        let closeable = reaches(resolver, &binary, AUTO_CLOSEABLE) || reaches(resolver, &binary, CLOSEABLE);
        if !closeable {
            out.push(CheckId::NonAutoCloseableResource.at(
                type_node,
                format!("The resource type `{}` must implement `AutoCloseable`", simple_name(&binary)),
            ));
        }
    }
}

// ── CST helpers ──────────────────────────────────────────────────────────────────────────────────

/// The first direct named child of `n` with the given kind.
fn child_of_kind<'t>(n: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut c = n.walk();
    for ch in n.named_children(&mut c) {
        if ch.kind() == kind {
            return Some(ch);
        }
    }
    None
}

/// A type node that names a class/interface (a catch alternative or a resource type).
fn is_type_node(kind: &str) -> bool {
    matches!(kind, "type_identifier" | "scoped_type_identifier" | "generic_type")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import};
    use std::collections::HashMap;
    use std::sync::Arc;

    /// The same mock-resolver shape the inheritance / member tests use: a `binary → members` map + a
    /// `simple → binary` table.
    struct MapResolver {
        members: HashMap<String, ClassMembers>,
        simple: HashMap<String, String>,
    }

    impl TypeResolver for MapResolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.members.get(binary).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
            self.simple.get(name).cloned()
        }
    }

    fn cm(superclass: Option<&str>, ifaces: &[&str], is_interface: bool) -> ClassMembers {
        let flags = ClassFlags { is_interface, ..ClassFlags::default() };
        ClassMembers {
            type_params: Vec::new(),
            superclass: superclass.map(str::to_string),
            interfaces: ifaces.iter().map(|s| s.to_string()).collect(),
            methods: Vec::new(),
            fields: Vec::new(),
            flags,
        }
    }

    /// A small, fully-known hierarchy:
    ///   Object; Throwable ← Exception ← IOException; Exception ← SQLException (sibling of IOException);
    ///   AutoCloseable (iface); Closeable (iface) extends AutoCloseable; FileInputStream implements
    ///   Closeable; PlainThing (NOT closeable). Plus a `Mystery`/`Unknown` intentionally absent so its
    ///   hierarchy is NOT known.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".into(), cm(None, &[], false));
        members.insert("java/lang/Throwable".into(), cm(Some("java/lang/Object"), &[], false));
        members.insert("java/lang/Exception".into(), cm(Some("java/lang/Throwable"), &[], false));
        members.insert("java/io/IOException".into(), cm(Some("java/lang/Exception"), &[], false));
        members.insert("java/sql/SQLException".into(), cm(Some("java/lang/Exception"), &[], false));
        members.insert("java/lang/AutoCloseable".into(), cm(None, &[], true));
        members.insert("java/io/Closeable".into(), cm(None, &["java/lang/AutoCloseable"], true));
        members.insert(
            "java/io/FileInputStream".into(),
            cm(Some("java/lang/Object"), &["java/io/Closeable"], false),
        );
        members.insert("com/acme/PlainThing".into(), cm(Some("java/lang/Object"), &[], false));
        // `RunawayEx` extends an UNKNOWN base → its hierarchy is not fully known (used to prove SKIP).
        members.insert("com/acme/RunawayEx".into(), cm(Some("com/acme/UnknownBase"), &[], false));
        // `lombok.val` — a REAL, resolvable, non-AutoCloseable type. With `import lombok.val;` a `val`
        // resource would resolve to it → the false positive the `var`/`val` skip guards against.
        members.insert("lombok/val".into(), cm(Some("java/lang/Object"), &[], false));

        let simple = [
            ("Exception", "java/lang/Exception"),
            ("Throwable", "java/lang/Throwable"),
            ("IOException", "java/io/IOException"),
            ("SQLException", "java/sql/SQLException"),
            ("AutoCloseable", "java/lang/AutoCloseable"),
            ("Closeable", "java/io/Closeable"),
            ("FileInputStream", "java/io/FileInputStream"),
            ("PlainThing", "com/acme/PlainThing"),
            ("RunawayEx", "com/acme/RunawayEx"),
            ("val", "lombok/val"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    /// Run against a body wrapped in a method that throws, so try/catch parses cleanly.
    fn diags(body: &str) -> Vec<String> {
        let src = format!("class C {{ void m() throws Throwable {{ {body} }} }}");
        exception_errors(&src, &resolver()).into_iter().map(|d| d.message).collect()
    }

    // ── check 1: unreachable catch ────────────────────────────────────────────────

    #[test]
    fn subtype_after_supertype_is_unreachable() {
        let d = diags("try { } catch (Exception e) { } catch (IOException e) { }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Unreachable catch") && d[0].contains("IOException") && d[0].contains("Exception"), "{d:?}");
    }

    #[test]
    fn identical_catch_type_after_is_unreachable() {
        // Same type twice → the second is unreachable (`sub == sup` branch).
        let d = diags("try { } catch (IOException e) { } catch (IOException e) { }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Unreachable catch"), "{d:?}");
    }

    #[test]
    fn sibling_catches_are_ok() {
        // NEGATIVE: IOException and SQLException are siblings (both extend Exception), neither a
        // subtype of the other → nothing flagged.
        assert!(diags("try { } catch (IOException e) { } catch (SQLException e) { }").is_empty());
    }

    #[test]
    fn supertype_after_subtype_is_ok() {
        // NEGATIVE: the narrow catch first, then the broad one — perfectly legal ordering.
        assert!(diags("try { } catch (IOException e) { } catch (Exception e) { }").is_empty());
    }

    #[test]
    fn unresolvable_later_catch_is_not_flagged() {
        // NEGATIVE: `Mystery` doesn't resolve → we can't prove it's caught by `Exception`, so SKIP.
        assert!(diags("try { } catch (Exception e) { } catch (Mystery e) { }").is_empty());
    }

    #[test]
    fn later_catch_with_unknown_hierarchy_is_not_flagged() {
        // NEGATIVE: `RunawayEx` resolves but extends an UNKNOWN base, so `hierarchy_fully_known` is
        // false → the subtype relation to `Exception` can't be trusted → SKIP.
        assert!(diags("try { } catch (Exception e) { } catch (RunawayEx e) { }").is_empty());
    }

    // ── check 2: redundant multi-catch ────────────────────────────────────────────

    #[test]
    fn multi_catch_subtype_with_supertype_is_flagged() {
        let d = diags("try { } catch (IOException | Exception e) { }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Multi-catch") && d[0].contains("IOException") && d[0].contains("Exception"), "{d:?}");
    }

    #[test]
    fn multi_catch_supertype_first_still_flagged() {
        // Order within the clause doesn't matter — the subtype is redundant either way.
        let d = diags("try { } catch (Exception | IOException e) { }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Multi-catch"), "{d:?}");
    }

    #[test]
    fn multi_catch_unrelated_types_are_ok() {
        // NEGATIVE: two siblings in one multi-catch → legal, nothing flagged.
        assert!(diags("try { } catch (IOException | SQLException e) { }").is_empty());
    }

    #[test]
    fn multi_catch_with_unresolvable_alt_is_ok() {
        // NEGATIVE: one alternative unresolvable → no reasoning about it → SKIP.
        assert!(diags("try { } catch (Exception | Mystery e) { }").is_empty());
    }

    // ── check 3: resource must be AutoCloseable ────────────────────────────────────

    #[test]
    fn non_closeable_resource_is_flagged() {
        let d = diags("try (PlainThing p = null) { }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("must implement `AutoCloseable`") && d[0].contains("PlainThing"), "{d:?}");
    }

    #[test]
    fn closeable_resource_is_ok() {
        // NEGATIVE: FileInputStream implements Closeable (→ AutoCloseable) → fine.
        assert!(diags("try (FileInputStream in = null) { }").is_empty());
    }

    #[test]
    fn autocloseable_resource_directly_is_ok() {
        // NEGATIVE: the resource type IS AutoCloseable.
        assert!(diags("try (AutoCloseable a = null) { }").is_empty());
    }

    #[test]
    fn unresolvable_resource_type_is_not_flagged() {
        // NEGATIVE: `Mystery` doesn't resolve → we can't prove it's not AutoCloseable → SKIP.
        assert!(diags("try (Mystery r = null) { }").is_empty());
    }

    #[test]
    fn resource_with_unknown_hierarchy_is_not_flagged() {
        // NEGATIVE: `RunawayEx` resolves but has an unknown supertype (which MIGHT be AutoCloseable)
        // → hierarchy not fully known → SKIP.
        assert!(diags("try (RunawayEx r = null) { }").is_empty());
    }

    #[test]
    fn bare_resource_variable_form_is_not_flagged() {
        // NEGATIVE (Java 9+): `try (r)` with a pre-existing variable has no written type → we don't
        // infer it → SKIP.
        let d = diags("PlainThing r = null; try (r) { }");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn var_and_val_resources_are_never_flagged() {
        // A `var` / Lombok `val` resource infers its type from the initializer — the keyword is never
        // the resource type. With `import lombok.val;` present, `val` resolves to the real
        // (non-AutoCloseable) `lombok.val`; the skip must happen BEFORE resolution.
        let with_lombok =
            "import lombok.val;\nclass C { void m() throws Throwable { try (val bao = new Object()) {} } }";
        assert!(exception_errors(with_lombok, &resolver()).is_empty(), "`val` must be skipped");
        let with_var = "class C { void m() throws Throwable { try (var r = new Object()) {} } }";
        assert!(exception_errors(with_var, &resolver()).is_empty(), "`var` must be skipped");
    }
}
