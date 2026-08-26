//! Three shapes a whole-project rename left behind on Guava, each reduced to the smallest project
//! that shows it.

mod common;
use common::{at, Project};

/// A TYPE used to qualify a static member — `Murmur3_32HashFunction.MURMUR3_32`. tree-sitter reads
/// the qualifier as a plain `identifier` inside a `field_access`, not a `type_identifier`, so a walk
/// that indexes type uses by node kind never sees it: renaming the type moved its declaration and
/// its file and left every static access spelling the old name.
#[test]
fn a_type_qualifying_a_static_member_is_a_use_of_that_type() {
    const HOLDER: &str = r#"package p;
public final class Holder {
    static final int VALUE = 1;
}
"#;
    const USER: &str = r#"package p;
public class User {
    int read() {
        return Holder.VALUE;
    }
}
"#;
    let p = Project::new(&[("p/Holder.java", HOLDER), ("p/User.java", USER)]);
    let edits = p.rename_edits("p/Holder.java", at(HOLDER, "class Holder") + 6, "Keeper");
    assert!(
        edits.iter().any(|e| e.file == "p/User.java"),
        "the static access must be rewritten: {:?}",
        edits.iter().map(|e| (&e.file, &e.old)).collect::<Vec<_>>()
    );
}

/// A method declared in an ENUM CONSTANT's body, called from another method of the same body. The
/// body is an anonymous subclass of the enum, and both the declaration and the call live inside it.
#[test]
fn a_method_declared_in_an_enum_constant_body_is_renamed_with_its_calls() {
    const SRC: &str = r#"package p;
public enum Tester {
    SMALL {
        @Override
        long test(long n) {
            return helper(n);
        }

        private long helper(long a) {
            return a * 2;
        }
    };

    abstract long test(long n);
}
"#;
    let p = Project::new(&[("p/Tester.java", SRC)]);
    let edits = p.rename_edits(
        "p/Tester.java",
        at(SRC, "private long helper") + 13,
        "scaled",
    );
    assert!(edits.len() >= 2, "declaration AND call: {edits:?}");
}

/// Two static imports of the same member NAME from different owners. Java tells the two calls
/// apart by their shape, and so must the rename: matching an import by its trailing name alone
/// rewrote whichever came first, leaving one import naming a method its owner no longer declares
/// and the other still spelling the old name. Guava's `AbstractTable` does exactly this with
/// `Collections2.safeRemove` and `Maps.safeRemove`.
#[test]
fn a_static_import_is_matched_by_owner_not_only_by_name() {
    const A: &str = "package p;\npublic final class A {\n    static void wipe(int n) { }\n}\n";
    const B: &str =
        "package p;\npublic final class B {\n    static void wipe(String s, int n) { }\n}\n";
    const USER: &str = r#"package p;

import static p.A.wipe;
import static p.B.wipe;

public class User {
    void run() {
        wipe(1);
        wipe("x", 2);
    }
}
"#;
    let p = Project::new(&[("p/A.java", A), ("p/B.java", B), ("p/User.java", USER)]);
    let edits = p.rename_edits("p/A.java", at(A, "void wipe") + 5, "erase");
    let in_user: Vec<usize> = edits
        .iter()
        .filter(|e| e.file == "p/User.java")
        .map(|e| e.start)
        .collect();

    let a_import = at(USER, "import static p.A.wipe");
    let b_import = at(USER, "import static p.B.wipe");
    assert!(
        in_user.iter().any(|&s| s > a_import && s < a_import + 22),
        "its OWN import must be rewritten: {in_user:?}"
    );
    assert!(
        !in_user.iter().any(|&s| s > b_import && s < b_import + 22),
        "the other owner's import must be left alone: {in_user:?}"
    );
    // And the call that binds to B — told apart by its argument count, as javac tells them apart.
    let b_call = at(USER, r#"wipe("x", 2)"#);
    assert!(
        !in_user.contains(&b_call),
        "the other owner's call must be left alone: {in_user:?}"
    );
}

/// A field of an ENCLOSING class, read bare from inside an anonymous body, and then stepped
/// through: `upperBoundWindow.upperBound.isLessThan(…)` in Guava's `TreeRangeSet`, and
/// `state.closeables.applyClosingFunction(…)` in `ClosingFuture`.
///
/// A nested class sees its enclosing classes' members and writes them unqualified (JLS §8.1.3), and
/// an anonymous body is a nested class — so typing the head of the chain means climbing out of it.
/// Without that the whole chain went untyped, and the call at its end was invisible to a rename.
#[test]
fn a_field_of_an_enclosing_class_read_inside_an_anonymous_body_types_its_chain() {
    const CUT: &str = r#"package p;
public class Cut {
    public boolean isLessThan(Cut other) { return true; }
}
"#;
    const RANGE: &str = "package p;\npublic class Range {\n    public Cut upperBound;\n}\n";
    const OUTER: &str = r#"package p;
public class Outer {
    final Range window = new Range();

    Runnable make() {
        return new Runnable() {
            @Override
            public void run() {
                if (window.upperBound.isLessThan(window.upperBound)) {
                    return;
                }
            }
        };
    }
}
"#;
    let p = Project::new(&[
        ("p/Cut.java", CUT),
        ("p/Range.java", RANGE),
        ("p/Outer.java", OUTER),
    ]);
    let edits = p.rename_edits("p/Cut.java", at(CUT, "boolean isLessThan") + 8, "precedes");
    assert!(
        edits.iter().any(|e| e.file == "p/Outer.java"),
        "the call inside the anonymous body must be rewritten: {:?}",
        edits.iter().map(|e| (&e.file, &e.old)).collect::<Vec<_>>()
    );
}
