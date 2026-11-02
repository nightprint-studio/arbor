//! Minimal Java repros for the inference defects the miss trace found: each is written as a tiny project, opened in the index, and the
//! expression the popup would need is typed. A repro that types is not a defect; the ones listed in `docs/bennu-ml-bench.md` do not.
//!
//! `cargo test -p bennu-be mlbench::repro -- --ignored --nocapture`

use std::sync::Arc;

use super::common::{env, Quiet};
use crate::index_service::IndexService;

struct Case {
    name: &'static str,
    /// The file; the expression to type is the one between `«` and `»`.
    java: &'static str,
}

const CASES: &[Case] = &[
    Case { name: "getClass() on an array", java: "package r;\npublic class A {\n    void f() {\n        boolean[] a = new boolean[1];\n        Object o = «a.getClass()».toString();\n    }\n}\n" },
    Case { name: "lambda parameter of a project generic", java: "package r;\nimport java.util.*;\npublic class A {\n    static class Item { String name() { return \"\"; } }\n    void f(List<Item> items) {\n        items.forEach(item -> System.out.println(«item».name()));\n    }\n}\n" },
    Case { name: "lambda parameter of a Map.forEach", java: "package r;\nimport java.util.*;\npublic class A {\n    static class Item { String name() { return \"\"; } }\n    void f(Map<String, Item> m) {\n        m.forEach((k, v) -> System.out.println(«v».name()));\n    }\n}\n" },
    Case { name: "super as a receiver", java: "package r;\nimport java.util.*;\npublic class A extends ArrayList<String> {\n    @Override public String get(int i) {\n        return «super».get(i).trim();\n    }\n}\n" },
    Case { name: "call on super", java: "package r;\nimport java.util.*;\npublic class A extends ArrayList<String> {\n    @Override public String get(int i) {\n        return «super.get(i)».trim();\n    }\n}\n" },
    Case { name: "two overloads of the same arity, one picked by the argument type", java: "package r;\nimport java.util.function.Function;\npublic class A {\n    static class Spec { Spec header(String h) { return this; } }\n    static class Client {\n        Spec uri(String path, Object... vars) { return new Spec(); }\n        Spec uri(String path, Function<String, String> f) { return new Spec(); }\n    }\n    void f(Client c) {\n        Object o = «c.uri(\"/p\", \"x\")».header(\"h\");\n    }\n}\n" },
    Case { name: "generic method return bound by the argument", java: "package r;\nimport java.util.*;\npublic class A {\n    static <T> T first(List<T> l) { return l.get(0); }\n    void f(List<String> names) {\n        Object o = «first(names)».trim();\n    }\n}\n" },
    Case { name: "identity generic method, T from the argument", java: "package r;\npublic class A {\n    static <T> T id(T x) { return x; }\n    void f(String s) {\n        Object o = «id(s)».trim();\n    }\n}\n" },
    Case { name: "generic method, T from a List<T> argument (project method)", java: "package r;\nimport java.util.*;\npublic class A {\n    static <T> T first(List<T> l) { return null; }\n    void f(List<String> names) {\n        Object o = «first(names)».trim();\n    }\n}\n" },
    Case { name: "JDK generic static: Collections.max", java: "package r;\nimport java.util.*;\npublic class A {\n    void f(List<String> names) {\n        Object o = «Collections.max(names)».trim();\n    }\n}\n" },
    Case { name: "JDK generic static: Optional.of(..).get()", java: "package r;\nimport java.util.*;\npublic class A {\n    void f(String s) {\n        Object o = «Optional.of(s).get()».trim();\n    }\n}\n" },
    Case { name: "JDK generic static: Arrays.asList(..).get(0)", java: "package r;\nimport java.util.*;\npublic class A {\n    void f() {\n        Object o = «Arrays.asList(\"a\", \"b\").get(0)».trim();\n    }\n}\n" },
    Case { name: "requireNonNull with a message", java: "package r;\nimport java.util.Objects;\npublic class A {\n    void f(String s) {\n        Object o = «Objects.requireNonNull(s, \"msg\")».trim();\n    }\n}\n" },
    Case { name: "requireNonNull without a message", java: "package r;\nimport java.util.Objects;\npublic class A {\n    void f(String s) {\n        Object o = «Objects.requireNonNull(s)».trim();\n    }\n}\n" },
    Case { name: "array of a class: array.getClass().getComponentType()", java: "package r;\npublic class A {\n    void f(String[] a) {\n        Object o = «a.getClass()».getComponentType();\n    }\n}\n" },
    Case { name: "getClass() on an object", java: "package r;\npublic class A {\n    void f(String s) {\n        Object o = «s.getClass()».getName();\n    }\n}\n" },
    Case { name: "builder chain with a generic self type", java: "package r;\npublic class A {\n    static abstract class B<T extends B<T>> { T self() { return null; } T name(String n) { return self(); } }\n    static class C extends B<C> { C extra() { return this; } }\n    void f(C c) {\n        Object o = «c.name(\"n\")».extra();\n    }\n}\n" },
    Case { name: "Objects.requireNonNull then a member", java: "package r;\nimport java.util.Objects;\npublic class A {\n    void f(String s) {\n        Object o = «Objects.requireNonNull(s, \"s\")».trim();\n    }\n}\n" },
    Case { name: "ternary receiver", java: "package r;\npublic class A {\n    void f(String s, boolean b) {\n        Object o = «(b ? s : \"x\")».trim();\n    }\n}\n" },
    Case { name: "cast receiver", java: "package r;\npublic class A {\n    void f(Object s) {\n        Object o = «((String) s)».trim();\n    }\n}\n" },
    Case { name: "inherited field from a generic superclass", java: "package r;\nimport java.util.*;\npublic class A extends Base<String> {\n    void f() {\n        Object o = «items».get(0).trim();\n    }\n}\nclass Base<T> { protected List<T> items = new ArrayList<>(); }\n" },
];

#[test]
#[ignore = "builds and opens tiny projects: see the module docs"]
fn the_repros_of_the_inference_defects() {
    let svc = IndexService::global();
    let dir = super::common::work_dir().join("repro");
    let mut report = String::from("| repro | types as | verdict |\n|---|---|---|\n");
    for (i, c) in CASES.iter().enumerate() {
        let root = dir.join(format!("p{i}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("r")).unwrap();
        let (a, b) = (c.java.find('«').unwrap(), c.java.find('»').unwrap());
        let src = format!("{}{}{}", &c.java[..a], &c.java[a + '«'.len_utf8()..b], &c.java[b + '»'.len_utf8()..]);
        let file = root.join("r/A.java");
        std::fs::write(&file, &src).unwrap();
        let root_s = root.to_string_lossy().into_owned();
        svc.open(&root_s, &env("BENNU_QUALITY_JDK").unwrap_or_else(|| "21".into()), "UTF-8", Arc::new(Quiet));
        let started = std::time::Instant::now();
        while !svc.has_resolver(&root_s) || !svc.has_semantic_engine(&root_s) {
            assert!(started.elapsed().as_secs() < 120, "index of {} did not come up", c.name);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let (start, end) = (a, b - '«'.len_utf8());
        let got = svc.expression_type(&file.to_string_lossy(), &src, start, end);
        let verdict = if got.is_some() { "types" } else { "DOES NOT TYPE" };
        report.push_str(&format!("| {} | {} | {verdict} |\n", c.name, got.as_deref().unwrap_or("—")));
        svc.forget(&root_s);
    }
    super::common::write_result("repro.md", &report);
    eprintln!("{report}");
}
