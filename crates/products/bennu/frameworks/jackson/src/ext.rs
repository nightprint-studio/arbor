//! The extension — what a host registers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use bennu_ext::prelude::{ExtHover, ExtStat, FileCtx, FrameworkExtension, ProjectScan};
use bennu_proto::prelude::{CapabilitySet, Diagnostic};

use crate::creator::{creators_in, parameter_names, ParameterNames};
use crate::dto::{dtos_in, Dto, MemberKind, Property};

pub const CODE_DUPLICATE: &str = "jackson.duplicate-property";
pub const CODE_CONTRADICTORY: &str = "jackson.contradictory";
pub const CODE_UNREACHABLE: &str = "jackson.unreachable-field";
pub const CODE_UNNAMED_CREATOR: &str = "jackson.unnamed-creator";

#[derive(Default)]
pub struct JacksonExtension {
    scanned: AtomicBool,
    /// How many DTOs the project has, for the overview.
    dtos: std::sync::atomic::AtomicUsize,
    /// Whether the build keeps parameter names — the second half of the `@JsonCreator` defect, and
    /// the only thing here that is a fact about the PROJECT rather than about one file. Held as a
    /// number because it is read on every diagnostics call and written once per scan.
    names: AtomicU8,
}

/// [`ParameterNames`] as the byte kept in the atomic.
fn encode(names: ParameterNames) -> u8 {
    match names {
        ParameterNames::Unknown => 0,
        ParameterNames::Kept => 1,
        ParameterNames::Dropped => 2,
    }
}

fn decode(raw: u8) -> ParameterNames {
    match raw {
        1 => ParameterNames::Kept,
        2 => ParameterNames::Dropped,
        _ => ParameterNames::Unknown,
    }
}

impl JacksonExtension {
    pub fn new() -> Self {
        Self::default()
    }
}

impl FrameworkExtension for JacksonExtension {
    fn id(&self) -> &'static str {
        "jackson"
    }

    fn display_name(&self) -> &'static str {
        "Jackson"
    }

    fn applies(&self, caps: &CapabilitySet) -> bool {
        caps.jackson
    }

    fn reindex(&self, scan: &ProjectScan<'_>) {
        let count =
            scan.java.iter().map(|f| dtos_in(&f.text).iter().filter(|d| d.jackson).count()).sum();
        self.dtos.store(count, Ordering::Release);

        // The compiler setting, from every pom in the reactor. Read here rather than per buffer
        // because it is one fact about the build, and because finding it means reading files that
        // have nothing to do with the class in front of you.
        let poms: Vec<String> = scan
            .xml
            .iter()
            .filter(|f| f.path.file_name().map(|n| n == "pom.xml").unwrap_or(false))
            .map(|f| f.text.clone())
            .collect();
        self.names.store(encode(parameter_names(&poms)), Ordering::Release);

        self.scanned.store(true, Ordering::Release);
    }

    fn is_ready(&self) -> bool {
        self.scanned.load(Ordering::Acquire)
    }

    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        if ctx.extension() != "java" {
            return Vec::new();
        }
        let mut out: Vec<Diagnostic> = dtos_in(ctx.source).iter().flat_map(check_dto).collect();

        // The half that needs the build file. Silent on `Unknown` — a Gradle project, or a scan
        // that has not landed — because not knowing is not evidence.
        for creator in creators_in(ctx.source, decode(self.names.load(Ordering::Acquire))) {
            out.push(Diagnostic {
                message: format!(
                    "Jackson cannot name the {} arguments of this creator: the build does not \
                     compile with `-parameters`, so deserialising {} fails with \"Argument #0 of \
                     constructor has no property name annotation\". Annotate each argument with \
                     @JsonProperty, or set <maven.compiler.parameters>true</maven.compiler.parameters>",
                    creator.arity, creator.owner
                ),
                severity: "error".to_string(),
                code: CODE_UNNAMED_CREATOR.to_string(),
                start: creator.start,
                end: creator.end,
            });
        }
        out.sort_by_key(|d| d.start);
        out
    }

    /// What a member will be called in the JSON — the question you open the class to answer.
    fn hover(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<ExtHover> {
        if ctx.extension() != "java" {
            return None;
        }
        for dto in dtos_in(ctx.source) {
            if !dto.jackson {
                continue;
            }
            let Some(property) =
                dto.properties.iter().find(|p| offset >= p.start && offset <= p.end)
            else {
                continue;
            };
            let doc = if property.ignored {
                "Not serialised — this member carries @JsonIgnore.".to_string()
            } else if property.explicit {
                "Named explicitly by @JsonProperty.".to_string()
            } else {
                "Named after the member, which is Jackson's default.".to_string()
            };
            return Some(ExtHover {
                title: format!("\"{}\"", property.name),
                signature: format!("{}.{}", dto.name, property.member),
                doc,
            });
        }
        None
    }

    fn stats(&self) -> Vec<ExtStat> {
        vec![ExtStat {
            label: "JSON types".to_string(),
            value: self.dtos.load(Ordering::Acquire),
            catalog: None,
        }]
    }
}

/// The three things that can be said about one class with certainty.
fn check_dto(dto: &Dto) -> Vec<Diagnostic> {
    if !dto.jackson {
        return Vec::new();
    }
    let mut out = Vec::new();

    // ── two members claiming one name ────────────────────────────────────────
    //
    // Jackson throws when such an object is serialised, which in practice is in front of a
    // customer — the class itself compiles and looks fine.
    let mut by_name: HashMap<&str, Vec<&Property>> = HashMap::new();
    for property in dto.properties.iter().filter(|p| !p.ignored) {
        by_name.entry(property.name.as_str()).or_default().push(property);
    }
    for (name, members) in &by_name {
        // A getter and a setter for one property are the ordinary case, not a conflict — and so is
        // a field with its own accessor. What conflicts is two members of the SAME kind, or two
        // that both name themselves explicitly.
        let explicit: Vec<&&Property> = members.iter().filter(|p| p.explicit).collect();
        let clashing = explicit.len() > 1
            || members.iter().filter(|p| p.kind == MemberKind::Field).count() > 1;
        if !clashing {
            continue;
        }
        for property in members.iter().filter(|p| p.explicit || p.kind == MemberKind::Field) {
            out.push(Diagnostic {
                message: format!(
                    "two members of {} are called `{name}` in JSON — Jackson refuses to serialise \
                     a type with a conflicting property",
                    dto.name
                ),
                severity: "error".to_string(),
                code: CODE_DUPLICATE.to_string(),
                start: property.start,
                end: property.end,
            });
        }
    }

    for property in &dto.properties {
        // ── an element that says both things ─────────────────────────────────
        if property.ignored && property.explicit {
            out.push(Diagnostic {
                message: format!(
                    "`{}` carries both @JsonIgnore and @JsonProperty — Jackson resolves this by \
                     dropping the property, which is unlikely to be what the @JsonProperty meant",
                    property.member
                ),
                severity: "warning".to_string(),
                code: CODE_CONTRADICTORY.to_string(),
                start: property.start,
                end: property.end,
            });
        }
    }

    // ── a field nothing can reach ────────────────────────────────────────────
    //
    // Only where the class says plainly what it is. Jackson's visibility is configurable globally
    // and per class, and Lombok writes accessors that are not in the file — either of those makes
    // this unanswerable, so either of them silences it.
    if !dto.auto_detect && !dto.lombok_accessors {
        let reachable: Vec<&str> = dto
            .properties
            .iter()
            .filter(|p| p.kind != MemberKind::Field)
            .map(|p| p.member.as_str())
            .collect();
        for field in dto
            .properties
            .iter()
            .filter(|p| p.kind == MemberKind::Field && !p.public && !p.ignored && !p.explicit)
        {
            if reachable.contains(&field.member.as_str()) {
                continue;
            }
            out.push(Diagnostic {
                message: format!(
                    "`{}` will not appear in the JSON — it is not public and has no accessor, so \
                     Jackson cannot see it",
                    field.member
                ),
                severity: "warning".to_string(),
                code: CODE_UNREACHABLE.to_string(),
                start: field.start,
                end: field.end,
            });
        }
    }

    out.sort_by_key(|d| d.start);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostics(src: &str) -> Vec<Diagnostic> {
        dtos_in(src).iter().flat_map(check_dto).collect()
    }

    /// The one that reaches production: two members that both serialise to `id`.
    #[test]
    fn two_members_claiming_one_name_is_an_error() {
        let src = r#"class Order {
            @JsonProperty("id") private String codice;
            @JsonProperty("id") private String numero;
            public String getCodice() { return codice; }
            public String getNumero() { return numero; }
        }"#;
        let found = diagnostics(src);
        assert_eq!(found.iter().filter(|d| d.code == CODE_DUPLICATE).count(), 2);
        assert_eq!(found[0].severity, "error");
    }

    /// A getter and its field are one property, not two. Reporting them would fire on every DTO.
    #[test]
    fn a_field_and_its_getter_are_one_property() {
        let src = r#"class Order {
            @JsonProperty private String nome;
            public String getNome() { return nome; }
        }"#;
        assert!(diagnostics(src).iter().all(|d| d.code != CODE_DUPLICATE), "{:?}", diagnostics(src));
    }

    #[test]
    fn a_field_with_no_way_out_is_reported() {
        let src = r#"class Order {
            @JsonProperty private String nome;
            private String segreto;
            public String getNome() { return nome; }
        }"#;
        let found = diagnostics(src);
        let issue = found.iter().find(|d| d.code == CODE_UNREACHABLE).expect("the unreachable one");
        assert!(issue.message.contains("segreto"), "{}", issue.message);
    }

    /// The gate that keeps this from firing on every DTO in the project: Lombok writes the
    /// accessors, and they are not in the file.
    #[test]
    fn a_lombok_class_is_not_judged_on_its_accessors() {
        let src = r#"@Data
        class Order {
            @JsonProperty private String nome;
            private String altro;
        }"#;
        assert!(diagnostics(src).iter().all(|d| d.code != CODE_UNREACHABLE));
    }

    /// And so does a class that sets its own visibility rules.
    #[test]
    fn a_class_that_configures_visibility_is_not_judged_either() {
        let src = r#"@JsonAutoDetect(fieldVisibility = Visibility.ANY)
        class Order {
            @JsonProperty private String nome;
            private String altro;
        }"#;
        assert!(diagnostics(src).iter().all(|d| d.code != CODE_UNREACHABLE));
    }

    /// A class with no Jackson annotation at all is not a DTO, whatever else the project does.
    #[test]
    fn a_class_that_is_not_jacksons_business_is_left_alone() {
        let src = "class Helper { private String a; public void go() { } }";
        assert!(diagnostics(src).is_empty());
    }

    #[test]
    fn an_element_that_says_both_things_is_reported() {
        let src = r#"class Order {
            @JsonIgnore @JsonProperty("nome") private String nome;
            public String getNome() { return nome; }
        }"#;
        let found = diagnostics(src);
        assert!(found.iter().any(|d| d.code == CODE_CONTRADICTORY), "{found:?}");
    }

    /// A public field is reachable without an accessor, and a `transient` or `static` one is not a
    /// property at all.
    #[test]
    fn the_fields_that_are_not_properties_are_not_reported() {
        let src = r#"class Order {
            @JsonProperty private String nome;
            public String pubblico;
            private static String COSTANTE;
            private transient String cache;
            public String getNome() { return nome; }
        }"#;
        assert!(diagnostics(src).iter().all(|d| d.code != CODE_UNREACHABLE), "{:?}", diagnostics(src));
    }
}
