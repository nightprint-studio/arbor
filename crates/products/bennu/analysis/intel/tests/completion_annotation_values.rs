//! Inside an annotation's parentheses the popup offers what the project already writes there, and inside
//! `@Value("${…")` the keys of its configuration files. The tables are filled by a background scan, so the test
//! waits for it the way a second keystroke would. Needs a JDK 21 (skipped without one).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bennu_index::prelude::PersistedIndex;
use bennu_intel::prelude::{build_project_index_from_sources, CompletionOptions, NativeJavaProvider, Position};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!("bennu-annvals-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(p.join("src")).expect("temp dir");
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn controller(name: &str, mapping: &str) -> String {
    format!("package p;\nclass {name} {{\n    {mapping}\n    void go() {{ }}\n}}\n")
}

/// The completion rows at the `|` of `marked`, written as `src/Use.java` into a project that already has other controllers.
fn offered(marked: &str) -> Option<Vec<bennu_intel::prelude::CompletionItem>> {
    let temp = TempDir::new();
    let root = &temp.0;
    std::fs::write(root.join("pom.xml"), "<project/>").unwrap();
    std::fs::write(root.join("src/application.yml"), "spring:\n  datasource:\n    url: jdbc:x\n    username: u\napp:\n  limit: 3\n").unwrap();
    for (n, m) in [
        ("A", "@PostMapping(value = \"/orders\", produces = MediaType.APPLICATION_JSON_VALUE)"),
        ("B", "@PostMapping(value = \"/orders\", produces = MediaType.APPLICATION_JSON_VALUE)"),
        ("C", "@PostMapping(value = \"/items\", produces = MediaType.APPLICATION_XML_VALUE)"),
    ] {
        std::fs::write(root.join(format!("src/{n}.java")), controller(n, m)).unwrap();
    }
    let offset = marked.find('|').expect("a caret");
    let source = marked.replace('|', "");
    let file = root.join("src/Use.java");
    std::fs::write(&file, &source).unwrap();

    let files = vec![(PathBuf::from("Use.java"), source.clone())];
    let built = build_project_index_from_sources(&files, root);
    built.builder.persist().expect("persist");
    PersistedIndex::open(built.builder.blob_path(), built.builder.fst_path()).expect("open");
    let names: Vec<(String, String)> = built.type_map.iter().map(|(s, b)| (s.clone(), b.clone())).collect();
    let provider = match NativeJavaProvider::for_project(root, "21", &names, None, None) {
        Ok(p) => p,
        Err(why) => {
            eprintln!("SKIPPED: no JDK 21 on this machine ({why})");
            return None;
        }
    };
    let ask = || {
        provider
            .complete_at(&Position { file: file.to_string_lossy().into_owned(), offset }, Some(&source), CompletionOptions::default())
            .expect("completion")
    };
    // the first request starts the scan and finds the tables empty; wait for the scan like the next keystroke would
    let _ = ask();
    let deadline = Instant::now() + Duration::from_secs(10);
    // …and until two reads in a row agree: the scan fills the tables file by file
    let mut items = ask();
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let again = ask();
        let settled = !again.is_empty() && again == items;
        items = again;
        if settled || Instant::now() > deadline {
            break;
        }
    }
    Some(items)
}

fn labels(items: &[bennu_intel::prelude::CompletionItem]) -> Vec<&str> {
    items.iter().map(|i| i.label.as_str()).collect()
}

#[test]
fn a_constant_the_project_uses_leads_the_value_of_an_element() {
    let Some(items) = offered("package p;\nimport org.springframework.http.MediaType;\nclass Use {\n    @PostMapping(produces = |)\n    void go() { }\n}\n") else { return };
    let l = labels(&items);
    assert_eq!(l.first(), Some(&"MediaType.APPLICATION_JSON_VALUE"), "{l:?}");
    assert!(l.contains(&"MediaType.APPLICATION_XML_VALUE"), "{l:?}");
    assert!(items[0].detail.as_deref().unwrap_or("").contains("2 places"), "{:?}", items[0].detail);
}

#[test]
fn a_string_the_project_uses_is_offered_inside_the_quotes_without_them() {
    let Some(items) = offered("package p;\nclass Use {\n    @PostMapping(value = \"/or|\")\n    void go() { }\n}\n") else { return };
    let item = items.iter().find(|i| i.label == "\"/orders\"").unwrap_or_else(|| panic!("{:?}", labels(&items)));
    assert_eq!(item.insert_text.as_deref(), Some("/orders"));
    assert!(item.replace_start.is_some() && item.replace_end.is_some());
    assert!(!items.iter().any(|i| i.label.contains("/items")), "only what starts with the typed text");
}

#[test]
fn the_keys_of_the_configuration_complete_inside_value_placeholders() {
    let Some(items) = offered("package p;\nclass Use {\n    @Value(\"${spring.da|}\")\n    String url;\n}\n") else { return };
    let l = labels(&items);
    assert_eq!(l, ["spring.datasource.url", "spring.datasource.username"], "{l:?}");
    let first = &items[0];
    assert_eq!(first.insert_text.as_deref(), Some("spring.datasource.url"));
    let source_start = "package p;\nclass Use {\n    @Value(\"${".len();
    assert_eq!(first.replace_start, Some(source_start));
}

#[allow(dead_code)]
fn unused(_: &Path) {}
