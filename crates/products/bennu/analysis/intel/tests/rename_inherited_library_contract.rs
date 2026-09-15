//! A subtype can bind a method to a library interface **without redeclaring it**, and that binding
//! has to refuse the rename just as loudly as a declaration would.
//!
//! Commons Collections is the measured case: `HashIterator` declares `hasMoreElements()`, and
//! `KeyIterator extends HashIterator implements Enumeration<K>` is what makes that method an
//! implementation of `java.util.Enumeration`. `KeyIterator` declares nothing of its own, so it is
//! not in the override family — the family collects declarations — and the refusal, asked only of
//! the family, said nothing. The rename applied cleanly and two inner classes stopped implementing
//! `Enumeration`.

mod common;
use common::{at, Project};

const SRC: &str = r#"package p;
import java.util.Enumeration;
public class Holder {
    class Base {
        public boolean hasMoreElements() { return false; }
        public String nextElement() { return null; }
    }
    class Keys extends Base implements Enumeration<String> {
    }
}
"#;

#[test]
fn a_subtype_that_inherits_the_method_into_a_library_interface_blocks_the_rename() {
    let p = Project::with_stream_jdk(&[("p/Holder.java", SRC)]);
    let src = p.source("p/Holder.java");
    let plan = p
        .rename("p/Holder.java", at(src, "hasMoreElements"), "hasMore")
        .expect("a plan");
    let blocked = plan
        .blocked
        .expect("the rename must refuse: `Keys` implements Enumeration with the inherited method");
    assert!(blocked.contains("Enumeration"), "{blocked}");
}

#[test]
fn a_method_no_library_type_declares_is_still_renameable() {
    const PLAIN: &str = r#"package p;
public class Holder {
    class Base {
        public boolean ownThing() { return false; }
    }
    class Keys extends Base {
    }
}
"#;
    let p = Project::with_stream_jdk(&[("p/Holder.java", PLAIN)]);
    let src = p.source("p/Holder.java");
    let plan = p
        .rename("p/Holder.java", at(src, "ownThing"), "myThing")
        .expect("a plan");
    assert!(plan.blocked.is_none(), "{:?}", plan.blocked);
}
