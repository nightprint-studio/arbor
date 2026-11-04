//! Writing Java (and anything else indented): a line buffer that tracks depth, and an import set
//! that renders the way the rest of the project's files do.
//!
//! Generators describe code through [`Lines`] and register what they used through [`Imports`]; none
//! of them concatenates strings by hand, so indentation and import hygiene are decided here once.

use std::collections::BTreeSet;

/// The imports a generated file needs, as fully-qualified names. `static pkg.Type.member` is a
/// static import.
#[derive(Debug, Default, Clone)]
pub struct Imports(BTreeSet<String>);

impl Imports {
    pub fn add(&mut self, fqn: impl AsRef<str>) -> &mut Self {
        self.0.insert(fqn.as_ref().trim().to_string());
        self
    }

    pub fn add_static(&mut self, fqn: impl AsRef<str>) -> &mut Self {
        self.0.insert(format!("static {}", fqn.as_ref().trim()));
        self
    }

    pub fn extend<I, S>(&mut self, items: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for item in items {
            self.add(item);
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Sorted, without `java.lang` and without the types of `package` itself, which need none.
    pub fn resolved(&self, package: &str) -> Vec<String> {
        self.0.iter().filter(|fqn| !is_implicit(fqn, package)).cloned().collect()
    }

    /// The `import` block: statics first, a blank line, then the rest.
    pub fn render(&self, package: &str) -> String {
        let all = self.resolved(package);
        let (statics, plain): (Vec<&String>, Vec<&String>) = all.iter().partition(|fqn| fqn.starts_with("static "));
        let mut out = String::new();
        for fqn in &statics {
            out.push_str(&format!("import {fqn};\n"));
        }
        if !statics.is_empty() && !plain.is_empty() {
            out.push('\n');
        }
        for fqn in &plain {
            out.push_str(&format!("import {fqn};\n"));
        }
        out
    }
}

fn is_implicit(fqn: &str, package: &str) -> bool {
    if fqn.starts_with("static ") {
        return false;
    }
    let Some((owner, name)) = fqn.rsplit_once('.') else { return true };
    if name == "*" {
        return owner == package || owner == "java.lang";
    }
    owner == package || (owner == "java.lang" && !name.contains('.'))
}

/// An indenting line buffer. Four spaces per level, blank lines carry no trailing whitespace.
#[derive(Debug, Default, Clone)]
pub struct Lines {
    out: Vec<String>,
    depth: usize,
}

impl Lines {
    pub fn new() -> Self {
        Self::default()
    }

    /// One line at the current depth; `""` is a blank line.
    pub fn line(&mut self, text: impl AsRef<str>) -> &mut Self {
        let text = text.as_ref();
        if text.is_empty() {
            self.out.push(String::new());
        } else {
            self.out.push(format!("{}{text}", "    ".repeat(self.depth)));
        }
        self
    }

    pub fn blank(&mut self) -> &mut Self {
        self.line("")
    }

    /// A blank line, unless the buffer is empty or already ends in one.
    pub fn gap(&mut self) -> &mut Self {
        if self.out.last().is_some_and(|l| !l.is_empty()) {
            self.blank();
        }
        self
    }

    /// Several lines written at the current depth; their own relative indentation is kept.
    pub fn text(&mut self, text: &str) -> &mut Self {
        for line in text.lines() {
            self.line(line);
        }
        self
    }

    pub fn indent(&mut self) -> &mut Self {
        self.depth += 1;
        self
    }

    pub fn dedent(&mut self) -> &mut Self {
        self.depth = self.depth.saturating_sub(1);
        self
    }

    /// `header {` … `}` with the body written by `f` one level deeper.
    pub fn block(&mut self, header: impl AsRef<str>, f: impl FnOnce(&mut Lines)) -> &mut Self {
        self.line(format!("{} {{", header.as_ref()));
        self.indent();
        f(self);
        self.dedent();
        self.line("}")
    }

    /// Continues the block just closed: `} catch (IOException e) {` … `}`.
    pub fn block_after(&mut self, header: impl AsRef<str>, f: impl FnOnce(&mut Lines)) -> &mut Self {
        let closing = self.out.last().filter(|last| last.trim() == "}").cloned();
        let joined = match closing {
            Some(last) => {
                let indent = last.len() - last.trim_start().len();
                self.out.pop();
                self.out.push(format!("{}}} {} {{", &last[..indent], header.as_ref()));
                true
            }
            None => false,
        };
        if !joined {
            self.line(format!("{} {{", header.as_ref()));
        }
        self.indent();
        f(self);
        self.dedent();
        self.line("}")
    }

    /// Like [`Lines::block`], closed by `closing` (`};` for a lambda or an anonymous class).
    pub fn block_closing(&mut self, header: impl AsRef<str>, closing: &str, f: impl FnOnce(&mut Lines)) -> &mut Self {
        self.line(format!("{} {{", header.as_ref()));
        self.indent();
        f(self);
        self.dedent();
        self.line(closing)
    }

    /// Append another buffer, re-indented to the current depth.
    pub fn append(&mut self, other: &Lines) -> &mut Self {
        for l in &other.out {
            self.line(l);
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.out.is_empty()
    }

    /// The text, one trailing newline.
    pub fn finish(&self) -> String {
        let mut text = self.out.join("\n");
        text.push('\n');
        text
    }
}

/// A Java compilation unit being written: `package`, the imports the body registered, the body.
#[derive(Debug, Clone)]
pub struct JavaFile {
    pub package: String,
    pub imports: Imports,
    pub body: Lines,
}

impl JavaFile {
    pub fn new(package: impl Into<String>) -> Self {
        JavaFile { package: package.into(), imports: Imports::default(), body: Lines::new() }
    }

    /// The imports, plus the common JDK types the body uses without a hand-written import
    /// (`List`, `BigDecimal`, `LocalDate`, …) — a generator writes the type, not the plumbing.
    fn with_known_imports(&self, body: &str) -> Imports {
        let mut imports = self.imports.clone();
        let mut seen = BTreeSet::new();
        for word in body.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')) {
            if !seen.insert(word) {
                continue;
            }
            let Some(fqn) = known_import(word) else { continue };
            let taken = imports.0.iter().any(|i| !i.starts_with("static ") && i.rsplit('.').next() == Some(word));
            if !taken {
                imports.add(fqn);
            }
        }
        imports
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        if !self.package.is_empty() {
            out.push_str(&format!("package {};\n\n", self.package));
        }
        let body = self.body.finish();
        let imports = self.with_known_imports(&body).render(&self.package);
        if !imports.is_empty() {
            out.push_str(&imports);
            out.push('\n');
        }
        out.push_str(&body);
        out
    }
}

/// The JDK types a generator may name without importing.
fn known_import(simple: &str) -> Option<&'static str> {
    Some(match simple {
        "List" => "java.util.List",
        "Set" => "java.util.Set",
        "Map" => "java.util.Map",
        "Optional" => "java.util.Optional",
        "Collection" => "java.util.Collection",
        "ArrayList" => "java.util.ArrayList",
        "Arrays" => "java.util.Arrays",
        "Objects" => "java.util.Objects",
        "UUID" => "java.util.UUID",
        "BigDecimal" => "java.math.BigDecimal",
        "BigInteger" => "java.math.BigInteger",
        "LocalDate" => "java.time.LocalDate",
        "LocalDateTime" => "java.time.LocalDateTime",
        "LocalTime" => "java.time.LocalTime",
        "Instant" => "java.time.Instant",
        "Duration" => "java.time.Duration",
        "OffsetDateTime" => "java.time.OffsetDateTime",
        "ZonedDateTime" => "java.time.ZonedDateTime",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_are_sorted_statics_first_and_implicit_ones_dropped() {
        let mut imports = Imports::default();
        imports.add("java.util.List").add("java.lang.String").add("com.acme.Same").add("a.b.C");
        imports.add_static("org.junit.Assert.assertEquals");
        let text = imports.render("com.acme");
        assert_eq!(
            text,
            "import static org.junit.Assert.assertEquals;\n\nimport a.b.C;\nimport java.util.List;\n"
        );
    }

    #[test]
    fn a_nested_java_lang_type_is_still_imported() {
        let mut imports = Imports::default();
        imports.add("java.lang.Thread.State");
        assert_eq!(imports.resolved("x"), vec!["java.lang.Thread.State".to_string()]);
    }

    #[test]
    fn blocks_indent_and_blank_lines_stay_empty() {
        let mut l = Lines::new();
        l.block("class A", |l| {
            l.line("int a;");
            l.blank();
            l.block("void f()", |l| {
                l.line("return;");
            });
        });
        assert_eq!(l.finish(), "class A {\n    int a;\n\n    void f() {\n        return;\n    }\n}\n");
    }

    #[test]
    fn a_file_without_imports_has_no_stray_blank_line() {
        let mut f = JavaFile::new("p");
        f.body.line("class A {}");
        assert_eq!(f.render(), "package p;\n\nclass A {}\n");
    }

    #[test]
    fn gap_does_not_double_blank_lines() {
        let mut l = Lines::new();
        l.gap().line("a").gap().gap().line("b");
        assert_eq!(l.finish(), "a\n\nb\n");
    }
}

#[cfg(test)]
mod block_after_tests {
    use super::*;

    #[test]
    fn a_catch_continues_the_try_that_was_just_closed() {
        let mut l = Lines::new();
        l.block("void f()", |l| {
            l.block("try", |l| {
                l.line("run();");
            });
            l.block_after("catch (IOException e)", |l| {
                l.line("throw new UncheckedIOException(e);");
            });
        });
        assert_eq!(
            l.finish(),
            "void f() {\n    try {\n        run();\n    } catch (IOException e) {\n        throw new UncheckedIOException(e);\n    }\n}\n"
        );
    }

    #[test]
    fn known_jdk_types_are_imported_when_the_body_names_them() {
        let mut f = JavaFile::new("p");
        f.body.line("List<BigDecimal> prices;");
        f.imports.add("com.other.Optional");
        f.body.line("Optional<String> x;");
        let text = f.render();
        assert!(text.contains("import java.math.BigDecimal;"), "{text}");
        assert!(text.contains("import java.util.List;"), "{text}");
        assert!(!text.contains("import java.util.Optional;"), "{text}");
    }
}
