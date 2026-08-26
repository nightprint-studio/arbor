//! Three ways a rename left a project that no longer compiles, each reduced to the shape it had on
//! a real tree (a bulk naming fix over ~750 files, 262 javac errors).
//!
//! * a **record component** is a field AND an accessor of the same name — the call sites are method
//!   calls, and nothing else in the plan moves them;
//! * a method reference qualified by a **nested type** (`Outer.Nested::run`) resolves its qualifier
//!   through a different path than `obj::run`, and that path did not understand `Outer.Nested`;
//! * an **anonymous class** implementing an interface declares the override in its own body, which
//!   is a declaration the rename has to move with the interface's.

mod common;
use common::{at, Project};

// ── record components ───────────────────────────────────────────────────────────

const UPLOAD_INFO: &str = r#"package p;
public record UploadInfo(String file_name, String file_type) { }
"#;

const UPLOAD_USER: &str = r#"package p;
public class Checker {
    String describe(UploadInfo confirm) {
        return confirm.file_name() + "." + confirm.file_type();
    }
}
"#;

#[test]
fn renaming_a_record_component_renames_its_accessor_call_sites() {
    let p = Project::new(&[
        ("p/UploadInfo.java", UPLOAD_INFO),
        ("p/Checker.java", UPLOAD_USER),
    ]);
    let decl = p.source("p/UploadInfo.java");
    let edits = p.rename_edits("p/UploadInfo.java", at(decl, "file_name"), "fileName");
    let user = p.source("p/Checker.java");
    let call = at(user, "file_name()");
    assert!(
        edits
            .iter()
            .any(|e| e.file == "p/Checker.java" && e.start == call),
        "the accessor call site was not renamed; edits: {:?}",
        edits.iter().map(|e| (&e.file, e.start)).collect::<Vec<_>>()
    );
}

// ── method references qualified by a nested type ────────────────────────────────

const OUTER: &str = r#"package p;
public class Outer {
    public interface Customizer {
        boolean validazione_standard();
    }
}
"#;

const REF_USER: &str = r#"package p;
import java.util.Optional;
public class Caller {
    boolean check(Optional<Outer.Customizer> c) {
        return c.map(Outer.Customizer::validazione_standard).orElse(true);
    }
}
"#;

#[test]
fn a_method_reference_qualified_by_a_nested_type_is_renamed() {
    let p = Project::new(&[("p/Outer.java", OUTER), ("p/Caller.java", REF_USER)]);
    let decl = p.source("p/Outer.java");
    let edits = p.rename_edits(
        "p/Outer.java",
        at(decl, "validazione_standard"),
        "validazioneStandard",
    );
    let caller = p.source("p/Caller.java");
    let site = at(caller, "Outer.Customizer::validazione_standard") + "Outer.Customizer::".len();
    assert!(
        edits
            .iter()
            .any(|e| e.file == "p/Caller.java" && e.start == site),
        "the `Outer.Nested::method` reference was not renamed; edits: {:?}",
        edits.iter().map(|e| (&e.file, e.start)).collect::<Vec<_>>()
    );
}

// ── anonymous-class overrides ───────────────────────────────────────────────────

const CHECKER_IFACE: &str = r#"package p;
public interface PermissionChecker {
    String create_attachment(String bucket);

    static PermissionChecker of(String fixed) {
        return new PermissionChecker() {
            @Override
            public String create_attachment(String bucket) {
                return fixed + bucket;
            }
        };
    }
}
"#;

#[test]
fn an_anonymous_class_override_is_renamed_with_the_interface_method() {
    let p = Project::new(&[("p/PermissionChecker.java", CHECKER_IFACE)]);
    let src = p.source("p/PermissionChecker.java");
    let edits = p.rename_edits(
        "p/PermissionChecker.java",
        at(src, "create_attachment"),
        "createAttachment",
    );
    // The override in the anonymous body is the SECOND occurrence in the file.
    let anon = src.rfind("create_attachment").expect("override present");
    assert!(
        edits.iter().any(|e| e.start == anon),
        "the anonymous-class override was not renamed; edits at {:?}",
        edits.iter().map(|e| e.start).collect::<Vec<_>>()
    );
}

/// The real shape the miss had: the same method name is declared by the outer interface AND by a
/// nested one, and the anonymous class implements the OUTER. Renaming the outer's method has to
/// move the anonymous override — and only that one.
const NESTED_TWINS: &str = r#"package p;
public interface Upload {
    String create_attachment(String bucket);

    static Upload build(AttachmentCreator creator) {
        return new Upload() {
            @Override
            public String create_attachment(final String bucket) {
                return creator.create_attachment(bucket);
            }
        };
    }

    interface AttachmentCreator {
        String create_attachment(final String bucket);
    }
}
"#;

#[test]
fn an_anonymous_override_moves_when_a_twin_name_exists_on_a_nested_interface() {
    let p = Project::new(&[("p/Upload.java", NESTED_TWINS)]);
    let src = p.source("p/Upload.java");
    let edits = p.rename_edits(
        "p/Upload.java",
        at(src, "create_attachment"),
        "createAttachment",
    );
    let anon = at(src, "public String create_attachment") + "public String ".len();
    assert!(
        edits.iter().any(|e| e.start == anon),
        "the anonymous override was not renamed; edits at {:?}",
        edits.iter().map(|e| e.start).collect::<Vec<_>>()
    );
}

/// The record case as it really appeared: the receiver is a **lambda parameter**, typed only by the
/// functional interface the lambda is passed as. Without that inference the accessor calls are
/// invisible to the index, so a rename of the component leaves every one of them behind.
const REC2: &str = r#"package p;
public record UploadInfo(String file_name) { }
"#;

const IFACE2: &str = r#"package p;
public interface Creator {
    String create(UploadInfo confirm);
}
"#;

const LAMBDA_USER: &str = r#"package p;
public class Wiring {
    Creator make() {
        return confirm -> confirm.file_name();
    }
}
"#;

#[test]
fn a_record_accessor_called_on_a_lambda_parameter_is_renamed() {
    let p = Project::new(&[
        ("p/UploadInfo.java", REC2),
        ("p/Creator.java", IFACE2),
        ("p/Wiring.java", LAMBDA_USER),
    ]);
    let decl = p.source("p/UploadInfo.java");
    let edits = p.rename_edits("p/UploadInfo.java", at(decl, "file_name"), "fileName");
    let user = p.source("p/Wiring.java");
    let call = at(user, "confirm.file_name()") + "confirm.".len();
    assert!(
        edits
            .iter()
            .any(|e| e.file == "p/Wiring.java" && e.start == call),
        "the accessor call on a lambda parameter was not renamed; edits: {:?}",
        edits.iter().map(|e| (&e.file, e.start)).collect::<Vec<_>>()
    );
}

/// The exact shape from the project: the lambda is an argument to a **static factory on an
/// interface**, so the call's receiver is a type name rather than an expression.
const FACTORY: &str = r#"package p;
public interface Checker {
    String create(UploadInfo confirm);

    static Checker build(String label, Creator creator) {
        return confirm -> label + creator.create(confirm);
    }
}
"#;

const FACTORY_USER: &str = r#"package p;
public class Wiring2 {
    Checker wire() {
        return Checker.build("x", confirm -> confirm.file_name());
    }
}
"#;

#[test]
fn a_lambda_passed_to_a_static_factory_types_its_parameter() {
    let p = Project::new(&[
        ("p/UploadInfo.java", REC2),
        ("p/Creator.java", IFACE2),
        ("p/Checker.java", FACTORY),
        ("p/Wiring2.java", FACTORY_USER),
    ]);
    let decl = p.source("p/UploadInfo.java");
    let edits = p.rename_edits("p/UploadInfo.java", at(decl, "file_name"), "fileName");
    let user = p.source("p/Wiring2.java");
    let call = at(user, "confirm.file_name()") + "confirm.".len();
    assert!(
        edits
            .iter()
            .any(|e| e.file == "p/Wiring2.java" && e.start == call),
        "the accessor call inside a lambda passed to a static factory was not renamed; edits: {:?}",
        edits.iter().map(|e| (&e.file, e.start)).collect::<Vec<_>>()
    );
}

// ── method references qualified by `this` and by a parameter ────────────────────

const THIS_REF: &str = r#"package p;
import java.util.List;
public class Service {
    void popola_aggiudicatarie(String row) { }

    void run(List<String> rows) {
        rows.forEach(this::popola_aggiudicatarie);
    }
}
"#;

#[test]
fn a_this_qualified_method_reference_is_renamed() {
    let p = Project::new(&[("p/Service.java", THIS_REF)]);
    let src = p.source("p/Service.java");
    let edits = p.rename_edits(
        "p/Service.java",
        at(src, "popola_aggiudicatarie"),
        "popolaAggiudicatarie",
    );
    let site = at(src, "this::popola_aggiudicatarie") + "this::".len();
    assert!(
        edits.iter().any(|e| e.start == site),
        "the `this::method` reference was not renamed; edits at {:?}",
        edits.iter().map(|e| e.start).collect::<Vec<_>>()
    );
}

const PARAM_REF_TARGET: &str = r#"package p;
public class Docs {
    public String documento_consensi(String id) { return id; }
}
"#;

const PARAM_REF_USER: &str = r#"package p;
import java.util.function.Function;
public class Wiring3 {
    Function<String, String> wire(final Docs service) {
        return service::documento_consensi;
    }
}
"#;

#[test]
fn a_method_reference_qualified_by_a_parameter_is_renamed() {
    let p = Project::new(&[
        ("p/Docs.java", PARAM_REF_TARGET),
        ("p/Wiring3.java", PARAM_REF_USER),
    ]);
    let decl = p.source("p/Docs.java");
    let edits = p.rename_edits(
        "p/Docs.java",
        at(decl, "documento_consensi"),
        "documentoConsensi",
    );
    let user = p.source("p/Wiring3.java");
    let site = at(user, "service::documento_consensi") + "service::".len();
    assert!(
        edits
            .iter()
            .any(|e| e.file == "p/Wiring3.java" && e.start == site),
        "the `param::method` reference was not renamed; edits: {:?}",
        edits.iter().map(|e| (&e.file, e.start)).collect::<Vec<_>>()
    );
}

// ── a fluent accessor reached through an enum constant ──────────────────────────

const HEADERS: &str = r#"package p;
import lombok.Getter;
import lombok.RequiredArgsConstructor;
import lombok.experimental.Accessors;

@Getter
@Accessors(fluent = true)
@RequiredArgsConstructor
public enum Headers {
    USERNAME("x-user"),
    TRACE_ID("x-trace");

    private final String header_name;
}
"#;

const HEADERS_USER: &str = r#"package p;
public class Uses {
    String user() {
        return Headers.USERNAME.header_name();
    }
}
"#;

#[test]
fn a_fluent_accessor_read_through_an_enum_constant_is_renamed() {
    let p = Project::new(&[("p/Headers.java", HEADERS), ("p/Uses.java", HEADERS_USER)]);
    let decl = p.source("p/Headers.java");
    let name_at = at(decl, "String header_name") + "String ".len();
    let edits = p.rename_edits("p/Headers.java", name_at, "headerName");
    let user = p.source("p/Uses.java");
    let call = at(user, "USERNAME.header_name()") + "USERNAME.".len();
    assert!(
        edits
            .iter()
            .any(|e| e.file == "p/Uses.java" && e.start == call),
        "the accessor call through an enum constant was not renamed; edits: {:?}",
        edits.iter().map(|e| (&e.file, e.start)).collect::<Vec<_>>()
    );
}

/// The anonymous class implements a SUB-interface of the one that declares the method — the shape
/// a `Public` / `Protected` split has. Matching the anonymous body against the interface named in
/// the `new` is not enough: the declaration being renamed is one level up.
const SUBIFACE: &str = r#"package p;
public interface Download {
    String document_info(String username, String identifier);

    static Download publicOne(String fixed) {
        return new Public() {
            @Override
            public String document_info(String username, String identifier) {
                return fixed;
            }
        };
    }

    interface Public extends Download { }
}
"#;

#[test]
fn an_anonymous_override_of_a_subinterface_moves_with_the_declaration() {
    let p = Project::new(&[("p/Download.java", SUBIFACE)]);
    let src = p.source("p/Download.java");
    let edits = p.rename_edits("p/Download.java", at(src, "document_info"), "documentInfo");
    let anon = at(src, "public String document_info") + "public String ".len();
    assert!(
        edits.iter().any(|e| e.start == anon),
        "the anonymous override of a sub-interface was not renamed; edits at {:?}",
        edits.iter().map(|e| e.start).collect::<Vec<_>>()
    );
}
