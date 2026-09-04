//! Inherited ("super") members of a Java type — the Structure panel's lazy **"Inherited"** bucket.
//!
//! Given a type identified by `(file, simple_name, decl_line)` (its own declaration site — line
//! disambiguates a nested / same-simple-named type), resolve its **binary name**, then collect the
//! members of its SUPERCLASS + INTERFACES recursively (NOT the type's own declared members),
//! deduping overrides by name+kind so an override shadows the super declaration it hides.
//!
//! This reuses the same resolution machinery as member-access completion (the member walk over
//! super + interfaces via [`TypeResolver::members_of`]) — here we start the walk one level up (the
//! supertypes), so the type's own members are excluded.
//!
//! Each collected member is tagged with the `declaring_type` (the FQCN it was collected from) + its
//! `visibility`, and — like go-to-declaration — a `source` file+line **only when that declaring
//! type resolves to a PROJECT source** (a JDK / jar supertype's member has no openable source, so
//! `source` is `None`).
//!
//! The tree-sitter CST scans this needs — resolving the target's binary name by `(simple, line)`
//! and locating a supertype's project source — live in [`bennu_java::prelude`]
//! ([`binary_of_type_at`](bennu_java::prelude::binary_of_type_at) /
//! [`find_type_name_span`](bennu_java::prelude::find_type_name_span)), so this crate stays
//! parser-free (a pure resolver walk).

use std::collections::HashSet;

use bennu_java::prelude::{
    binary_of_type_at, find_type_name_span, Member, MemberKind, TypeRef, TypeResolver, Visibility,
};
// Only the `#[cfg(test)]` `MapResolver` names `ClassMembers` directly (the walk consumes members
// through the resolver, which hands back `Arc<ClassMembers>`).
#[cfg(test)]
use bennu_java::prelude::ClassMembers;

use crate::source::PlanFile;

/// One inherited member (the query-level view the be layer maps to the wire `InheritedMember`
/// field-for-field).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InheritedMember {
    /// `"method"` | `"field"`.
    pub kind: String,
    /// The member's simple name.
    pub name: String,
    /// A readable detail: the return type (methods) / field type. `None` when the type is not
    /// recorded.
    pub detail: Option<String>,
    /// `"public"` | `"protected"` | `"private"` | `"package"`.
    pub visibility: String,
    /// The dotted FQCN of the class / interface that declares the member.
    pub declaring_type: String,
    /// The project-source declaration site (file + 1-based line), or `None` when the declaring type
    /// is a JDK / jar type (no project source to open).
    pub source: Option<InheritedSource>,
}

/// Where an inherited member is declared, when it resolves to PROJECT source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InheritedSource {
    /// Absolute path (forward slashes) of the project file declaring the member's type.
    pub file: String,
    /// 1-based line of that type's declaration.
    pub line: i64,
}

/// Resolve the target type by `(file, simple_name, decl_line)` and collect the members of its
/// SUPERCLASS + INTERFACES (recursively, deduped) — the "inherited" set. Returns `[]` when the
/// target type can't be resolved in `file` (unknown type / stale line) — a benign, non-fatal state
/// (the FE shows an empty bucket).
///
/// `java_files` are the project's `.java` sources (path + text); they resolve the target's binary
/// name and, for each inherited member, its declaring type's project source (else `None`, like
/// go-to-declaration).
pub fn inherited_members(
    resolver: &dyn TypeResolver,
    java_files: &[PlanFile],
    file: &str,
    type_name: &str,
    decl_line: i64,
) -> Vec<InheritedMember> {
    let Some(binary) = resolve_target_binary(java_files, file, type_name, decl_line) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    // The target's own members are EXCLUDED — its depth is 0, everything above it is not.
    bennu_java::prelude::walk_up::<()>(resolver, &TypeRef::simple(&binary), |a| {
        if a.depth == 0 {
            return None;
        }
        let binary = &a.ty.binary_name;
        let declaring_type = binary.replace('/', ".");
        // Resolve the declaring type's project source ONCE per type (shared by its members).
        let source = project_source_of(java_files, binary);
        for m in a.members.methods.iter().chain(a.members.fields.iter()) {
            let key = format!("{}/{}", kind_tag(m.kind), m.name);
            if !seen.insert(key) {
                continue; // an override lower in the hierarchy already claimed this name+kind
            }
            out.push(InheritedMember {
                kind: kind_tag(m.kind).to_string(),
                name: m.name.clone(),
                detail: render_detail(m),
                visibility: visibility_tag(m.visibility).to_string(),
                declaring_type: declaring_type.clone(),
                source: source.clone(),
            });
        }
        None
    });
    // Deterministic order: fields then methods, alphabetical within (matches completion).
    out.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));
    out
}

/// Resolve the target type's JVM binary name by locating the declaration named `type_name` on
/// 1-based `decl_line` in `file`'s project source. The line disambiguates a nested /
/// same-simple-named type. `None` when no project source is `file`, or `file` declares no such type
/// at that line.
fn resolve_target_binary(
    java_files: &[PlanFile],
    file: &str,
    type_name: &str,
    decl_line: i64,
) -> Option<String> {
    let source = java_files
        .iter()
        .find(|f| f.path == file)
        .map(|f| f.source.as_str())?;
    binary_of_type_at(source, type_name, decl_line)
}

/// The project-source declaration of type `binary`, or `None` when no project `.java` declares it
/// (a JDK / jar type). Mirrors go-to-declaration's project-source scan: the first source with a
/// matching type-name span wins.
fn project_source_of(java_files: &[PlanFile], binary: &str) -> Option<InheritedSource> {
    let simple = binary.rsplit(['/', '$']).next().unwrap_or(binary);
    for f in java_files {
        if let Some((start, _end)) = find_type_name_span(&f.source, simple) {
            let line = line_1based(&f.source, start);
            return Some(InheritedSource {
                file: f.path.clone(),
                line,
            });
        }
    }
    None
}

/// 1-based line of byte `start` in `source` (1 + count of `'\n'` before it).
fn line_1based(source: &str, start: usize) -> i64 {
    let clamped = start.min(source.len());
    1 + source.as_bytes()[..clamped]
        .iter()
        .filter(|&&b| b == b'\n')
        .count() as i64
}

fn kind_tag(k: MemberKind) -> &'static str {
    match k {
        MemberKind::Method => "method",
        MemberKind::Field => "field",
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

/// A readable detail line: the return type (methods) / field type. `None` when the type is
/// unrecorded (an empty binary name).
fn render_detail(m: &Member) -> Option<String> {
    let rendered = render_type(&m.return_type);
    if rendered.is_empty() {
        None
    } else {
        Some(rendered)
    }
}

/// Render a `TypeRef` to a readable simple form: `java/util/List<Foo>` → `List<Foo>`.
fn render_type(t: &TypeRef) -> String {
    if t.binary_name.is_empty() {
        return String::new();
    }
    let simple = t.binary_name.rsplit('/').next().unwrap_or(&t.binary_name);
    if t.type_args.is_empty() {
        simple.to_string()
    } else {
        let args: Vec<String> = t.type_args.iter().map(render_type).collect();
        format!("{}<{}>", simple, args.join(", "))
    }
}

/// A trivial in-crate `TypeResolver` used by the unit tests: a fixed binary→members map, plus a
/// simple→binary table. Mirrors the shape the real `IndexResolver` exposes.
#[cfg(test)]
struct MapResolver {
    members: std::collections::HashMap<String, ClassMembers>,
    simple: std::collections::HashMap<String, String>,
}

#[cfg(test)]
impl TypeResolver for MapResolver {
    fn members_of(&self, binary: &str) -> Option<std::sync::Arc<ClassMembers>> {
        self.members.get(binary).cloned().map(std::sync::Arc::new)
    }
    fn resolve_simple_name(
        &self,
        name: &str,
        _imports: &[bennu_java::prelude::Import],
    ) -> Option<String> {
        self.simple.get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn method(name: &str, ret: &str, vis: Visibility) -> Member {
        Member::method(name, TypeRef::simple(ret.to_string()), Vec::new())
            .vis(vis)
            .sig(format!("{ret} {name}()"))
    }

    fn field(name: &str, ty: &str, vis: Visibility) -> Member {
        Member::field(name, TypeRef::simple(ty.to_string()))
            .vis(vis)
            .sig(format!("{ty} {name}"))
    }

    fn plan_file(path: &str, source: &str) -> PlanFile {
        PlanFile {
            path: path.to_string(),
            source: source.to_string(),
        }
    }

    #[test]
    fn project_subclass_inherits_project_superclass_method_with_source() {
        // Sub extends Base; Base declares `greet()` in a PROJECT source → the inherited member
        // carries a source pointing at Base.java, and Sub's own members are excluded.
        let base_src = "package com.acme;\npublic class Base {\n  public String greet() { return \"hi\"; }\n}\n";
        let sub_src = "package com.acme;\npublic class Sub extends Base {\n  public int own() { return 1; }\n}\n";
        let java = vec![
            plan_file("Base.java", base_src),
            plan_file("Sub.java", sub_src),
        ];

        let mut members = HashMap::new();
        members.insert(
            "com/acme/Base".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some(TypeRef::simple("java/lang/Object")),
                interfaces: Vec::new(),
                methods: vec![method("greet", "java/lang/String", Visibility::Public)],
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        members.insert(
            "com/acme/Sub".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some(TypeRef::simple("com/acme/Base")),
                interfaces: Vec::new(),
                methods: vec![method("own", "int", Visibility::Public)],
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        // `java/lang/Object` is a JDK type: NOT in `members` (no ClassMembers) → its members
        // (none here) contribute nothing, and it has no project source.
        let simple = HashMap::new();
        let resolver = MapResolver { members, simple };

        let got = inherited_members(&resolver, &java, "Sub.java", "Sub", 2);
        // Exactly Base.greet() — Sub.own() (the type's OWN member) is excluded.
        assert_eq!(got.len(), 1);
        let g = &got[0];
        assert_eq!(g.kind, "method");
        assert_eq!(g.name, "greet");
        assert_eq!(g.detail.as_deref(), Some("String"));
        assert_eq!(g.visibility, "public");
        assert_eq!(g.declaring_type, "com.acme.Base");
        // Base is a project source → source present, pointing at Base.java line 2.
        let src = g.source.as_ref().expect("project source");
        assert_eq!(src.file, "Base.java");
        assert_eq!(src.line, 2);
    }

    #[test]
    fn jdk_supertype_member_has_null_source() {
        // Sub extends a JDK type (java/util/AbstractList) whose member `size()` is resolvable (the
        // JDK member index provides ClassMembers) but has NO project source → source null.
        let sub_src = "package com.acme;\npublic class Sub {\n}\n";
        let java = vec![plan_file("Sub.java", sub_src)];

        let mut members = HashMap::new();
        members.insert(
            "com/acme/Sub".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some(TypeRef::simple("java/util/AbstractList")),
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        members.insert(
            "java/util/AbstractList".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: None,
                interfaces: Vec::new(),
                methods: vec![method("size", "int", Visibility::Public)],
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        let resolver = MapResolver {
            members,
            simple: HashMap::new(),
        };

        let got = inherited_members(&resolver, &java, "Sub.java", "Sub", 2);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "size");
        assert_eq!(got[0].declaring_type, "java.util.AbstractList");
        // No project source declares AbstractList → source is None.
        assert!(
            got[0].source.is_none(),
            "JDK supertype member has null source"
        );
    }

    #[test]
    fn override_dedups_to_lowest_declaration() {
        // Both Base and Mid declare `run()`; the walk starts at Sub's supertype Mid, so Mid's `run`
        // claims the name+kind and Base's is shadowed (deduped).
        let java = vec![plan_file(
            "Sub.java",
            "package com.acme;\npublic class Sub {\n}\n",
        )];
        let mut members = HashMap::new();
        members.insert(
            "com/acme/Sub".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some(TypeRef::simple("com/acme/Mid")),
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        members.insert(
            "com/acme/Mid".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some(TypeRef::simple("com/acme/Base")),
                interfaces: Vec::new(),
                methods: vec![method("run", "void", Visibility::Protected)],
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        members.insert(
            "com/acme/Base".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: None,
                interfaces: Vec::new(),
                methods: vec![method("run", "void", Visibility::Public)],
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        let resolver = MapResolver {
            members,
            simple: HashMap::new(),
        };
        let got = inherited_members(&resolver, &java, "Sub.java", "Sub", 2);
        // Exactly one `run` — Mid's (the nearer declaration), Base's is deduped.
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "run");
        assert_eq!(got[0].declaring_type, "com.acme.Mid");
        assert_eq!(got[0].visibility, "protected");
    }

    #[test]
    fn unresolved_target_returns_empty() {
        let java = vec![plan_file(
            "Sub.java",
            "package com.acme;\npublic class Sub {\n}\n",
        )];
        let resolver = MapResolver {
            members: HashMap::new(),
            simple: HashMap::new(),
        };
        // Unknown type name → empty (not a panic).
        assert!(inherited_members(&resolver, &java, "Sub.java", "Nope", 2).is_empty());
    }

    #[test]
    fn field_and_method_ordering_is_deterministic() {
        let java = vec![plan_file(
            "Sub.java",
            "package com.acme;\npublic class Sub {\n}\n",
        )];
        let mut members = HashMap::new();
        members.insert(
            "com/acme/Sub".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some(TypeRef::simple("com/acme/Base")),
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        members.insert(
            "com/acme/Base".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: None,
                interfaces: Vec::new(),
                methods: vec![
                    method("zeta", "void", Visibility::Public),
                    method("alpha", "void", Visibility::Public),
                ],
                fields: vec![field("count", "int", Visibility::Protected)],
                flags: Default::default(),
            },
        );
        let resolver = MapResolver {
            members,
            simple: HashMap::new(),
        };
        let got = inherited_members(&resolver, &java, "Sub.java", "Sub", 2);
        // fields before methods, alphabetical within.
        let names: Vec<&str> = got.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["count", "alpha", "zeta"]);
        assert_eq!(got[0].kind, "field");
    }
}
