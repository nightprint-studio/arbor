//! Renaming a member that callers reach through a `import static`.

mod common;
use common::{at, Project};

fn project() -> Project {
    Project::new(&[
        (
            "p/Util.java",
            "package p;\npublic class Util {\n    public static String join_all(String a) { return a; }\n    public static final int MAX_SIZE = 3;\n}\n",
        ),
        (
            "p/Caller.java",
            "package p;\nimport static p.Util.join_all;\nimport static p.Util.MAX_SIZE;\npublic class Caller {\n    String go() {\n        return join_all(\"x\") + MAX_SIZE;\n    }\n}\n",
        ),
    ])
}

#[test]
fn the_static_import_of_a_renamed_method_is_rewritten() {
    let p = project();
    let src = p.source("p/Util.java");
    let edits = p.rename_edits("p/Util.java", at(src, "join_all"), "joinAll");
    let caller = p.source("p/Caller.java");
    let import_at = at(caller, "import static p.Util.join_all") + "import static p.Util.".len();
    assert!(
        edits.iter().any(|e| e.file == "p/Caller.java" && e.start == import_at),
        "the static import was not rewritten; edits: {:?}",
        edits.iter().map(|e| (e.file.as_str(), e.start, e.reason.label())).collect::<Vec<_>>()
    );
}

#[test]
fn the_call_through_a_static_import_is_rewritten() {
    let p = project();
    let src = p.source("p/Util.java");
    let edits = p.rename_edits("p/Util.java", at(src, "join_all"), "joinAll");
    let caller = p.source("p/Caller.java");
    let call_at = at(caller, "return join_all(") + "return ".len();
    assert!(
        edits.iter().any(|e| e.file == "p/Caller.java" && e.start == call_at),
        "the bare call through the static import was not renamed"
    );
}

#[test]
fn the_static_import_of_a_renamed_field_is_rewritten() {
    let p = project();
    let src = p.source("p/Util.java");
    let edits = p.rename_edits("p/Util.java", at(src, "MAX_SIZE"), "MAX_COUNT");
    let caller = p.source("p/Caller.java");
    let import_at = at(caller, "import static p.Util.MAX_SIZE") + "import static p.Util.".len();
    assert!(
        edits.iter().any(|e| e.file == "p/Caller.java" && e.start == import_at),
        "the static import of the field was not rewritten"
    );
}
