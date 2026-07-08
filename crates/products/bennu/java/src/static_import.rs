//! Static-import targets — parsing `import static …` into `(owner, member)` for the semantic engine.
//!
//! A static import binds an owner type's static members into the file's bare namespace: `import static
//! java.lang.Math.max;` makes `max(…)` resolvable unqualified; `import static java.lang.Math.*;` makes
//! every static member of `Math` resolvable. The path is always fully qualified, so the owner is
//! derived directly (no name resolution). Consumed by type inference (a bare `PI` / `max(…)` resolves
//! to its member's type) and by the undefined-variable check (a bare name that IS a static import is
//! not undefined).

use crate::symbols::Import;

/// One `import static …` resolved to its owner type + the imported member (or wildcard).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticImportTarget {
    /// The owner type's binary name (slash form), e.g. `java/lang/Math`.
    pub owner_binary: String,
    /// The specific imported member's simple name (`max`), or `None` for a wildcard `import static X.*;`.
    pub member: Option<String>,
}

/// The static-import targets declared by a file's imports (empty when it has no `import static`).
/// `import static a.b.C.foo;` → owner `a/b/C`, member `foo`; `import static a.b.C.*;` → owner `a/b/C`,
/// member `None`. A single-segment path (a default-package owner) with no wildcard has no owner to
/// split off, so it's dropped (not a real, resolvable static import).
pub fn static_import_targets(imports: &[Import]) -> Vec<StaticImportTarget> {
    imports
        .iter()
        .filter(|i| i.static_)
        .filter_map(|i| {
            let binary = i.path.replace('.', "/");
            if i.star {
                Some(StaticImportTarget { owner_binary: binary, member: None })
            } else {
                let (owner, member) = binary.rsplit_once('/')?;
                Some(StaticImportTarget {
                    owner_binary: owner.to_string(),
                    member: Some(member.to_string()),
                })
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn imp(path: &str, star: bool, static_: bool) -> Import {
        Import { path: path.to_string(), star, static_ }
    }

    #[test]
    fn specific_and_wildcard_targets_only_from_static_imports() {
        let imports = vec![
            imp("java.lang.Math.max", false, true),      // specific static
            imp("java.util.Collections", true, true),    // wildcard static
            imp("java.util.List", false, false),         // non-static → ignored
            imp("java.util.stream.Collectors", true, false), // non-static wildcard → ignored
        ];
        let t = static_import_targets(&imports);
        assert_eq!(t.len(), 2);
        assert_eq!(
            t[0],
            StaticImportTarget { owner_binary: "java/lang/Math".into(), member: Some("max".into()) }
        );
        assert_eq!(
            t[1],
            StaticImportTarget { owner_binary: "java/util/Collections".into(), member: None }
        );
    }
}
