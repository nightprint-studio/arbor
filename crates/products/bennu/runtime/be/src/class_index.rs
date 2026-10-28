//! `class_index` domain — `bennu_class_index`, powering the "Go to Class" navigator.
//!
//! **Cache-first**: when a project is open at `root`, the entries were already captured
//! during the background index build (the same parse that built the symbol index — no
//! separate whole-project scan), so this handler returns them instantly. The **fresh**
//! source scan below is the fallback for when no open project owns `root` or its build
//! hasn't landed yet.
//!
//! The fresh scan walks the project's `.java` sources under `root` with [`collect_java`],
//! runs [`extract_symbols`] on each, and emits one [`ClassEntry`] per declared type (class
//! / interface / enum, including nested types).
//!
//! `extract_symbols`' [`TypeDecl`] carries no byte offset / line, so the declaration
//! line is recovered here by lexing the source for `class`/`interface`/`enum`/`record <Name>`
//! (comments and literals skipped; repeated names claimed in source order); a type that can't
//! be located that way falls back to line 1. Sources are decoded in the project's declared encoding (Maven
//! `sourceEncoding`) via `read_source_for_index`, recovering a mislabelled file rather than
//! dropping it, so a non-UTF-8 file still surfaces its classes; only a genuine IO error skips
//! a file (and that's logged, not silent).

use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_intel::prelude::{collect_java, read_source_for_index};
use bennu_java::prelude::extract_symbols;
use bennu_proto::prelude::ClassEntry;
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_class_index`].
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ClassIndexArgs {
    /// Absolute path to the project root to scan.
    pub root: String,
}

/// Return one [`ClassEntry`] per declared type in the project at `root`. Serves the cache
/// the index build populated when a project is open (instant, no re-parse); falls back to a
/// fresh `.java` scan otherwise.
#[arbor_rpc::handler(mcp(
    title = "List every type declared in the project",
    safety = read,
    description = "List every class, interface, enum and record the project declares, \
each with its fully-qualified name, its simple name, the file it lives in and the line \
it is declared on. This is how you go from a type name to its location without \
guessing at paths — the project's own naming conventions are rarely the ones you would \
predict from the package.",
))]
fn bennu_class_index(_ctx: &BennuState, args: ClassIndexArgs) -> Result<Vec<ClassEntry>, String> {
    // Cache-first: the open project's build already captured every declared type.
    if let Some(cached) = IndexService::global().class_index(&args.root) {
        return Ok(cached);
    }

    // A Cargo root has no Java to scan; an empty list would read as "this project declares no types".
    if crate::agent::is_cargo_root(&args.root) {
        return Err("bennu_class_index lists Java classes only; this is a Cargo project".to_string());
    }

    let mut paths = Vec::new();
    collect_java(Path::new(&args.root), &mut paths);

    // Decode in the project's declared encoding (Maven `sourceEncoding`), recovering a
    // mislabelled file rather than dropping it — same policy as the open-project build.
    let encoding = crate::index_service::encoding_plan(&args.root);
    let mut out = Vec::new();
    for path in paths {
        // Only a true IO error skips (logged inside the helper); a non-UTF-8 source is
        // decoded + recovered so its classes still surface.
        let Some(decoded) = read_source_for_index(&path, &encoding) else {
            continue;
        };
        let source = decoded.text;
        let file = path.to_string_lossy().replace('\\', "/");
        let symbols = extract_symbols(&source);
        let mut sites = DeclSites::scan(&source);
        for td in &symbols.types {
            let line = sites.next_line(&td.name).unwrap_or(1);
            out.push(ClassEntry {
                fqcn: td.fqn.clone(),
                simple: td.name.clone(),
                file: file.clone(),
                line,
                kind: td.kind.slug().to_string(),
            });
        }
    }
    Ok(out)
}

/// Where each type declaration sits in one source file, found by a small lexer rather than a text
/// search: comments, string literals and text blocks are skipped (a `class Foo` inside a Javadoc
/// example is not a declaration), and a name declared twice — nested `Builder`s — keeps every
/// occurrence, handed out in source order so each type gets its own line.
struct DeclSites {
    lines: std::collections::HashMap<String, std::collections::VecDeque<usize>>,
}

impl DeclSites {
    fn scan(source: &str) -> Self {
        let mut lines: std::collections::HashMap<String, std::collections::VecDeque<usize>> =
            std::collections::HashMap::new();
        for (name, line) in declaration_sites(source) {
            lines.entry(name).or_default().push_back(line);
        }
        Self { lines }
    }

    /// The line of the next not-yet-claimed declaration of `name`.
    fn next_line(&mut self, name: &str) -> Option<usize> {
        self.lines.get_mut(name)?.pop_front()
    }
}

/// `(name, 1-based line)` of every `class | interface | enum | record | @interface` declaration, in
/// source order.
fn declaration_sites(source: &str) -> Vec<(String, usize)> {
    let bytes = source.as_bytes();
    let (mut i, mut line) = (0usize, 1usize);
    // True right after a declaration keyword: the next identifier is the type's name.
    let mut pending = false;
    let mut out = Vec::new();
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i < bytes.len() && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/')) {
                    line += usize::from(bytes[i] == b'\n');
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            b'"' | b'\'' => {
                i = skip_literal(bytes, i, &mut line);
                pending = false;
            }
            _ if is_ident_byte(b) && !b.is_ascii_digit() => {
                let start = i;
                while i < bytes.len() && is_ident_byte(bytes[i]) {
                    i += 1;
                }
                let word = &source[start..i];
                if pending {
                    out.push((word.to_string(), line));
                    pending = false;
                } else {
                    pending = matches!(word, "class" | "interface" | "enum" | "record");
                }
            }
            _ if b.is_ascii_whitespace() => i += 1,
            // Anything else (`.class`, `)`, `=`) means the keyword was not introducing a declaration.
            _ => {
                pending = false;
                i += 1;
            }
        }
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80
}

/// Index just past the string, text block or char literal that starts at `start`, counting newlines.
fn skip_literal(bytes: &[u8], start: usize, line: &mut usize) -> usize {
    let quote = bytes[start];
    let text_block = quote == b'"' && bytes[start..].starts_with(b"\"\"\"");
    let mut i = start + if text_block { 3 } else { 1 };
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'\n' => {
                *line += 1;
                // A plain literal never spans lines; recover rather than swallow the file.
                if !text_block {
                    return i + 1;
                }
            }
            b if b == quote && (!text_block || bytes[i..].starts_with(b"\"\"\"")) => {
                return i + if text_block { 3 } else { 1 };
            }
            _ => {}
        }
        i += 1;
    }
    bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines_of(src: &str, name: &str) -> Vec<usize> {
        declaration_sites(src).into_iter().filter(|(n, _)| n == name).map(|(_, l)| l).collect()
    }

    #[test]
    fn locates_class_and_interface_lines() {
        let src = "package a.b;\n\npublic class Order {\n  int x;\n}\n";
        assert_eq!(lines_of(src, "Order"), vec![3]);
        assert_eq!(lines_of("interface Repo {}\n", "Repo"), vec![1]);
    }

    #[test]
    fn name_is_word_bounded() {
        // `Foo` must not match the declaration of `FooBar`.
        let src = "class FooBar {}\n";
        assert!(lines_of(src, "Foo").is_empty());
        assert_eq!(lines_of(src, "FooBar"), vec![1]);
    }

    #[test]
    fn records_and_annotation_types_are_found() {
        assert_eq!(lines_of("\npublic record Point(int x) {}\n", "Point"), vec![2]);
        assert_eq!(lines_of("@interface Marker {}\n", "Marker"), vec![1]);
    }

    #[test]
    fn comments_and_strings_do_not_declare_types() {
        let src = "// class Fake\n/* class Fake\n */\nString s = \"class Fake\";\nclass Real {}\n";
        assert!(lines_of(src, "Fake").is_empty());
        assert_eq!(lines_of(src, "Real"), vec![5]);
        // `Foo.class` is a class literal, not a declaration of whatever follows it.
        assert!(lines_of("Object o = Foo.class;\nint x;\n", "x").is_empty());
    }

    #[test]
    fn repeated_names_get_their_own_line() {
        let src = "class A {\n  static class Builder {}\n}\nclass B {\n  static class Builder {}\n}\n";
        let mut sites = DeclSites::scan(src);
        assert_eq!(sites.next_line("Builder"), Some(2));
        assert_eq!(sites.next_line("Builder"), Some(5));
        assert_eq!(sites.next_line("Builder"), None);
    }
}
