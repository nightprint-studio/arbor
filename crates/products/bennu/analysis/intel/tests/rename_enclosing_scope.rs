//! A nested class sees the members of every class it is written inside (JLS §8.1.3), and writes
//! them unqualified. Filing those uses under the INNER class — a key no rename looks up — left the
//! outer declaration renamed and every use inside a nested class spelling the old name.

mod common;
use common::{at, Project};

const MAPPER: &str = r#"package p;
public class Mapper {
    private static final String z_offset = "Z";
    private static String convert_value(String raw) { return raw; }

    public static class Deserializer {
        String read(String found) {
            return convert_value(found) + z_offset;
        }
    }
}
"#;

#[test]
fn a_field_of_the_outer_class_is_renamed_where_a_nested_class_reads_it() {
    let p = Project::new(&[("p/Mapper.java", MAPPER)]);
    let src = p.source("p/Mapper.java");
    let edits = p.rename_edits("p/Mapper.java", at(src, "z_offset"), "Z_OFFSET");
    let use_site = at(src, "+ z_offset") + "+ ".len();
    assert!(
        edits.iter().any(|e| e.start == use_site),
        "the nested class's use of the outer field was not renamed; edits at {:?}",
        edits.iter().map(|e| e.start).collect::<Vec<_>>()
    );
}

#[test]
fn a_method_of_the_outer_class_is_renamed_where_a_nested_class_calls_it() {
    let p = Project::new(&[("p/Mapper.java", MAPPER)]);
    let src = p.source("p/Mapper.java");
    let edits = p.rename_edits("p/Mapper.java", at(src, "convert_value"), "convertValue");
    let call = at(src, "return convert_value(found)") + "return ".len();
    assert!(
        edits.iter().any(|e| e.start == call),
        "the nested class's call to the outer method was not renamed; edits at {:?}",
        edits.iter().map(|e| e.start).collect::<Vec<_>>()
    );
}

/// Two levels deep, and through an anonymous class — the same rule, and the shape a Jackson
/// serializer actually has.
const DEEP: &str = r#"package p;
public class Outer {
    private static final int shared_limit = 3;

    static class Mid {
        Runnable make() {
            return new Runnable() {
                public void run() {
                    int x = shared_limit;
                }
            };
        }
    }
}
"#;

#[test]
fn the_climb_reaches_through_an_anonymous_class_and_two_levels_of_nesting() {
    let p = Project::new(&[("p/Outer.java", DEEP)]);
    let src = p.source("p/Outer.java");
    let edits = p.rename_edits("p/Outer.java", at(src, "shared_limit"), "SHARED_LIMIT");
    let use_site = at(src, "= shared_limit;") + "= ".len();
    assert!(
        edits.iter().any(|e| e.start == use_site),
        "the deep use was not renamed; edits at {:?}",
        edits.iter().map(|e| e.start).collect::<Vec<_>>()
    );
}
