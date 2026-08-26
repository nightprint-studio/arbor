//! `Outer.this.member(…)` is how an inner class reaches its enclosing instance. It is spelled as a
//! field access whose "field" is the keyword `this` — there is no such field — so it typed to
//! nothing and every member reached that way was invisible to the index. Apache Commons writes it
//! throughout its inner collection views.

mod common;
use common::{at, Project};

const SRC: &str = r#"package p;
import java.util.Set;
public class Outer {
    private void removeAllExpired(final long now) { }
    private long now() { return 0L; }

    private final class View {
        public boolean contains(final Object o) {
            Outer.this.removeAllExpired(Outer.this.now());
            return false;
        }
    }
}
"#;

#[test]
fn a_call_through_a_qualified_this_is_renamed() {
    let p = Project::new(&[("p/Outer.java", SRC)]);
    let src = p.source("p/Outer.java");
    let decl = at(src, "private void removeAllExpired") + "private void ".len();
    let edits = p.rename_edits("p/Outer.java", decl, "remove_all_expired");
    let call = at(src, "Outer.this.removeAllExpired") + "Outer.this.".len();
    assert!(
        edits.iter().any(|e| e.start == call),
        "the call through `Outer.this` was not renamed; edits at {:?}",
        edits.iter().map(|e| e.start).collect::<Vec<_>>()
    );
}

#[test]
fn the_qualified_this_receiver_types_the_argument_call_too() {
    let p = Project::new(&[("p/Outer.java", SRC)]);
    let src = p.source("p/Outer.java");
    let decl = at(src, "private long now()") + "private long ".len();
    let edits = p.rename_edits("p/Outer.java", decl, "now_millis");
    let call = at(src, "Outer.this.now()") + "Outer.this.".len();
    assert!(
        edits.iter().any(|e| e.start == call),
        "edits at {:?}",
        edits.iter().map(|e| e.start).collect::<Vec<_>>()
    );
}
