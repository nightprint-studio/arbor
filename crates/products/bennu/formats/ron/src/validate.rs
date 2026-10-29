//! A RON document checked against a Rust type.
//!
//! The walk goes down the document and the type together: at each struct, the fields the document
//! writes are matched against the fields the type declares (by serialised name or alias, through
//! `flatten`), and the other way round for the ones it leaves out.
//!
//! **Quiet by construction.** The schema is a reading of the source by `syn`, and there is
//! serde behaviour it never sees (see the crate docs). So every check only fires where the
//! document's shape *agrees* with the type's — a named struct written as `Name(a: 1)`, an enum
//! written with a variant name — and stays silent when it does not, because a shape mismatch is as
//! likely an `untagged` enum or a `transparent` wrapper as a mistake.

use arbor_studio_types::prelude::{FieldDef, ResolvedType, Schema, TypeDef, VariantDef, VariantShape};

use crate::scan::{scan, Field, Head, Kind, Node, Scalar};
use crate::{RonDiagnostic, RonSeverity};

/// Nesting beyond this is a recursive type walked through a deep document; stop rather than follow.
const MAX_DEPTH: usize = 64;
/// A document with this many findings is wrong at the root, and the rest is noise.
const MAX_FINDINGS: usize = 200;

/// The findings of `src` against `schema`'s root type. Empty when the text does not scan — the
/// syntax diagnostics are the voice for that.
pub fn validate(src: &str, schema: &Schema) -> Vec<RonDiagnostic> {
    let Some(root) = scan(src) else { return Vec::new() };
    let mut v = Validator { schema, out: Vec::new() };
    v.check(&root, &ResolvedType::Named { path: schema.root_type.clone() }, 0);
    v.out
}

struct Validator<'a> {
    schema: &'a Schema,
    out: Vec<RonDiagnostic>,
}

impl Validator<'_> {
    fn report(&mut self, start: usize, end: usize, severity: RonSeverity, code: &'static str, message: String) {
        if self.out.len() < MAX_FINDINGS {
            self.out.push(RonDiagnostic { start, end, severity, code, message });
        }
    }

    fn check(&mut self, node: &Node, ty: &ResolvedType, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        match ty {
            ResolvedType::Primitive { name } => self.check_primitive(node, name),
            ResolvedType::Option { inner } => self.check_option(node, inner, depth),
            ResolvedType::Vec { inner } => {
                if let Kind::List(items) = &node.kind {
                    for item in items {
                        self.check(item, inner, depth + 1);
                    }
                }
            }
            ResolvedType::Map { key, value } => {
                if let Kind::Map(entries) = &node.kind {
                    for (k, v) in entries {
                        self.check(k, key, depth + 1);
                        self.check(v, value, depth + 1);
                    }
                }
            }
            ResolvedType::Tuple { items: tys } => self.check_tuple(node, tys, depth),
            ResolvedType::Named { path } => {
                if let Some(def) = self.schema.types.get(path) {
                    self.check_def(node, def, depth);
                }
            }
            // A type from another crate, or one the loader gave up on: nothing to compare to.
            ResolvedType::External { .. } | ResolvedType::Unknown { .. } => {}
        }
    }

    /// `None`, `Some(x)`, or — under `implicit_some` — a bare `x`.
    fn check_option(&mut self, node: &Node, inner: &ResolvedType, depth: usize) {
        match &node.kind {
            Kind::Ident(n) if n == "None" => {}
            Kind::Tuple { head, items } if head.name.as_deref() == Some("Some") && items.len() == 1 => {
                self.check(&items[0], inner, depth + 1);
            }
            _ => self.check(node, inner, depth + 1),
        }
    }

    fn check_tuple(&mut self, node: &Node, tys: &[ResolvedType], depth: usize) {
        let Kind::Tuple { head, items } = &node.kind else { return };
        if head.name.is_some() {
            return;
        }
        if items.len() != tys.len() {
            self.report(
                node.start,
                node.end,
                RonSeverity::Warning,
                "ron-tuple-arity",
                format!("expected {} elements, found {}", tys.len(), items.len()),
            );
            return;
        }
        for (item, ty) in items.iter().zip(tys) {
            self.check(item, ty, depth + 1);
        }
    }

    fn check_primitive(&mut self, node: &Node, name: &str) {
        let Kind::Scalar(found) = node.kind else { return };
        let expected: &str = match name {
            "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32" | "i64" | "i128" | "isize" => {
                if found == Scalar::Int {
                    return;
                }
                "an integer"
            }
            "f32" | "f64" => {
                if matches!(found, Scalar::Int | Scalar::Float) {
                    return;
                }
                "a number"
            }
            "bool" => {
                if found == Scalar::Bool {
                    return;
                }
                "a bool"
            }
            "String" | "str" | "&str" => {
                if found == Scalar::Str {
                    return;
                }
                "a string"
            }
            "char" => {
                if found == Scalar::Char {
                    return;
                }
                "a char"
            }
            _ => return,
        };
        self.report(
            node.start,
            node.end,
            RonSeverity::Warning,
            "ron-type-mismatch",
            format!("expected {expected} (`{name}`), found {}", found.describe()),
        );
    }

    fn check_def(&mut self, node: &Node, def: &TypeDef, depth: usize) {
        match def {
            TypeDef::Alias { target, .. } => self.check(node, target, depth + 1),
            TypeDef::Struct { name, fields, tuple_like } => {
                if *tuple_like {
                    if let Kind::Tuple { items, .. } = &node.kind {
                        self.check_positional(items, fields, depth);
                    }
                } else {
                    match &node.kind {
                        Kind::Struct { head, fields: written } => {
                            self.check_fields(head, written, fields, name, depth)
                        }
                        // `()` is an empty struct as far as required fields go.
                        Kind::Unit => {
                            let head = Head { name: None, start: node.start, end: node.end };
                            self.check_fields(&head, &[], fields, name, depth)
                        }
                        _ => {}
                    }
                }
            }
            TypeDef::Enum { name, variants } => self.check_enum(node, name, variants, depth),
        }
    }

    fn check_positional(&mut self, items: &[Node], fields: &[FieldDef], depth: usize) {
        for (item, field) in items.iter().zip(fields) {
            self.check(item, &field.ty, depth + 1);
        }
    }

    fn check_enum(&mut self, node: &Node, enum_name: &str, variants: &[VariantDef], depth: usize) {
        // Only a *named* node says which variant it means; an anonymous one is an untagged enum or a
        // newtype, and guessing there is how a correct file gets a squiggle.
        let (name, start, end) = match &node.kind {
            Kind::Ident(n) => (n.as_str(), node.start, node.end),
            Kind::Tuple { head, .. } | Kind::Struct { head, .. } => match &head.name {
                Some(n) => (n.as_str(), head.start, head.end),
                None => return,
            },
            _ => return,
        };
        let Some(variant) = variants.iter().find(|v| v.name == name) else {
            if !looks_renamed(name, variants.iter().map(|v| v.name.as_str())) {
                let hint = suggestion(name, variants.iter().map(|v| v.name.as_str()));
                self.report(
                    start,
                    end,
                    RonSeverity::Warning,
                    "ron-unknown-variant",
                    format!(
                        "unknown variant `{name}` of `{enum_name}`{hint}; expected one of: {}",
                        list(variants.iter().map(|v| v.name.as_str()))
                    ),
                );
            }
            return;
        };
        match (&node.kind, variant.shape) {
            (Kind::Tuple { items, .. }, VariantShape::Tuple) => {
                self.check_positional(items, &variant.fields, depth);
            }
            (Kind::Struct { head, fields: written }, VariantShape::Struct) => {
                self.check_fields(head, written, &variant.fields, &format!("{enum_name}::{name}"), depth);
            }
            _ => {}
        }
    }

    fn check_fields(&mut self, head: &Head, written: &[Field], defs: &[FieldDef], owner: &str, depth: usize) {
        let (known, open) = expand_flatten(self.schema, defs, 0);
        for w in written {
            match known.iter().find(|d| matches_name(d, &w.name)) {
                Some(def) => self.check(&w.value, &def.ty, depth + 1),
                None if open => {}
                None => {
                    let hint = suggestion(&w.name, known.iter().map(|d| d.name.as_str()));
                    self.report(
                        w.start,
                        w.end,
                        RonSeverity::Warning,
                        "ron-unknown-field",
                        format!(
                            "`{owner}` has no field `{}`{hint}; it has: {}",
                            w.name,
                            list(known.iter().map(|d| d.name.as_str()))
                        ),
                    );
                }
            }
        }
        let missing: Vec<&str> = known
            .iter()
            .filter(|d| !d.has_default && !d.skip_if_default && !is_option(&d.ty))
            .filter(|d| !written.iter().any(|w| matches_name(d, &w.name)))
            .map(|d| d.name.as_str())
            .collect();
        if !missing.is_empty() {
            // Info, not a warning: a container-level `#[serde(default)]` makes every one of these
            // optional, and the schema does not carry container attributes.
            self.report(
                head.start,
                head.end,
                RonSeverity::Info,
                "ron-missing-field",
                format!(
                    "`{owner}` is missing {} (unless the type is `#[serde(default)]`)",
                    missing.iter().map(|m| format!("`{m}`")).collect::<Vec<_>>().join(", ")
                ),
            );
        }
    }
}

fn matches_name(def: &FieldDef, written: &str) -> bool {
    def.name == written || def.aliases.iter().any(|a| a == written)
}

fn is_option(ty: &ResolvedType) -> bool {
    matches!(ty, ResolvedType::Option { .. })
}

/// The fields a struct accepts, with every `#[serde(flatten)]` of a struct spliced in. `open` is
/// true when a flattened field is something that can absorb any key (a map, an unresolved type),
/// in which case "no such field" cannot be claimed.
fn expand_flatten<'s>(schema: &'s Schema, defs: &'s [FieldDef], depth: usize) -> (Vec<&'s FieldDef>, bool) {
    let mut known = Vec::new();
    let mut open = false;
    for d in defs {
        if !d.flatten {
            known.push(d);
            continue;
        }
        let inner = match &d.ty {
            ResolvedType::Named { path } if depth < 8 => match schema.types.get(path) {
                Some(TypeDef::Struct { fields, tuple_like: false, .. }) => Some(fields),
                _ => None,
            },
            _ => None,
        };
        match inner {
            Some(fields) => {
                let (more, more_open) = expand_flatten(schema, fields, depth + 1);
                known.extend(more);
                open |= more_open;
            }
            None => open = true,
        }
    }
    (known, open)
}

/// `Dark` vs `dark` vs `DARK` vs `dark_mode` — the spellings a `rename_all` produces. The schema
/// does not carry variant renames, so a name that is one of those spellings of a real variant is
/// taken to be a rename and not reported.
fn looks_renamed<'a>(name: &str, variants: impl Iterator<Item = &'a str>) -> bool {
    let norm = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    let n = norm(name);
    variants.into_iter().any(|v| norm(v) == n)
}

fn list<'a>(names: impl Iterator<Item = &'a str>) -> String {
    let all: Vec<&str> = names.collect();
    let shown: Vec<String> = all.iter().take(8).map(|n| format!("`{n}`")).collect();
    let more = if all.len() > 8 { format!(", … ({} more)", all.len() - 8) } else { String::new() };
    format!("{}{more}", shown.join(", "))
}

/// ` (did you mean `x`?)` when one candidate is a small edit away, else nothing.
fn suggestion<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> String {
    let limit = (name.chars().count() / 3).max(2);
    candidates
        .map(|c| (edit_distance(&name.to_lowercase(), &c.to_lowercase()), c))
        .filter(|(d, _)| *d <= limit && *d < name.chars().count())
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| format!(" (did you mean `{c}`?)"))
        .unwrap_or_default()
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != *cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use arbor_studio_types::prelude::SchemaStats;

    use super::*;

    fn prim(name: &str) -> ResolvedType {
        ResolvedType::Primitive { name: name.into() }
    }

    fn named(path: &str) -> ResolvedType {
        ResolvedType::Named { path: path.into() }
    }

    fn field(name: &str, ty: ResolvedType) -> FieldDef {
        FieldDef {
            name: name.into(),
            ty,
            aliases: Vec::new(),
            has_default: false,
            skip_if_default: false,
            flatten: false,
        }
    }

    fn schema(types: Vec<(&str, TypeDef)>, root: &str) -> Schema {
        Schema {
            root_type: root.into(),
            root_name: root.rsplit("::").next().unwrap().into(),
            crate_manifest: String::new(),
            crate_name: "t".into(),
            types: types.into_iter().map(|(k, v)| (k.to_string(), v)).collect::<BTreeMap<_, _>>(),
            stats: SchemaStats::default(),
        }
    }

    fn config() -> Schema {
        let mut mode = field("mode", named("crate::Mode"));
        mode.has_default = true;
        let mut old = field("port", prim("u16"));
        old.aliases = vec!["listen_port".into()];
        schema(
            vec![
                (
                    "crate::Config",
                    TypeDef::Struct {
                        name: "Config".into(),
                        fields: vec![
                            old,
                            field("title", prim("String")),
                            field("nick", ResolvedType::Option { inner: Box::new(prim("String")) }),
                            mode,
                            field("servers", ResolvedType::Vec { inner: Box::new(named("crate::Server")) }),
                        ],
                        tuple_like: false,
                    },
                ),
                (
                    "crate::Server",
                    TypeDef::Struct {
                        name: "Server".into(),
                        fields: vec![field("host", prim("String"))],
                        tuple_like: false,
                    },
                ),
                (
                    "crate::Mode",
                    TypeDef::Enum {
                        name: "Mode".into(),
                        variants: vec![
                            VariantDef { name: "Fast".into(), shape: VariantShape::Unit, fields: vec![] },
                            VariantDef {
                                name: "Custom".into(),
                                shape: VariantShape::Struct,
                                fields: vec![field("level", prim("u8"))],
                            },
                            VariantDef {
                                name: "Wrapped".into(),
                                shape: VariantShape::Tuple,
                                fields: vec![field("0", prim("f32"))],
                            },
                        ],
                    },
                ),
            ],
            "crate::Config",
        )
    }

    fn codes(src: &str) -> Vec<&'static str> {
        validate(src, &config()).into_iter().map(|d| d.code).collect()
    }

    #[test]
    fn a_correct_document_is_silent() {
        let src = r#"(port: 80, title: "x", nick: Some("n"), mode: Custom(level: 3), servers: [(host: "a")])"#;
        assert_eq!(codes(src), Vec::<&str>::new());
    }

    #[test]
    fn an_unknown_field_is_flagged_on_its_name_with_a_suggestion() {
        let src = r#"(port: 80, titel: "x")"#;
        let found = validate(src, &config());
        let d = found.iter().find(|d| d.code == "ron-unknown-field").expect("unknown field");
        assert_eq!(&src[d.start..d.end], "titel");
        assert!(d.message.contains("did you mean `title`"), "{}", d.message);
    }

    #[test]
    fn a_missing_required_field_is_an_info_on_the_struct_head() {
        let src = "Config(port: 80)";
        let found = validate(src, &config());
        let d = found.iter().find(|d| d.code == "ron-missing-field").expect("missing");
        assert_eq!(d.severity, RonSeverity::Info);
        assert_eq!(&src[d.start..d.end], "Config");
        assert!(d.message.contains("`title`"));
        // `nick` is an Option and `mode` has a default: neither is required.
        assert!(!d.message.contains("`nick`") && !d.message.contains("`mode`"));
    }

    #[test]
    fn an_alias_is_accepted_as_the_field_it_names() {
        assert_eq!(codes(r#"(listen_port: 80, title: "x", servers: [])"#), Vec::<&str>::new());
    }

    #[test]
    fn an_unknown_variant_is_flagged_but_a_renamed_spelling_is_not() {
        let src = r#"(port: 1, title: "t", servers: [], mode: Slow)"#;
        let found = validate(src, &config());
        let d = found.iter().find(|d| d.code == "ron-unknown-variant").expect("variant");
        assert_eq!(&src[d.start..d.end], "Slow");
        assert!(d.message.contains("`Fast`"));
        // `fast` is what `rename_all = "lowercase"` would have produced.
        assert_eq!(codes(r#"(port: 1, title: "t", servers: [], mode: fast)"#), Vec::<&str>::new());
    }

    #[test]
    fn a_struct_variants_fields_and_a_tuple_variants_payload_are_checked() {
        let bad_field = r#"(port: 1, title: "t", servers: [], mode: Custom(lvl: 1))"#;
        assert!(codes(bad_field).contains(&"ron-unknown-field"));
        let bad_payload = r#"(port: 1, title: "t", servers: [], mode: Wrapped("x"))"#;
        assert!(codes(bad_payload).contains(&"ron-type-mismatch"));
    }

    #[test]
    fn nested_elements_are_walked() {
        let src = r#"(port: 1, title: "t", servers: [(host: "a"), (hots: "b")])"#;
        let found = validate(src, &config());
        let d = found.iter().find(|d| d.code == "ron-unknown-field").unwrap();
        assert_eq!(&src[d.start..d.end], "hots");
    }

    #[test]
    fn a_scalar_of_the_wrong_kind_is_a_warning_on_the_value() {
        let src = r#"(port: "eighty", title: "t", servers: [])"#;
        let found = validate(src, &config());
        let d = found.iter().find(|d| d.code == "ron-type-mismatch").unwrap();
        assert_eq!(&src[d.start..d.end], "\"eighty\"");
        assert_eq!(d.severity, RonSeverity::Warning);
    }

    #[test]
    fn option_accepts_some_none_and_an_implicit_bare_value() {
        for nick in ["Some(\"n\")", "None", "\"n\""] {
            let src = format!(r#"(port: 1, title: "t", servers: [], nick: {nick})"#);
            assert_eq!(codes(&src), Vec::<&str>::new(), "{nick}");
        }
    }

    #[test]
    fn a_flattened_map_means_no_field_is_unknown() {
        let mut extra = field("extra", ResolvedType::Map { key: Box::new(prim("String")), value: Box::new(prim("String")) });
        extra.flatten = true;
        let s = schema(
            vec![(
                "crate::C",
                TypeDef::Struct { name: "C".into(), fields: vec![field("a", prim("u8")), extra], tuple_like: false },
            )],
            "crate::C",
        );
        assert!(validate("(a: 1, whatever: \"x\")", &s).is_empty());
    }

    #[test]
    fn a_flattened_struct_contributes_its_fields() {
        let mut inner = field("base", named("crate::Base"));
        inner.flatten = true;
        let s = schema(
            vec![
                ("crate::C", TypeDef::Struct { name: "C".into(), fields: vec![field("a", prim("u8")), inner], tuple_like: false }),
                ("crate::Base", TypeDef::Struct { name: "Base".into(), fields: vec![field("id", prim("u32"))], tuple_like: false }),
            ],
            "crate::C",
        );
        assert!(validate("(a: 1, id: 7)", &s).is_empty());
        assert_eq!(validate("(a: 1, idd: 7)", &s).iter().filter(|d| d.code == "ron-unknown-field").count(), 1);
    }

    #[test]
    fn text_that_does_not_scan_yields_nothing() {
        assert!(validate("(port: ", &config()).is_empty());
    }

    #[test]
    fn a_shape_the_type_does_not_describe_is_left_alone() {
        // An anonymous value where an enum is expected: could be `untagged`. Silence.
        assert_eq!(codes(r#"(port: 1, title: "t", servers: [], mode: (1, 2))"#), Vec::<&str>::new());
    }

    #[test]
    fn edit_distance_counts_single_edits() {
        assert_eq!(edit_distance("title", "titel"), 2);
        assert_eq!(edit_distance("host", "hots"), 2);
        assert_eq!(edit_distance("a", "a"), 0);
    }
}

/// The whole chain on a real crate on disk: `syn` reads the source, the loader resolves the serde
/// attributes, and the validator reports against what came out.
#[cfg(test)]
mod end_to_end {
    use std::fs;

    use super::validate;
    use crate::directive::schema_hint;

    const LIB: &str = r#"
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub window_title: String,
    #[serde(alias = "vsync")]
    pub v_sync: bool,
    #[serde(default)]
    pub volume: f32,
    pub audio: Audio,
}

#[derive(Deserialize)]
pub struct Audio { pub device: Option<String>, pub channels: u8 }
"#;

    #[test]
    fn a_directive_loads_the_crate_and_the_serde_names_are_what_is_checked() {
        let dir = std::env::temp_dir().join(format!("bennu-ron-e2e-{}", std::process::id()));
        let src_dir = dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(dir.join("Cargo.toml"), "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
        fs::write(src_dir.join("lib.rs"), LIB).unwrap();
        let ron_path = dir.join("settings.ron");

        let text = "// ron-studio: schema=\"src/lib.rs\", root=\"crate::Settings\"\n\
                    (windowTitle: \"x\", vsync: true, audio: (channels: 2), volumn: 1.0)";
        let hint = schema_hint(text, &ron_path.to_string_lossy()).expect("directive");
        let schema = arbor_studio_ron::schema::load(&hint.rs_file, &hint.root_type).expect("schema loads");
        let found = validate(text, &schema);
        let _ = fs::remove_dir_all(&dir);

        let msgs: Vec<String> = found.iter().map(|d| format!("{}: {}", d.code, &text[d.start..d.end])).collect();
        // `windowTitle` (rename_all) and `vsync` (alias) are known; only the typo is not.
        assert_eq!(msgs, vec!["ron-unknown-field: volumn".to_string()], "{msgs:?}");
    }
}
