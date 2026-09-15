//! A field reached through `import static p.Holder.*;` is written bare, exactly like one of the
//! file's own. The method path already asked the static imports who owns such a name; the field
//! path did not, so those uses were filed under nobody — and renaming the field left every one of
//! them spelling a name that no longer exists.
//!
//! It showed up in test sources, where a shared fixture class is star-imported by half a dozen
//! test classes.

mod common;
use common::{at, Project};

const HOLDER: &str = r#"package p;
public class Fixture {
    public static String db_dettaglio = "x";
    public static String other = "y";
}
"#;

const STAR_USER: &str = r#"package p;
import static p.Fixture.*;

public class StarTest {
    String read() {
        return db_dettaglio;
    }
}
"#;

const NAMED_USER: &str = r#"package p;
import static p.Fixture.db_dettaglio;

public class NamedTest {
    String read() {
        return db_dettaglio;
    }
}
"#;

fn project() -> Project {
    Project::new(&[
        ("p/Fixture.java", HOLDER),
        ("p/StarTest.java", STAR_USER),
        ("p/NamedTest.java", NAMED_USER),
    ])
}

fn edits(p: &Project) -> Vec<bennu_intel::prelude::Edit> {
    let decl = p.source("p/Fixture.java");
    p.rename_edits(
        "p/Fixture.java",
        at(decl, "String db_dettaglio") + "String ".len(),
        "dbDettaglio",
    )
}

#[test]
fn a_star_imported_field_use_is_renamed() {
    let p = project();
    let e = edits(&p);
    let user = p.source("p/StarTest.java");
    let site = at(user, "return db_dettaglio;") + "return ".len();
    assert!(
        e.iter()
            .any(|x| x.file == "p/StarTest.java" && x.start == site),
        "the star-imported use was not renamed; edits: {:?}",
        e.iter().map(|x| (&x.file, x.start)).collect::<Vec<_>>()
    );
}

#[test]
fn a_named_static_import_and_its_use_are_both_renamed() {
    let p = project();
    let e = edits(&p);
    let user = p.source("p/NamedTest.java");
    let import_site =
        at(user, "import static p.Fixture.db_dettaglio;") + "import static p.Fixture.".len();
    let use_site = at(user, "return db_dettaglio;") + "return ".len();
    assert!(
        e.iter()
            .any(|x| x.file == "p/NamedTest.java" && x.start == import_site),
        "the import itself was not renamed"
    );
    assert!(
        e.iter()
            .any(|x| x.file == "p/NamedTest.java" && x.start == use_site),
        "the use was not renamed; edits: {:?}",
        e.iter().map(|x| (&x.file, x.start)).collect::<Vec<_>>()
    );
}
