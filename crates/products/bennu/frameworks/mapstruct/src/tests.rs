//! End-to-end: a small project indexed through the extension, then asked about a mapper buffer the
//! way the editor asks. Every check has its silence cases next to it — they are the ones that keep
//! the crate usable.

use std::path::{Path, PathBuf};

use bennu_ext::prelude::{ExtEdit, ExtIntention, ExtProblem, FileCtx, FrameworkExtension, ProjectScan, ScannedFile};
use bennu_proto::prelude::Diagnostic;

use crate::prelude::*;

const MAPPER_PATH: &str = "/p/src/main/java/com/acme/UserMapper.java";

const USER: &str = "package com.acme;\n\npublic class User {\n    private Long id;\n    private String firstName;\n    private String email;\n    public Long getId() { return id; }\n    public String getFirstName() { return firstName; }\n    public String getEmail() { return email; }\n}\n";

const USER_DTO: &str = "package com.acme.dto;\n\npublic class UserDto {\n    private Long id;\n    private String name;\n    private String email;\n    private String nickname;\n    public void setId(Long id) { this.id = id; }\n    public void setName(String name) { this.name = name; }\n    public void setEmail(String email) { this.email = email; }\n    public void setNickname(String nickname) { this.nickname = nickname; }\n}\n";

/// The mapping the first test is about: `name` from `firstName`, `nickname` from nothing.
const TO_DTO: &str = "    @Mapping(target = \"name\", source = \"firstName\")\n    UserDto toDto(User user);";

fn mapper_with(imports: &str, args: &str, body: &str) -> String {
    format!(
        "package com.acme;\n\nimport com.acme.dto.UserDto;\n{imports}import org.mapstruct.Mapper;\nimport org.mapstruct.Mapping;\n\n@Mapper({args})\npublic interface UserMapper {{\n{body}\n}}\n"
    )
}

fn mapper(args: &str, body: &str) -> String {
    mapper_with("", args, body)
}

fn file(path: &str, text: &str) -> ScannedFile {
    ScannedFile { path: PathBuf::from(path), text: text.to_string() }
}

fn project(mapper_src: &str, extra: &[(&str, &str)], pom: bool) -> MapStructExtension {
    let mut java = vec![
        file("/p/src/main/java/com/acme/User.java", USER),
        file("/p/src/main/java/com/acme/dto/UserDto.java", USER_DTO),
        file(MAPPER_PATH, mapper_src),
    ];
    java.extend(extra.iter().map(|(p, t)| file(p, t)));
    let xml = if pom { vec![file("/p/pom.xml", "<project/>")] } else { Vec::new() };
    let ext = MapStructExtension::new();
    ext.reindex(&ProjectScan { java: &java, xml: &xml, ..ProjectScan::empty(Path::new("/p")) });
    ext
}

fn ctx(src: &str) -> FileCtx<'_> {
    FileCtx { path: Path::new(MAPPER_PATH), source: src }
}

fn diagnostics_in(ext: &MapStructExtension, src: &str) -> Vec<Diagnostic> {
    ext.diagnostics(&ctx(src))
}

fn check(src: &str) -> Vec<Diagnostic> {
    diagnostics_in(&project(src, &[], true), src)
}

fn check_with(src: &str, extra: &[(&str, &str)]) -> Vec<Diagnostic> {
    diagnostics_in(&project(src, extra, true), src)
}

fn codes(found: &[Diagnostic]) -> Vec<&str> {
    found.iter().map(|d| d.code.as_str()).collect()
}

fn one<'a>(found: &'a [Diagnostic], code: &str) -> &'a Diagnostic {
    let hits: Vec<&Diagnostic> = found.iter().filter(|d| d.code == code).collect();
    assert_eq!(hits.len(), 1, "expected one {code} in {found:#?}");
    hits[0]
}

fn problem(d: &Diagnostic) -> ExtProblem {
    ExtProblem { code: d.code.clone(), start: d.start, end: d.end }
}

fn offer<'a>(offers: &'a [ExtIntention], id: &str) -> &'a ExtIntention {
    offers.iter().find(|i| i.id == id).unwrap_or_else(|| panic!("no {id} in {offers:#?}"))
}

fn apply(src: &str, edits: &[ExtEdit]) -> String {
    let mut edits = edits.to_vec();
    edits.sort_by(|a, b| b.start.cmp(&a.start));
    let mut out = src.to_string();
    for e in edits {
        out.replace_range(e.start..e.end, &e.text);
    }
    out
}

// ── unmapped targets ─────────────────────────────────────────────────────────

#[test]
fn an_unmapped_target_property_is_warned_on_the_method_name() {
    let src = mapper("componentModel = \"spring\"", TO_DTO);
    let found = check(&src);
    assert_eq!(codes(&found), [CODE_UNMAPPED], "{found:#?}");
    let d = &found[0];
    assert_eq!(d.severity, "warning");
    assert_eq!(&src[d.start..d.end], "toDto");
    assert!(d.message.contains("nickname"), "{}", d.message);
    assert!(!d.message.contains("email"), "mapped implicitly from User.getEmail: {}", d.message);
}

#[test]
fn the_error_policy_makes_it_an_error() {
    let src = mapper("unmappedTargetPolicy = ReportingPolicy.ERROR", TO_DTO);
    assert_eq!(one(&check(&src), CODE_UNMAPPED).severity, "error");
}

/// Each of these leaves the answer somewhere this crate cannot see — or says it on purpose.
#[test]
fn unmapped_targets_are_not_reported_when_they_cannot_be_known() {
    let silent = [
        mapper("unmappedTargetPolicy = ReportingPolicy.IGNORE", TO_DTO),
        mapper("", "    @org.mapstruct.BeanMapping(ignoreByDefault = true)\n    UserDto toDto(User user);"),
        mapper("", "    @org.mapstruct.InheritConfiguration\n    UserDto toDto(User user);"),
        mapper("", "    @ToDto\n    UserDto toDto(User user);"),
        mapper("", "    java.util.List<UserDto> toDtos(java.util.List<User> users);"),
        mapper("", "    @Mapping(target = \".\", source = \"user\")\n    UserDto toDto(User user);"),
        mapper("", "    @Mapping(target = NAME, source = \"firstName\")\n    UserDto toDto(User user);"),
    ];
    for src in &silent {
        let found = check(src);
        assert!(found.iter().all(|d| d.code != CODE_UNMAPPED), "{src}\n{found:#?}");
    }
}

#[test]
fn a_build_whose_default_policy_cannot_be_read_says_nothing_about_unmapped_targets() {
    let src = mapper("", TO_DTO);
    let found = diagnostics_in(&project(&src, &[], false), &src);
    assert!(found.is_empty(), "no pom — a Gradle build's compiler arguments are not visible: {found:#?}");
}

#[test]
fn a_config_class_sets_the_policy_and_an_unreadable_one_silences_it() {
    let config = |policy: &str| {
        format!(
            "package com.acme;\n\nimport org.mapstruct.MapperConfig;\nimport org.mapstruct.ReportingPolicy;\n\n@MapperConfig(unmappedTargetPolicy = ReportingPolicy.{policy})\npublic interface CentralConfig {{}}\n"
        )
    };
    let path = "/p/src/main/java/com/acme/CentralConfig.java";
    let src = mapper("config = CentralConfig.class", TO_DTO);

    let ignoring = config("IGNORE");
    assert!(check_with(&src, &[(path, ignoring.as_str())]).is_empty());
    let failing = config("ERROR");
    assert_eq!(one(&check_with(&src, &[(path, failing.as_str())]), CODE_UNMAPPED).severity, "error");

    let missing = mapper("config = SomewhereElse.class", TO_DTO);
    assert!(check(&missing).is_empty(), "a config that cannot be read is not a WARN");
}

#[test]
fn a_type_from_a_jar_is_not_a_type_with_no_properties() {
    let src = mapper_with(
        "import org.lib.External;\n",
        "",
        "    @Mapping(target = \"name\", source = \"whatever\")\n    UserDto fromExternal(External external);",
    );
    assert!(check(&src).is_empty());
}

#[test]
fn a_target_whose_supertype_is_not_in_the_project_is_left_alone() {
    let paged = "package com.acme.dto;\n\nimport org.lib.Base;\n\npublic class PagedDto extends Base {\n    public void setTotal(int total) {}\n}\n";
    let src = mapper_with(
        "import com.acme.dto.PagedDto;\n",
        "",
        "    @Mapping(target = \"totl\", ignore = true)\n    PagedDto toPaged(User user);",
    );
    let found = check_with(&src, &[("/p/src/main/java/com/acme/dto/PagedDto.java", paged)]);
    assert!(found.is_empty(), "Base may declare `totl` and a dozen unmapped setters: {found:#?}");
}

#[test]
fn a_record_and_a_lombok_dto_are_read_for_their_properties() {
    let record = "package com.acme.dto;\n\npublic record PersonRecord(String firstName, String phone) {}\n";
    let lombok = "package com.acme.dto;\n\nimport lombok.Data;\n\n@Data\npublic class LombokDto {\n    private String email;\n    private String phone;\n}\n";
    let src = mapper_with(
        "import com.acme.dto.LombokDto;\nimport com.acme.dto.PersonRecord;\n",
        "",
        "    PersonRecord toRecord(User user);\n\n    LombokDto toLombok(User user);",
    );
    let found = check_with(
        &src,
        &[
            ("/p/src/main/java/com/acme/dto/PersonRecord.java", record),
            ("/p/src/main/java/com/acme/dto/LombokDto.java", lombok),
        ],
    );
    let unmapped: Vec<&str> = found.iter().filter(|d| d.code == CODE_UNMAPPED).map(|d| &src[d.start..d.end]).collect();
    assert_eq!(unmapped, ["toRecord", "toLombok"], "{found:#?}");
    assert!(found.iter().all(|d| d.message.contains("phone") && !d.message.contains("email")));
}

#[test]
fn a_same_named_source_parameter_maps_implicitly() {
    let src = mapper("", "    @Mapping(target = \"name\", source = \"user.firstName\")\n    UserDto merge(User user, String nickname);");
    assert!(check(&src).is_empty(), "{:#?}", check(&src));
}

// ── unknown properties ───────────────────────────────────────────────────────

#[test]
fn an_unknown_target_property_is_an_error_on_its_segment_with_a_fix() {
    let src = mapper("", "    @Mapping(target = \"nmae\", source = \"firstName\")\n    UserDto toDto(User user);");
    let ext = project(&src, &[], true);
    let found = diagnostics_in(&ext, &src);
    let d = one(&found, CODE_UNKNOWN_TARGET);
    assert_eq!(&src[d.start..d.end], "nmae");
    assert_eq!(d.severity, "error");

    let offers = ext.intentions(&ctx(&src), d.start, &[problem(d)]);
    let fixed = apply(&src, &offer(&offers, INTENT_DID_YOU_MEAN).edits);
    assert!(fixed.contains("target = \"name\""), "{fixed}");
}

#[test]
fn an_unknown_source_property_is_an_error_with_a_fix() {
    let src = mapper("", "    @Mapping(target = \"name\", source = \"frstName\")\n    UserDto toDto(User user);");
    let ext = project(&src, &[], true);
    let found = diagnostics_in(&ext, &src);
    let d = one(&found, CODE_UNKNOWN_SOURCE);
    assert_eq!(&src[d.start..d.end], "frstName");
    let offers = ext.intentions(&ctx(&src), d.start, &[problem(d)]);
    let fixed = apply(&src, &offer(&offers, INTENT_DID_YOU_MEAN).edits);
    assert!(fixed.contains("source = \"firstName\""), "{fixed}");
}

#[test]
fn with_several_sources_only_a_path_through_a_parameter_is_checked() {
    let through = mapper("", "    @Mapping(target = \"name\", source = \"user.frstName\")\n    UserDto merge(User user, String nickname);");
    assert_eq!(&through[one(&check(&through), CODE_UNKNOWN_SOURCE).start..][..8], "frstName");
    let bare = mapper("", "    @Mapping(target = \"name\", source = \"frstName\")\n    UserDto merge(User user, String nickname);");
    assert!(check(&bare).iter().all(|d| d.code != CODE_UNKNOWN_SOURCE));
}

#[test]
fn a_nested_target_follows_property_types() {
    let order = "package com.acme.dto;\n\npublic class OrderDto {\n    public void setAddress(AddressDto address) {}\n}\n";
    let address = "package com.acme.dto;\n\npublic class AddressDto {\n    public void setStreet(String street) {}\n}\n";
    let src = mapper_with(
        "import com.acme.dto.OrderDto;\n",
        "",
        "    @Mapping(target = \"address.stret\", source = \"email\")\n    OrderDto toOrder(User user);",
    );
    let found = check_with(
        &src,
        &[
            ("/p/src/main/java/com/acme/dto/OrderDto.java", order),
            ("/p/src/main/java/com/acme/dto/AddressDto.java", address),
        ],
    );
    let d = one(&found, CODE_UNKNOWN_TARGET);
    assert_eq!(&src[d.start..d.end], "stret");
    assert!(d.message.contains("AddressDto"), "{}", d.message);
}

#[test]
fn mappings_nested_in_mappings_are_checked_like_any_other() {
    let body = "    @Mappings({\n        @Mapping(target = \"name\", source = \"firstName\"),\n        @Mapping(target = \"nickname\", ignore = true),\n        @Mapping(target = \"emial\", ignore = true)\n    })\n    UserDto toDto(User user);";
    let src = mapper_with("import org.mapstruct.Mappings;\n", "", body);
    let found = check(&src);
    assert_eq!(codes(&found), [CODE_UNKNOWN_TARGET], "{found:#?}");
    assert_eq!(&src[found[0].start..found[0].end], "emial");
}

// ── refused combinations ─────────────────────────────────────────────────────

#[test]
fn a_target_mapped_twice_and_refused_elements_are_errors() {
    let src = mapper(
        "",
        "    @Mapping(target = \"name\", source = \"firstName\")\n    @Mapping(target = \"name\", constant = \"x\", source = \"email\")\n    @Mapping(target = \"nickname\", ignore = true)\n    UserDto toDto(User user);",
    );
    let found = check(&src);
    let duplicate = one(&found, CODE_DUPLICATE_TARGET);
    assert_eq!(&src[duplicate.start..duplicate.end], "name");
    assert!(duplicate.start > src.find("\"name\"").unwrap() + 1, "the second one is the duplicate");
    let conflict = one(&found, CODE_CONFLICTING);
    assert!(src[conflict.start..conflict.end].contains("constant = \"x\""));
}

// ── ignore-unmapped fix ──────────────────────────────────────────────────────

#[test]
fn ignoring_the_unmapped_properties_writes_mappings_that_silence_the_warning() {
    let src = mapper("", TO_DTO);
    let ext = project(&src, &[], true);
    let d = one(&diagnostics_in(&ext, &src), CODE_UNMAPPED).clone();
    let offers = ext.intentions(&ctx(&src), d.start, &[problem(&d)]);
    let fix = offer(&offers, INTENT_IGNORE_UNMAPPED);
    let fixed = apply(&src, &fix.edits);
    assert!(
        fixed.contains("    @Mapping(target = \"nickname\", ignore = true)\n    @Mapping(target = \"name\", source = \"firstName\")\n    UserDto toDto"),
        "{fixed}"
    );
    assert!(diagnostics_in(&ext, &fixed).is_empty(), "the fix leaves nothing to report");
}

#[test]
fn the_ignore_fix_imports_mapping_when_the_file_does_not() {
    let src = "package com.acme;\n\nimport com.acme.dto.UserDto;\nimport org.mapstruct.Mapper;\n\n@Mapper\npublic interface UserMapper {\n    UserDto toDto(User user);\n}\n";
    let ext = project(src, &[], true);
    let d = one(&diagnostics_in(&ext, src), CODE_UNMAPPED).clone();
    let offers = ext.intentions(&ctx(src), d.start, &[problem(&d)]);
    let fix = offer(&offers, INTENT_IGNORE_UNMAPPED);
    assert_eq!(fix.label, "Ignore 2 unmapped target properties");
    assert_eq!(
        apply(src, &fix.edits),
        "package com.acme;\n\nimport com.acme.dto.UserDto;\nimport org.mapstruct.Mapper;\nimport org.mapstruct.Mapping;\n\n@Mapper\npublic interface UserMapper {\n    @Mapping(target = \"name\", ignore = true)\n    @Mapping(target = \"nickname\", ignore = true)\n    UserDto toDto(User user);\n}\n"
    );
}

#[test]
fn a_fix_for_a_problem_the_buffer_no_longer_has_offers_nothing() {
    let src = mapper("", TO_DTO);
    let ext = project(&src, &[], true);
    let stale = ExtProblem { code: CODE_UNMAPPED.to_string(), start: 3, end: 8 };
    assert!(ext.intentions(&ctx(&src), 3, &[stale]).is_empty());
}

// ── inside the strings ───────────────────────────────────────────────────────

#[test]
fn completion_offers_target_properties_and_replaces_only_the_segment() {
    let src = mapper("", "    @Mapping(target = \"ni\", source = \"firstName\")\n    UserDto toDto(User user);");
    let ext = project(&src, &[], true);
    let at = src.find("\"ni\"").unwrap() + 1;
    let items = ext.completions(&ctx(&src), at + 2);
    let nickname = items.iter().find(|i| i.label == "nickname").expect("nickname offered");
    assert_eq!((nickname.replace_start, nickname.replace_end), (Some(at), Some(at + 2)));
    assert!(items.iter().all(|i| i.label.starts_with("ni")), "{items:#?}");
}

#[test]
fn go_to_and_hover_on_a_source_segment_reach_the_declaration() {
    let src = mapper("", TO_DTO);
    let ext = project(&src, &[], true);
    let at = src.find("firstName").unwrap() + 2;
    let targets = ext.navigate(&ctx(&src), at);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].file, "/p/src/main/java/com/acme/User.java");
    assert_eq!(targets[0].offset, USER.find("firstName;").unwrap(), "the field, not the getter");

    let hover = ext.hover(&ctx(&src), at).expect("a hover");
    assert_eq!(hover.signature, "User.firstName : String");
    assert!(hover.doc.contains("getter"), "{}", hover.doc);
}

// ── panel and gutter ─────────────────────────────────────────────────────────

#[test]
fn the_mappers_panel_lists_each_method_with_its_unmapped_properties() {
    let src = mapper("componentModel = \"spring\"", TO_DTO);
    let ext = project(&src, &[], true);
    let rows = ext.catalog("mappers");
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!((row.primary.as_str(), row.secondary.as_str(), row.kind.as_str()), ("UserMapper", "com.acme.UserMapper", "@Mapper"));
    assert_eq!(row.tags, ["spring", "1 unmapped"]);
    let method = &row.children[0];
    assert_eq!(method.id, "com.acme.UserMapper#toDto");
    assert_eq!((method.primary.as_str(), method.secondary.as_str()), ("toDto(User)", "User → UserDto"));
    assert_eq!(method.kind, "mapping");
    assert_eq!(method.tags, ["unmapped: nickname"]);
    let stats = ext.stats();
    assert_eq!((stats[0].value, stats[0].catalog.as_deref()), (1, Some("mappers")));
    assert!(ext.catalog("beans").is_empty());
}

#[test]
fn a_mapping_method_with_a_generated_implementation_gets_a_gutter_mark() {
    let generated = "package com.acme;\n\nimport javax.annotation.processing.Generated;\n\n@Generated(value = \"org.mapstruct.ap.MappingProcessor\")\npublic class UserMapperImpl implements UserMapper {\n    @Override\n    public UserDto toDto(User user) { return null; }\n}\n";
    let path = "/p/target/generated-sources/annotations/com/acme/UserMapperImpl.java";
    let src = mapper("", TO_DTO);
    let ext = project(&src, &[(path, generated)], true);
    let marks = ext.gutter(&ctx(&src));
    assert_eq!(marks.len(), 1, "{marks:#?}");
    assert_eq!(marks[0].kind, "impl");
    assert_eq!(marks[0].tooltip, "Implemented in UserMapperImpl");
    assert_eq!(marks[0].line, line_of(&src, src.find("toDto").unwrap()));
    assert_eq!((marks[0].targets[0].file.as_str(), marks[0].targets[0].offset), (path, generated.find("toDto").unwrap()));

    assert!(project(&src, &[], true).gutter(&ctx(&src)).is_empty(), "nothing generated, nothing to jump to");
}

#[test]
fn before_a_scan_nothing_that_needs_the_project_is_said() {
    let ext = MapStructExtension::new();
    let src = mapper("", TO_DTO);
    assert!(!ext.is_ready());
    assert!(diagnostics_in(&ext, &src).is_empty());
    assert!(ext.catalog("mappers").is_empty());
}
