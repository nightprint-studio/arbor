fn main() {
    // extract method inside a static method
    let src = "class A {\n    static int f(int n) {\n        int base = n * 2;\n        int scaled = base + 10;\n        return scaled;\n    }\n}";
    let tree = bennu_java::prelude::parse_java(src).unwrap();
    let s = src.find("int base").unwrap();
    let e = src.find("base + 10;").unwrap() + "base + 10;".len();
    println!("=== extract method in a static method ===");
    match bennu_refactor::prelude::extract_method(tree.root_node(), src, s, e) {
        Some(Ok(p)) => println!("{}", p.apply(src)),
        other => println!("{other:?}"),
    }
    // create method from a static caller
    let src2 = "class A {\n    static void f() {\n        report(1);\n    }\n}";
    let t2 = bennu_java::prelude::parse_java(src2).unwrap();
    let at = src2.find("report").unwrap();
    println!("=== create method from a static caller ===");
    match bennu_refactor::prelude::create_method(t2.root_node(), src2, at, at + "report".len()) {
        Some(Ok(p)) => println!("{}", p.apply(src2)),
        other => println!("{other:?}"),
    }
    // extract constant inside a static context
    let src3 = "class A {\n    static String f() {\n        return \"literal\";\n    }\n}";
    let t3 = bennu_java::prelude::parse_java(src3).unwrap();
    let a3 = src3.find("\"literal\"").unwrap();
    println!("=== extract constant ===");
    match bennu_refactor::prelude::extract_constant(t3.root_node(), src3, a3, a3 + 9) {
        Some(Ok(mut p)) => { p.fill_type("String"); println!("{}", p.apply(src3)); }
        other => println!("{other:?}"),
    }
}
