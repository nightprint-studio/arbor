//! A rename must never quietly break the build. Where the engine cannot type a receiver it cannot
//! tell whether `x.do_it()` is a use of the method being renamed — so it refuses, by name, instead
//! of rewriting the declaration and leaving that call behind.
//!
//! This is the backstop under every inference gap: the gaps get closed one at a time, and until one
//! is, the cost of it is a refusal the user can read rather than a compile error they cannot.

mod common;
use common::{at, Project};

const OWNER: &str = r#"package p;
public class Service {
    public String do_it() { return "x"; }
}
"#;

const BLIND_CALLER: &str = r#"package p;
import com.example.unknown.Mystery;
public class Caller {
    String go(Mystery m) {
        // `Mystery` is not on any classpath the engine can read, so the type of `m.thing()` is
        // unknown — and with it, whose `do_it` this is.
        return m.thing().do_it();
    }
}
"#;

const CLEAR_CALLER: &str = r#"package p;
public class Plain {
    String go(Service s) {
        return s.do_it();
    }
}
"#;

#[test]
fn a_call_on_an_untypeable_receiver_blocks_the_rename() {
    let p = Project::new(&[("p/Service.java", OWNER), ("p/Caller.java", BLIND_CALLER)]);
    let src = p.source("p/Service.java");
    let plan = p
        .rename(&"p/Service.java", at(src, "do_it"), "doIt")
        .expect("a plan");
    let blocked = plan.blocked.expect("the plan must refuse");
    assert!(
        blocked.contains("do_it"),
        "the reason must name the method: {blocked}"
    );
    assert!(
        blocked.contains("Caller.java"),
        "the reason must say where: {blocked}"
    );
}

#[test]
fn a_project_the_engine_can_read_is_not_refused() {
    let p = Project::new(&[("p/Service.java", OWNER), ("p/Plain.java", CLEAR_CALLER)]);
    let src = p.source("p/Service.java");
    let plan = p
        .rename(&"p/Service.java", at(src, "do_it"), "doIt")
        .expect("a plan");
    assert!(
        plan.blocked.is_none(),
        "nothing here is unseen: {:?}",
        plan.blocked
    );
    let plain = p.source("p/Plain.java");
    let edits = p.rename_edits("p/Service.java", at(src, "do_it"), "doIt");
    assert!(edits
        .iter()
        .any(|e| e.file == "p/Plain.java" && e.start == at(plain, "s.do_it()") + 2));
}

/// A name the engine could not resolve somewhere ELSE must not block an unrelated rename.
#[test]
fn the_refusal_is_keyed_on_the_name_being_renamed() {
    let p = Project::new(&[("p/Service.java", OWNER), ("p/Caller.java", BLIND_CALLER)]);
    let src = p.source("p/Service.java");
    // `Service` declares nothing called `other_thing`; renaming a DIFFERENT member is unaffected.
    let plan = p.rename(&"p/Service.java", at(src, "Service"), "Svc");
    assert!(plan.map(|pl| pl.blocked.is_none()).unwrap_or(true));
}
