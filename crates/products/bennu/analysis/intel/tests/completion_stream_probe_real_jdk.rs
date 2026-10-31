//! **Stream pipelines** through the provider and the real JDK: what is the type at each dot of a chain, and inside each
//! lambda? Every case names the member a correct answer must offer; the table printed at the end shows which ones fail.
//!
//! The harness tests (`autocomplete_optional_chain.rs`) pin each query. What only the provider can
//! show is how they are put together: whether a caret after a dot falls through to the bare-word
//! answer, and whether the type a function slot receives heads the type names the index offers.
//! Skipped, loudly, when no JDK 21 resolves.

use std::path::PathBuf;

use bennu_intel::prelude::{
    build_project_index_from_sources, CompletionItem, CompletionOptions, NativeJavaProvider, Position,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "bennu-stream-probe-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("temp dir");
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const USER: &str = "com/acme/Use.java";

fn provider_at(marked_body: &str) -> Option<(NativeJavaProvider, String, usize, TempDir)> {
    let marked = format!(
        "package com.acme;\n\nimport java.util.*;\nimport java.util.stream.*;\n\npublic class Use {{\n    void run(List<Person> people, Map<String, List<Person>> by_name) {{\n        {marked_body}\n    }}\n}}\n"
    );
    let caret = marked.find('|').expect("a caret marker");
    let source = marked.replacen('|', "", 1);
    let temp = TempDir::new();
    let files = vec![
        (
            PathBuf::from("com/acme/Person.java"),
            "package com.acme;\nimport java.util.List;\npublic class Person {\n    public String getName() { return null; }\n    public int getAge() { return 0; }\n    public List<String> getTags() { return null; }\n    public Address getAddress() { return null; }\n}\n".to_string(),
        ),
        (PathBuf::from("com/acme/Address.java"), "package com.acme;\npublic class Address {\n    public String getCity() { return null; }\n}\n".to_string()),
        (PathBuf::from(USER), source.clone()),
    ];
    let built = build_project_index_from_sources(&files, &temp.0);
    built.builder.persist().expect("persist");
    let names: Vec<(String, String)> = built.type_map.iter().map(|(s, b)| (s.clone(), b.clone())).collect();
    match NativeJavaProvider::for_project(&temp.0, "21", &names, None, None) {
        Ok(provider) => Some((provider, source, caret, temp)),
        Err(why) => {
            eprintln!("SKIPPED: no JDK 21 on this machine ({why})");
            None
        }
    }
}

fn labels(marked_body: &str) -> Option<Vec<String>> {
    let (provider, source, caret, _t) = provider_at(marked_body)?;
    let items = provider
        .complete_at(&Position { file: USER.to_string(), offset: caret }, Some(&source), CompletionOptions::default())
        .expect("completion");
    Some(items.into_iter().map(|i: CompletionItem| i.label).collect())
}

/// (the chain with the caret, a member a right answer must contain)
const CASES: &[(&str, &str)] = &[
    ("people.stream().filter(p -> p.|)", "getName"),
    ("people.stream().map(p -> p.getName()).filter(s -> s.|)", "isEmpty"),
    ("people.stream().map(Person::getAge).|", "mapToInt"),
    ("people.stream().map(Person::getName).collect(Collectors.toList()).|", "size"),
    ("people.stream().collect(Collectors.toList()).|", "size"),
    ("people.stream().toList().get(0).|", "getName"),
    ("people.stream().flatMap(p -> p.getTags().stream()).|", "filter"),
    ("people.stream().flatMap(p -> p.getTags().stream()).filter(t -> t.|)", "isEmpty"),
    ("people.stream().map(p -> p.getAddress()).map(a -> a.|)", "getCity"),
    ("people.stream().mapToInt(p -> p.getAge()).|", "sum"),
    ("people.stream().sorted(Comparator.comparing(p -> p.|))", "getName"),
    ("people.stream().collect(Collectors.groupingBy(Person::getName)).|", "entrySet"),
    ("by_name.entrySet().stream().map(e -> e.getValue().|)", "size"),
    ("by_name.values().stream().flatMap(List::stream).map(p -> p.|)", "getName"),
    ("people.stream().map(Person::getAddress).map(Address::getCity).|", "filter"),
    ("people.stream().filter(p -> p.getAge() > 3).findFirst().|", "orElseThrow"),
    ("people.stream().filter(p -> p.getAge() > 3).findFirst().map(p -> p.|)", "getName"),
    ("people.stream().collect(Collectors.toMap(Person::getName, p -> p.|))", "getAge"),
    ("people.stream().collect(Collectors.groupingBy(p -> p.|))", "getAddress"),
    ("people.stream().sorted(Comparator.comparing(Person::getName).thenComparing(p -> p.|))", "getAge"),
    ("Comparator<Person> by_age = Comparator.comparing(p -> p.|);", "getAge"),
];

#[test]
fn every_stage_of_a_pipeline_is_typed() {
    let mut failed = Vec::new();
    for (chain, want) in CASES {
        let Some(got) = labels(chain) else { return };
        let ok = got.iter().any(|l| l == want);
        eprintln!("{} {chain}  -> wants {want}{}", if ok { "ok  " } else { "FAIL" }, if ok { String::new() } else { format!(" (got {} rows, first: {:?})", got.len(), got.iter().take(5).collect::<Vec<_>>()) });
        if !ok {
            failed.push(*chain);
        }
    }
    assert!(failed.is_empty(), "{} of {} stages not typed: {failed:#?}", failed.len(), CASES.len());
}
