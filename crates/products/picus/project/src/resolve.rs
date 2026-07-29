//! Inheritance: turning what each folder *declares* into what actually applies
//! to it.
//!
//! One rule, and everything in the script half rests on it:
//!
//! > A folder's dialect (and its role) is the one it declares; a folder that
//! > declares none takes its nearest ancestor's, and a folder no ancestor
//! > declares one for has **none**.
//!
//! That is what lets a repository put the role at the top of the tree and the
//! dialect at the bottom —
//!
//! ```text
//! AGGIORNAMENTO           role    = update
//! AGGIORNAMENTO/2024/ORA  dialect = oracle
//! ```
//!
//! — and it is the reason `effective_dialect` is an `Option` rather than a
//! default. `AGGIORNAMENTO/2024/POS` in that same repository matches nothing
//! Picus recognises, so it ends up with no dialect, takes part in no
//! cross-dialect comparison and receives no generated SQL until a human says
//! what it is. A default here would write Oracle syntax into a PostgreSQL file,
//! which is the failure the product exists to catch.
//!
//! Pure and total: no I/O, no configuration lookup, no failure mode.

use picus_types::prelude::{FolderEngine, FolderRole};

use crate::tree::FolderNode;

/// Resolve a forest of folders in place.
///
/// `engine` and `role` are what the nodes inherit from **above** the forest —
/// the declaration on the repository root (`path = ""`), or `None` when there is
/// none. Every consumer other than discovery passes `None, None`.
///
/// The engine inherits as **one** value, whichever of the four it is: a
/// repository that declares `MSQ` once at the top means SQL Server all the way
/// down, `COMUNE` means portable all the way down, and `ORACLE` means Oracle all
/// the way down. There is nothing here that knows the difference between them,
/// which is exactly why adding a fourth state cost this function nothing.
pub fn resolve(nodes: &mut [FolderNode], engine: Option<FolderEngine>, role: Option<FolderRole>) {
    resolve_in(nodes, engine, role, false)
}

/// The same, for the one caller that also has the repository root's **exclusion**
/// to hand down.
///
/// Separate rather than a fourth parameter on [`resolve`] because every consumer
/// but discovery passes `None, None` and has nothing above the forest at all;
/// making them all say `false` would be noise on a dozen call sites to serve one.
pub fn resolve_from(
    nodes: &mut [FolderNode],
    engine: Option<FolderEngine>,
    role: Option<FolderRole>,
    excluded: bool,
) {
    resolve_in(nodes, engine, role, excluded)
}

/// The recursion, carrying the one thing that inherits as a plain value rather
/// than as an `Option`: whether we are already inside an excluded subtree.
fn resolve_in(
    nodes: &mut [FolderNode],
    engine: Option<FolderEngine>,
    role: Option<FolderRole>,
    excluded: bool,
) {
    resolve_within(nodes, engine, role, excluded, None)
}

/// The recursion proper, carrying the product as well.
///
/// `product` is threaded as a borrowed `Option<&str>` rather than owned: it
/// inherits down every level of a tree with thousands of folders, and cloning a
/// string per node to hand it to its children would be paying for a repository
/// shape almost nobody has.
fn resolve_within(
    nodes: &mut [FolderNode],
    engine: Option<FolderEngine>,
    role: Option<FolderRole>,
    excluded: bool,
    product: Option<&str>,
) {
    for node in nodes {
        let engine = node.engine.or(engine);
        let role = node.role.or(role);
        // Three-valued on the way in, two-valued on the way out: saying nothing
        // inherits, and saying `false` inside an excluded folder rescues this
        // subtree — which is the whole reason it is an `Option<bool>` and not a
        // `bool`. Without that, one wanted script inside an archived folder
        // would mean un-archiving the folder.
        let excluded = node.excluded.unwrap_or(excluded);
        node.effective_engine = engine;
        node.effective_excluded = excluded;
        // `Ignored` is the honest fallback rather than a dismissal: a folder
        // nobody classified must not receive generated SQL.
        node.effective_role = role.unwrap_or(FolderRole::Ignored);
        // Same three-valued shape, minus the escape hatch: there is deliberately
        // no way to declare "no product" under a folder that has one, because a
        // subfolder of a product's scripts belongs to that product.
        node.effective_product = match node.product.as_deref() {
            Some(own) => Some(own.to_string()),
            None => product.map(str::to_string),
        };
        // A file is the last link of the same chain, on both axes: it takes its
        // folder's answer unless it declares one, exactly as the folder takes its
        // parent's. Done here rather than anywhere else so there is one
        // inheritance rule in the codebase and no second place for a file and its
        // folder to disagree.
        for file in &mut node.files {
            file.effective_engine = file.engine.or(engine);
            file.effective_excluded = file.excluded.unwrap_or(excluded);
        }
        // Destructured so the children's mutable borrow and the product's shared
        // one are disjoint — `node.effective_product` is what the subtree inherits,
        // and it has just been computed.
        let FolderNode { effective_product, children, .. } = node;
        resolve_within(children, engine, role, excluded, effective_product.as_deref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::ScriptFile;
    use picus_types::prelude::{EngineKind, ForeignEngine};

    fn node(path: &str, dialect: Option<EngineKind>, role: Option<FolderRole>) -> FolderNode {
        engine_node(path, dialect.map(FolderEngine::Supported), role)
    }

    fn engine_node(
        path: &str,
        engine: Option<FolderEngine>,
        role: Option<FolderRole>,
    ) -> FolderNode {
        let name = crate::path::last_segment(path).to_string();
        FolderNode { engine, role, ..FolderNode::new(path, name) }
    }

    /// A folder declaring an engine Picus recognises and does not read.
    fn foreign_node(path: &str, engine: ForeignEngine) -> FolderNode {
        engine_node(path, Some(FolderEngine::Unsupported(engine)), None)
    }

    fn product_node(path: &str, product: Option<&str>) -> FolderNode {
        let name = crate::path::last_segment(path).to_string();
        FolderNode {
            product: product.map(str::to_string),
            ..FolderNode::new(path, name)
        }
    }

    fn nest(mut parent: FolderNode, children: Vec<FolderNode>) -> FolderNode {
        parent.children = children;
        parent
    }

    #[test]
    fn a_declaration_reaches_every_descendant() {
        let mut tree = vec![nest(
            node("ORACLE", Some(EngineKind::Oracle), None),
            vec![nest(
                node("ORACLE/AGGIORNAMENTO", None, Some(FolderRole::Update)),
                vec![node("ORACLE/AGGIORNAMENTO/2026", None, None)],
            )],
        )];
        resolve(&mut tree, None, None);

        let deepest = &tree[0].children[0].children[0];
        assert_eq!(deepest.effective_dialect(), Some(EngineKind::Oracle));
        assert_eq!(deepest.effective_role, FolderRole::Update);
        // …and the intermediate folder is not skipped over.
        assert_eq!(tree[0].children[0].effective_dialect(), Some(EngineKind::Oracle));
        // The top declares no role, so it has none — the update folder's role is
        // not read upwards.
        assert_eq!(tree[0].effective_role, FolderRole::Ignored);
    }

    #[test]
    fn the_nearest_declaration_wins_over_a_further_one() {
        let mut tree = vec![nest(
            node("DB", Some(EngineKind::Oracle), Some(FolderRole::Init)),
            vec![node("DB/PG", Some(EngineKind::Postgres), Some(FolderRole::Update))],
        )];
        resolve(&mut tree, None, None);

        let child = &tree[0].children[0];
        assert_eq!(child.effective_dialect(), Some(EngineKind::Postgres));
        assert_eq!(child.effective_role, FolderRole::Update);
        // The override changes nothing above it.
        assert_eq!(tree[0].effective_dialect(), Some(EngineKind::Oracle));
    }

    #[test]
    fn the_role_and_the_dialect_inherit_independently() {
        // The repository this whole model exists for: the role is declared at the
        // top, the dialect at the bottom, and neither carries the other.
        let mut tree = vec![nest(
            node("AGGIORNAMENTO", None, Some(FolderRole::Update)),
            vec![nest(
                node("AGGIORNAMENTO/2024", None, None),
                vec![
                    node("AGGIORNAMENTO/2024/ORA", Some(EngineKind::Oracle), None),
                    node("AGGIORNAMENTO/2024/POS", None, None),
                ],
            )],
        )];
        resolve(&mut tree, None, None);

        let year = &tree[0].children[0];
        let ora = &year.children[0];
        let pos = &year.children[1];
        assert_eq!((ora.effective_dialect(), ora.effective_role), (Some(EngineKind::Oracle), FolderRole::Update));
        // Unclassified: no dialect at all, and that is the answer, not a gap to
        // be filled with a guess.
        assert_eq!((pos.effective_dialect(), pos.effective_role), (None, FolderRole::Update));
    }

    #[test]
    fn a_sibling_never_inherits_from_a_sibling() {
        let mut tree = vec![
            node("ORACLE", Some(EngineKind::Oracle), Some(FolderRole::Init)),
            node("ALTRO", None, None),
        ];
        resolve(&mut tree, None, None);
        assert_eq!(tree[1].effective_dialect(), None);
        assert_eq!(tree[1].effective_role, FolderRole::Ignored);
    }

    #[test]
    fn a_declaration_on_the_repository_root_applies_to_everything() {
        let mut tree = vec![nest(
            node("AGGIORNAMENTO", None, Some(FolderRole::Update)),
            vec![node("AGGIORNAMENTO/2024", None, None)],
        )];
        resolve(&mut tree, Some(FolderEngine::Supported(EngineKind::Postgres)), None);
        for node in tree[0].walk() {
            assert_eq!(node.effective_dialect(), Some(EngineKind::Postgres), "{}", node.path);
        }
    }

    #[test]
    fn an_unsupported_engine_inherits_exactly_like_a_dialect() {
        // Declared once at the top of a subtree, meant all the way down: the
        // whole tree under `MSQ` is SQL Server and none of it is ever parsed.
        let mut tree = vec![nest(
            foreign_node("AGGIORNAMENTO/MSQ", ForeignEngine::SqlServer),
            vec![node("AGGIORNAMENTO/MSQ/2024", None, None)],
        )];
        resolve(&mut tree, None, None);

        for node in tree[0].walk() {
            assert_eq!(
                node.effective_engine.and_then(FolderEngine::foreign),
                Some(ForeignEngine::SqlServer),
                "{}",
                node.path
            );
            assert_eq!(node.effective_dialect(), None, "{}", node.path);
            assert!(node.engine_is_unsupported() && !node.engine_is_unknown(), "{}", node.path);
        }
    }

    #[test]
    fn portable_sql_inherits_like_any_other_engine_and_covers_both_dialects() {
        // `COMUNE` holds the plain inserts meant to run on both engines; the year
        // folders under it say nothing and mean it.
        let mut tree = vec![nest(
            engine_node("COMUNE", Some(FolderEngine::Generic), Some(FolderRole::Data)),
            vec![engine_node("COMUNE/2024", None, None)],
        )];
        resolve(&mut tree, None, None);

        for node in tree[0].walk() {
            assert!(node.is_generic(), "{}", node.path);
            assert_eq!(node.effective_dialect(), None, "{}: no single one", node.path);
            for dialect in EngineKind::ALL {
                assert!(node.covers(*dialect), "{} must count for {dialect}", node.path);
            }
        }
    }

    #[test]
    fn a_dialect_declared_under_a_portable_folder_narrows_it_again() {
        // The escape hatch: one subfolder of an otherwise portable tree that is
        // genuinely Oracle-only. The nearest declaration wins, as always.
        let mut tree = vec![nest(
            engine_node("COMUNE", Some(FolderEngine::Generic), None),
            vec![node("COMUNE/ORA", Some(EngineKind::Oracle), None)],
        )];
        resolve(&mut tree, None, None);

        let child = &tree[0].children[0];
        assert!(!child.is_generic());
        assert_eq!(child.effective_dialect(), Some(EngineKind::Oracle));
        assert!(!child.covers(EngineKind::Postgres));
        assert!(tree[0].covers(EngineKind::Postgres));
    }

    #[test]
    fn a_dialect_below_an_unsupported_engine_overrides_it_and_vice_versa() {
        // One slot: the nearest declaration wins whichever kind it is, and the
        // two never coexist on a folder.
        let mut tree = vec![nest(
            foreign_node("DB", ForeignEngine::Db2),
            vec![node("DB/PG", Some(EngineKind::Postgres), None)],
        )];
        resolve(&mut tree, None, None);

        let child = &tree[0].children[0];
        assert_eq!(child.effective_dialect(), Some(EngineKind::Postgres));
        assert_eq!(child.effective_engine.and_then(FolderEngine::foreign), None);
        assert_eq!(tree[0].effective_engine.and_then(FolderEngine::foreign), Some(ForeignEngine::Db2));
    }

    // ── Exclusion ─────────────────────────────────────────────────────────────

    fn with_files(mut node: FolderNode, names: &[&str]) -> FolderNode {
        node.files = names
            .iter()
            .map(|name| ScriptFile {
                path: format!("{}/{name}", node.path),
                name: name.to_string(),
                size: 0,
                encoding: "windows-1252".to_string(),
                encoding_source: arbor_fs::prelude::encoding::EncodingSource::Inherited,
                eol: crate::tree::LineEnding::Crlf,
                expected_encoding: "windows-1252".to_string(),
                engine: None,
                effective_engine: None,
                excluded: None,
                effective_excluded: false,
            })
            .collect();
        node
    }

    #[test]
    fn excluding_a_folder_reaches_every_file_under_it() {
        let mut tree = vec![nest(
            FolderNode {
                excluded: Some(true),
                ..node("MIGRAZIONE_2019", Some(EngineKind::Oracle), Some(FolderRole::Update))
            },
            vec![with_files(node("MIGRAZIONE_2019/DDL", None, None), &["01.sql"])],
        )];
        resolve(&mut tree, None, None);

        for folder in tree[0].walk() {
            assert!(folder.is_excluded(), "{}", folder.path);
        }
        assert!(tree[0].children[0].files[0].is_excluded());
        // …and the engine still resolves normally underneath. Exclusion is a
        // separate axis, not a way of un-classifying something.
        assert_eq!(tree[0].children[0].effective_dialect(), Some(EngineKind::Oracle));
    }

    #[test]
    fn one_script_can_be_rescued_from_an_excluded_folder() {
        // The reason it is an `Option<bool>` and not a `bool`: without this, one
        // wanted script inside an archived folder would mean un-archiving the
        // whole folder.
        let mut folder = with_files(node("ARCHIVIO", None, None), &["vecchio.sql", "serve.sql"]);
        folder.excluded = Some(true);
        folder.files[1].excluded = Some(false);
        let mut tree = vec![folder];
        resolve(&mut tree, None, None);

        assert!(tree[0].is_excluded());
        assert!(tree[0].files[0].is_excluded());
        assert!(!tree[0].files[1].is_excluded(), "this one was rescued");
        let kept: Vec<&str> = tree[0].included_files().map(|f| f.name.as_str()).collect();
        assert_eq!(kept, ["serve.sql"]);
    }

    #[test]
    fn a_single_script_can_be_excluded_inside_a_folder_that_is_not() {
        let mut folder = with_files(node("AGGIORNAMENTO", Some(EngineKind::Oracle), Some(FolderRole::Update)), &["a.sql", "migrazione.sql"]);
        folder.files[1].excluded = Some(true);
        let mut tree = vec![folder];
        resolve(&mut tree, None, None);

        assert!(!tree[0].is_excluded());
        assert!(!tree[0].files[0].is_excluded());
        assert!(tree[0].files[1].is_excluded());
        // The excluded script takes no part in the lane its folder is in.
        assert!(tree[0].is_in_lane(EngineKind::Oracle, FolderRole::Update));
        assert_eq!(tree[0].included_files().count(), 1);
    }

    #[test]
    fn excluded_is_not_the_same_as_the_ignored_role() {
        // The distinction the whole design rests on. `Ignored` is also what a
        // folder nobody classified falls back to, so if it meant "excluded" every
        // unclassified folder would silently vanish from the report — which is
        // the opposite of what should happen to it.
        let mut tree = vec![with_files(node("MISCELLANEA", Some(EngineKind::Oracle), None), &["x.sql"])];
        resolve(&mut tree, None, None);

        assert_eq!(tree[0].effective_role, FolderRole::Ignored, "nobody said what it is for");
        assert!(!tree[0].is_excluded(), "…and that must not remove it from the project");
        assert_eq!(tree[0].included_files().count(), 1);
    }

    #[test]
    fn nothing_is_excluded_unless_somebody_said_so() {
        let mut tree = vec![nest(
            node("ORACLE", Some(EngineKind::Oracle), None),
            vec![with_files(node("ORACLE/BACKUP", None, Some(FolderRole::Ignored)), &["old.sql"])],
        )];
        resolve(&mut tree, None, None);
        for folder in tree[0].walk() {
            assert!(!folder.is_excluded(), "{}", folder.path);
        }
    }

    #[test]
    fn resolving_twice_changes_nothing() {
        // The tree is re-resolved after every edit the user confirms, so this has
        // to be a function of the declarations alone — never of the previous run.
        let mut tree = vec![nest(
            node("ORACLE", Some(EngineKind::Oracle), None),
            vec![node("ORACLE/AGGIORNAMENTO", None, Some(FolderRole::Update))],
        )];
        resolve(&mut tree, None, None);
        let once = tree.clone();
        resolve(&mut tree, None, None);
        assert_eq!(tree, once);
    }

    #[test]
    fn a_product_declared_once_reaches_every_folder_below_it() {
        // The whole point of putting it on a folder rather than on each
        // destination: `PORTALE` is said at the top and next month's version
        // folder inherits it without anyone touching the file again.
        let mut tree = vec![nest(
            product_node("PORTALE", Some("PORTALE")),
            vec![nest(
                product_node("PORTALE/AGGIORNAMENTO", None),
                vec![product_node("PORTALE/AGGIORNAMENTO/4_13/ORA", None)],
            )],
        )];
        resolve(&mut tree, None, None);
        for folder in tree[0].walk() {
            assert_eq!(folder.effective_product.as_deref(), Some("PORTALE"), "{}", folder.path);
        }
    }

    #[test]
    fn a_folder_that_names_its_own_product_overrides_the_one_above() {
        // A repository that nests one product's scripts inside another's tree.
        let mut tree = vec![nest(
            product_node("INSTALL", Some("CORE")),
            vec![product_node("INSTALL/PORTALE", Some("PORTALE"))],
        )];
        resolve(&mut tree, None, None);
        assert_eq!(tree[0].effective_product.as_deref(), Some("CORE"));
        assert_eq!(tree[0].children[0].effective_product.as_deref(), Some("PORTALE"));
    }

    #[test]
    fn a_repository_that_declares_no_product_resolves_to_none_everywhere() {
        // The ordinary repository, which installs one thing. Nothing about this
        // feature may cost it anything.
        let mut tree = vec![nest(
            node("ORACLE", Some(EngineKind::Oracle), None),
            vec![node("ORACLE/AGGIORNAMENTO", None, Some(FolderRole::Update))],
        )];
        resolve(&mut tree, None, None);
        for folder in tree[0].walk() {
            assert_eq!(folder.effective_product, None, "{}", folder.path);
        }
    }
}
