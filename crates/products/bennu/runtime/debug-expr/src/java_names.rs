//! `Status.ACTIVE` → the class names a JVM could know `Status` by.
//!
//! A static field or an enum constant is read from a *type*, and the JVM only knows types by their
//! binary name (`com.acme.Order$Status`). What the user typed is a name as the **source** would
//! write it, resolved against that file's imports, its package and the classes around it. This
//! module generates the candidates in the order Java would try them; the debugger asks the VM about
//! each (`VirtualMachine.ClassesBySignature`) and takes the first that is loaded.
//!
//! It is pure — no VM, no file — so the resolution order is pinned by tests rather than by a
//! debugging session.

use std::collections::HashSet;

/// What a file's import block says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JavaImports {
    /// `import com.acme.Status;` — the fully qualified names.
    pub types: Vec<String>,
    /// `import com.acme.*;` — the package (or class) before the `.*`.
    pub wildcards: Vec<String>,
    /// `import static com.acme.Limits.MAX;` as `(owner, member)`; `member` is `*` for a wildcard.
    pub statics: Vec<(String, String)>,
}

/// Read the imports out of a Java source. A line scan, not a parse: imports are one statement per
/// line in practice, and the scan stops at the first line that starts a declaration.
pub fn parse_imports(source: &str) -> JavaImports {
    let mut imports = JavaImports::default();
    for line in source.lines() {
        let line = line.trim();
        if starts_declaration(line) {
            break;
        }
        let Some(rest) = line.strip_prefix("import ") else { continue };
        let rest = rest.split("//").next().unwrap_or(rest).trim().trim_end_matches(';').trim();
        if let Some(member) = rest.strip_prefix("static ") {
            if let Some((owner, name)) = member.trim().rsplit_once('.') {
                imports.statics.push((owner.trim().to_string(), name.trim().to_string()));
            }
        } else if let Some(owner) = rest.strip_suffix(".*") {
            imports.wildcards.push(owner.trim().to_string());
        } else if !rest.is_empty() {
            imports.types.push(rest.to_string());
        }
    }
    imports
}

fn starts_declaration(line: &str) -> bool {
    ["public ", "class ", "final ", "abstract ", "interface ", "enum ", "record ", "sealed ", "@"]
        .iter()
        .any(|keyword| line.starts_with(keyword))
}

/// The classes a statically imported `name` could come from, in import order: the ones that import
/// it by name, then the wildcards.
pub fn static_import_owners(imports: &JavaImports, name: &str) -> Vec<String> {
    let named = imports.statics.iter().filter(|(_, member)| member == name);
    let wild = imports.statics.iter().filter(|(_, member)| member == "*");
    let mut owners: Vec<String> = Vec::new();
    for (owner, _) in named.chain(wild) {
        for variant in nested_variants(owner) {
            if !owners.contains(&variant) {
                owners.push(variant);
            }
        }
    }
    owners
}

/// The binary names `qualifier` could denote, most likely first.
///
/// `qualifier` is the dotted name as typed (`["Status"]`, `["Order", "Status"]`,
/// `["com", "acme", "Status"]`), and `current_class` the binary name of the class the frame is in
/// (`com.acme.Order$Line`), which supplies the package and the enclosing classes.
pub fn candidate_classes(
    imports: &JavaImports,
    current_class: &str,
    qualifier: &[String],
) -> Vec<String> {
    let Some((head, rest)) = qualifier.split_first() else { return Vec::new() };
    let mut out: Vec<String> = Vec::new();

    // A name that starts lowercase and has dots is a package path (`com.acme.Status`), which no
    // import, enclosing class or `java.lang` could complete — probing them would only cost
    // round trips to the VM.
    if starts_uppercase(head) || qualifier.len() == 1 {
        // 1. Imported by name: `import com.acme.Status;` makes `Status` mean exactly that.
        for imported in imports.types.iter().filter(|t| simple_name(t) == head) {
            for variant in nested_variants(imported) {
                out.push(join_nested(&variant, rest));
            }
        }
        // 2. A class around the one the frame is in: `Order$Line` sees `Order$Status` as `Status`.
        for enclosing in enclosing_classes(current_class) {
            out.push(join_nested(&format!("{enclosing}${head}"), rest));
        }
        // 3. The same package.
        match current_class.rsplit_once('.') {
            Some((package, _)) => out.push(join_nested(&format!("{package}.{head}"), rest)),
            None => out.push(join_nested(head, rest)),
        }
        // 4. A wildcard import: a package, or a class whose nested types are imported.
        for owner in &imports.wildcards {
            out.push(join_nested(&format!("{owner}.{head}"), rest));
            for variant in nested_variants(owner).iter().skip(1) {
                out.push(join_nested(&format!("{variant}${head}"), rest));
            }
        }
        // 5. `java.lang` is always imported.
        out.push(join_nested(&format!("java.lang.{head}"), rest));
    }
    // 6. As written, read as a fully qualified name. Packages are lowercase and types are not, so
    //    the first capitalised segment is the class and what follows it is nested
    //    (`com.acme.Order.Status` → `com.acme.Order$Status`); every other split is the fallback.
    if let Some(class_at) = qualifier.iter().position(|s| starts_uppercase(s)) {
        out.push(split_at_class(qualifier, class_at));
    }
    for class_at in (0..qualifier.len()).rev() {
        out.push(split_at_class(qualifier, class_at));
    }

    let mut seen = HashSet::new();
    out.retain(|name| seen.insert(name.clone()));
    out
}

fn starts_uppercase(segment: &str) -> bool {
    segment.chars().next().is_some_and(char::is_uppercase)
}

/// `base` with each of `rest` appended as a nested class.
fn join_nested(base: &str, rest: &[String]) -> String {
    rest.iter().fold(base.to_string(), |name, segment| format!("{name}${segment}"))
}

/// `qualifier` read with `qualifier[class_at]` as the top-level class: the segments before it are
/// its package, the ones after it nested types.
fn split_at_class(qualifier: &[String], class_at: usize) -> String {
    let package = qualifier[..class_at].join(".");
    let class = &qualifier[class_at];
    let base = if package.is_empty() { class.clone() } else { format!("{package}.{class}") };
    join_nested(&base, &qualifier[class_at + 1..])
}
/// `com.acme.Order$Status` → `Status`; `com.acme.Status` → `Status`.
fn simple_name(name: &str) -> &str {
    name.rsplit(['.', '$']).next().unwrap_or(name)
}

/// The classes a binary name sits inside, innermost first, itself included:
/// `a.B$C$1` → `a.B$C$1`, `a.B$C`, `a.B`.
fn enclosing_classes(class: &str) -> Vec<String> {
    let mut out = vec![class.to_string()];
    let mut current = class;
    while let Some((outer, _)) = current.rsplit_once('$') {
        out.push(outer.to_string());
        current = outer;
    }
    out
}

/// A dotted name read as source writes a nested class, turned into binary names: the name itself,
/// then with each dot that follows a class name as a `$`. (`a.b.Outer.Inner` → `a.b.Outer$Inner`.)
fn nested_variants(dotted: &str) -> Vec<String> {
    let mut out = vec![dotted.to_string()];
    let mut current = dotted.to_string();
    while let Some(at) = current.rfind('.') {
        // Only a dot that follows a class name can be a nesting: `model.Order` is a package path.
        let outer = current[..at].rsplit(['.', '$']).next().unwrap_or("");
        if !starts_uppercase(outer) {
            break;
        }
        current.replace_range(at..=at, "$");
        out.push(current.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    const SOURCE: &str = "package com.acme.web;\n\
        \n\
        import com.acme.model.Status;\n\
        import com.acme.model.Order; // the order\n\
        import java.util.*;\n\
        import static com.acme.Limits.MAX;\n\
        import static com.acme.Defaults.*;\n\
        \n\
        public class Controller {\n\
            import not.Real;\n\
        }\n";

    #[test]
    fn imports_are_read_up_to_the_first_declaration() {
        let imports = parse_imports(SOURCE);
        assert_eq!(imports.types, vec!["com.acme.model.Status", "com.acme.model.Order"]);
        assert_eq!(imports.wildcards, vec!["java.util"]);
        assert_eq!(
            imports.statics,
            vec![
                ("com.acme.Limits".to_string(), "MAX".to_string()),
                ("com.acme.Defaults".to_string(), "*".to_string()),
            ]
        );
    }

    #[test]
    fn a_name_the_file_imports_resolves_to_that_import_first() {
        let imports = parse_imports(SOURCE);
        let names = candidate_classes(&imports, "com.acme.web.Controller", &q(&["Status"]));
        assert_eq!(names[0], "com.acme.model.Status");
        assert!(names.contains(&"com.acme.web.Status".to_string()), "same package is tried: {names:?}");
        assert!(names.contains(&"java.lang.Status".to_string()));
    }

    #[test]
    fn a_nested_type_is_a_dollar_in_the_binary_name() {
        let imports = parse_imports(SOURCE);
        let names = candidate_classes(&imports, "com.acme.web.Controller", &q(&["Order", "Status"]));
        assert_eq!(names[0], "com.acme.model.Order$Status");
        // A class around the frame's own sees its nested types by their simple name.
        let inner = candidate_classes(&JavaImports::default(), "com.acme.Order$Line", &q(&["Status"]));
        assert_eq!(inner[0], "com.acme.Order$Line$Status");
        assert_eq!(inner[1], "com.acme.Order$Status");
        assert_eq!(inner[2], "com.acme.Status");
    }

    #[test]
    fn a_fully_qualified_name_is_tried_with_the_longest_package_first() {
        let names = candidate_classes(
            &JavaImports::default(),
            "com.acme.web.Controller",
            &q(&["com", "acme", "Order", "Status"]),
        );
        let nested = names.iter().position(|n| n == "com.acme.Order$Status").expect("tried");
        let flat = names.iter().position(|n| n == "com.acme.Order.Status").expect("tried");
        assert!(nested < flat, "{names:?}");
    }

    #[test]
    fn a_wildcard_import_and_java_lang_are_candidates() {
        let imports = parse_imports(SOURCE);
        let names = candidate_classes(&imports, "com.acme.web.Controller", &q(&["List"]));
        assert!(names.contains(&"java.util.List".to_string()), "{names:?}");
        let lang = candidate_classes(&imports, "com.acme.web.Controller", &q(&["Integer"]));
        assert!(lang.contains(&"java.lang.Integer".to_string()));
    }

    #[test]
    fn a_class_in_the_default_package_has_no_package_prefix() {
        let names = candidate_classes(&JavaImports::default(), "Main", &q(&["Mode"]));
        assert!(names.contains(&"Mode".to_string()), "{names:?}");
    }

    #[test]
    fn there_are_no_duplicates_and_an_empty_qualifier_has_no_candidates() {
        let names = candidate_classes(&JavaImports::default(), "a.B", &q(&["C"]));
        let unique: HashSet<&String> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "{names:?}");
        assert!(candidate_classes(&JavaImports::default(), "a.B", &[]).is_empty());
    }

    #[test]
    fn a_static_import_names_the_class_a_bare_constant_comes_from() {
        let imports = parse_imports(SOURCE);
        let owners = static_import_owners(&imports, "MAX");
        assert_eq!(owners[0], "com.acme.Limits", "named first");
        assert!(owners.contains(&"com.acme.Defaults".to_string()), "then the wildcard: {owners:?}");
        assert_eq!(static_import_owners(&imports, "OTHER"), vec!["com.acme.Defaults"]);
    }
}
