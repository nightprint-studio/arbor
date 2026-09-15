//! The line between what needs the full classpath and what does not.
//!
//! The project's dependency jars are resolved on their own thread, because that is the step that
//! shells out to Maven and can take minutes. The semantic engine goes live before it lands, which
//! is the whole point: **navigation never needed a dependency jar.** Go-to-declaration, hover and
//! find-usages are answered by the walk resolver, and they used to queue behind a child process for
//! no reason at all — the symptom being go-to silently doing nothing while the class list, filled in
//! an earlier phase, kept answering.
//!
//! Rename and safe delete are the two that do need it, and for one specific question: *does this
//! member override something declared in a jar?* A partial classpath answers that with a confident
//! **no**, which is exactly the answer that lets a rename go ahead and break the build — measured at
//! twenty broken builds out of 134 on commons-collections. So they refuse until the full view
//! arrives rather than take the cheap one.
//!
//! These tests pin both halves of the line, and that the window closes.

mod common;

use common::{at, Project};

const SRC: &str = r#"
package p;
public class Greeter {
    private String name;
    String greet() {
        return "hi " + name;
    }
    String twice() {
        return greet() + greet();
    }
}
"#;

fn project() -> Project {
    Project::with_provisional_policy(&[("p/Greeter.java", SRC)])
}

/// Reading answers with no full classpath: it never consults one.
#[test]
fn go_to_declaration_does_not_wait_for_the_dependency_tier() {
    let p = project();
    let src = p.source("p/Greeter.java").to_string();
    let target = p.goto("p/Greeter.java", at(&src, "greet() + greet()"));
    assert!(target.is_some(), "go-to must answer while the dependency tier is still resolving");
}

/// So does find-usages — same resolver, same reason.
#[test]
fn find_usages_does_not_wait_for_the_dependency_tier() {
    let p = project();
    let src = p.source("p/Greeter.java").to_string();
    assert_eq!(p.usage_count("p/Greeter.java", at(&src, "greet()")), 2);
}

/// Writing does not. A rename planned against half a classpath is the trap this refusal exists for:
/// it plans clean and stops compiling, and nothing at the call site says why.
#[test]
fn rename_refuses_until_the_classpath_is_complete() {
    let p = project();
    let src = p.source("p/Greeter.java").to_string();
    assert!(
        p.rename("p/Greeter.java", at(&src, "greet()"), "welcome").is_none(),
        "a provisional policy must refuse, not answer from the cheap view"
    );
}

/// And the window closes: the dependency thread hands over the full classpath and the same call
/// answers. A refusal that never lifted would be a worse bug than the one it prevents.
#[test]
fn rename_answers_once_the_dependency_tier_lands() {
    let p = project();
    let src = p.source("p/Greeter.java").to_string();
    p.grant_full_policy();
    let plan = p.rename("p/Greeter.java", at(&src, "greet()"), "welcome");
    assert!(plan.is_some(), "the full policy is in — rename must answer");
}
