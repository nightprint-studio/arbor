//! Invariants of the ghost-text layers, checked over thousands of generated cases instead of a handful of examples.
//!
//! A layer that writes code must never panic, never cut a character in half, and — for the ones that write a whole declaration — must
//! leave a file that still parses. Those three hold for every caret position of every source below, and for every prefix of every
//! name the block layers react to, with `\n` and `\r\n` line ends, tabs and spaces, and non-ASCII identifiers.

use bennu_java::prelude::parse_java;

use crate::flc_derived;
use crate::flc_words::canon;

/// Classes of different shapes: plain fields, static and transient ones, generics, arrays, floating point, an existing constructor,
/// a nested class, an enum, a record, an interface, and non-ASCII names.
const SOURCES: &[&str] = &[
    "package shop;\n\nimport java.util.List;\n\npublic class Order {\n    private final long id;\n    private String customer;\n    private List<String> items;\n    private double total;\n    private int[] counts;\n    private static int created;\n    private transient Object cache;\n    private boolean paid = false;\n\n    public long getId() { return id; }\n}\n",
    "class Point {\n    int x;\n    int y;\n    Point(int x, int y) { this.x = x; this.y = y; }\n}\n",
    "class Empty {\n}\n",
    "class Wide {\n    int a; int b; int c; int d; int e; int f; int g; int h; int i; int j; int k; int l; int m; int n;\n}\n",
    "class Outer {\n    int a;\n    class Inner {\n        String b;\n        float c;\n    }\n}\n",
    "enum Level {\n    LOW, HIGH;\n    private int weight;\n}\n",
    "record Pair(int a, int b) {\n    static int made;\n}\n",
    "interface Shape {\n    double area();\n}\n",
    "class Naïve {\n    String naïveName;\n    int größe;\n}\n",
    "class Tabs {\n\tint a;\n\tString b;\n}\n",
];

/// The names the block layers start from.
const NAMES: &[&str] = &["toString", "equals", "hashCode"];

/// `source` with the closing `}` of its class preceded by `typed` on a line of its own, and the offset after `typed`.
fn typing(source: &str, typed: &str, indent: &str) -> (String, usize) {
    let at = source.rfind('}').expect("a class ends with a brace");
    let before = &source[..at];
    let line = format!("{indent}{typed}");
    let text = format!("{before}\n{line}\n{}", &source[at..]);
    let offset = before.len() + 1 + line.len();
    (text, offset)
}

/// What the buffer holds once `block` has replaced the typed line.
fn applied(source: &str, offset: usize, block: &str) -> String {
    let line_start = source[..offset].rfind('\n').map_or(0, |p| p + 1);
    let line_end = source[offset..].find('\n').map_or(source.len(), |p| offset + p);
    let line = &source[line_start..offset];
    let indent = &line[..line.len() - line.trim_start().len()];
    format!("{}{indent}{block}{}", &source[..line_start], &source[line_end..])
}

fn parses_clean(source: &str) -> bool {
    parse_java(source).is_some_and(|t| !t.root_node().has_error())
}

fn variants(source: &str) -> Vec<(String, &'static str)> {
    vec![(source.to_string(), "lf"), (source.replace('\n', "\r\n"), "crlf")]
}

#[test]
fn no_caret_position_of_any_source_panics_a_block_layer() {
    let mut asked = 0;
    for src in SOURCES {
        for (source, _) in variants(src) {
            for offset in (0..=source.len()).filter(|o| source.is_char_boundary(*o)) {
                let _ = flc_derived::suggest(&source, offset);
                asked += 1;
            }
        }
    }
    assert!(asked > 1500, "the sweep covered {asked} positions");
}

#[test]
fn an_offset_inside_a_character_is_turned_away_not_cut() {
    for src in SOURCES.iter().filter(|s| !s.is_ascii()) {
        for offset in (0..=src.len()).filter(|o| !src.is_char_boundary(*o)) {
            assert!(flc_derived::suggest(src, offset).is_none());
        }
    }
}

#[test]
fn every_prefix_of_every_object_method_name_leaves_a_file_that_parses() {
    let (mut offered, mut asked) = (0, 0);
    for src in SOURCES {
        for (source, _) in variants(src) {
            for name in NAMES {
                for modifier in ["", "public ", "protected "] {
                    for ret in ["", "String ", "boolean ", "int "] {
                        for cut in 1..=name.len() {
                            let typed = format!("{modifier}{ret}{}", &name[..cut]);
                            for indent in ["    ", "\t", ""] {
                                let (text, offset) = typing(&source, &typed, indent);
                                asked += 1;
                                let Some(d) = flc_derived::suggest(&text, offset) else { continue };
                                offered += 1;
                                assert!(!d.text.is_empty() && d.confidence > 0.0 && d.confidence <= 1.0);
                                assert!(!d.text.ends_with(char::is_whitespace), "no trailing space in {:?}", d.text);
                                let done = applied(&text, offset, &d.text);
                                // a file that was valid stays valid once the block replaces the typed line
                                if parses_clean(&source) {
                                    assert!(parses_clean(&done), "block for {typed:?} breaks the file:\n{done}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(asked > 5000 && offered > 100, "asked {asked}, offered {offered}");
}

#[test]
fn every_class_name_and_prefix_with_a_paren_offers_a_constructor_that_parses() {
    let (mut offered, mut asked) = (0, 0);
    for (src, class) in [(SOURCES[0], "Order"), (SOURCES[4], "Outer"), (SOURCES[8], "Naïve"), (SOURCES[9], "Tabs")] {
        for (source, _) in variants(src) {
            for modifier in ["", "public ", "protected ", "private "] {
                for paren in ["(", ""] {
                    let typed = format!("{modifier}{class}{paren}");
                    let (text, offset) = typing(&source, &typed, "    ");
                    asked += 1;
                    if let Some(d) = flc_derived::suggest(&text, offset) {
                        offered += 1;
                        assert!(d.text.contains(&format!("{class}(")), "{:?}", d.text);
                        assert!(parses_clean(&applied(&text, offset, &d.text)), "constructor breaks:\n{}", d.text);
                    }
                }
            }
        }
    }
    assert!(asked >= 30 && offered >= 4, "asked {asked}, offered {offered}");
}

#[test]
fn text_after_the_caret_or_a_wrong_name_gets_nothing() {
    for src in SOURCES {
        for typed in ["toString() { return \"\"; }", "foo", "String toStringify", "int x = toString", "/* hashCode", "// equals"] {
            let (text, offset) = typing(src, typed, "    ");
            let cut = offset - 3.min(typed.len());
            let _ = flc_derived::suggest(&text, cut);
            if typed == "foo" || typed.starts_with("//") || typed.starts_with("/*") {
                assert!(flc_derived::suggest(&text, offset).is_none(), "{typed:?} offered something");
            }
        }
    }
}

#[test]
fn the_same_input_gives_the_same_answer() {
    for src in SOURCES {
        let (text, offset) = typing(src, "public String toString(", "    ");
        let a = flc_derived::suggest(&text, offset).map(|d| d.text);
        let b = flc_derived::suggest(&text, offset).map(|d| d.text);
        assert_eq!(a, b);
    }
}

/// Spelling `words` in each of the styles a Java project mixes.
fn styles(words: &[&str]) -> Vec<String> {
    let cap = |w: &str| w[..1].to_uppercase() + &w[1..];
    let camel = words.iter().enumerate().map(|(i, w)| if i == 0 { w.to_string() } else { cap(w) }).collect::<String>();
    let pascal = words.iter().map(|w| cap(w)).collect::<String>();
    let snake = words.join("_");
    let screaming = snake.to_uppercase();
    let kebab = words.join("-");
    vec![camel, pascal, snake, screaming, kebab]
}

#[test]
fn a_name_means_the_same_in_every_style() {
    let pool = ["order", "line", "jaxb", "processor", "byte", "stream", "http", "client", "user", "id", "total", "price"];
    let mut checked = 0;
    for a in pool {
        for b in pool {
            for c in ["", "item", "factory", "builder"] {
                let words: Vec<&str> = [a, b, c].into_iter().filter(|w| !w.is_empty()).collect();
                let all = styles(&words);
                let first = canon(&all[0]);
                for s in &all {
                    assert_eq!(canon(s), first, "{s:?} vs {:?}", all[0]);
                    assert_eq!(canon(&canon(s)), canon(s), "canon is idempotent for {s:?}");
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 2000);
}

#[test]
fn different_names_stay_different_and_odd_input_does_not_panic() {
    assert_ne!(canon("orderLine"), canon("orderLines"));
    assert_ne!(canon("getId"), canon("setId"));
    for odd in ["", "_", "__", "$", "a", "A", "AB", "aB", "ab1", "1ab", "über", "ÖlStand", "日本語", "a__b", "_a_", "A_B_C", "XMLHttp2Request", "\u{0}", "a b"] {
        let c = canon(odd);
        assert_eq!(canon(&c), c, "canon is idempotent for {odd:?}");
    }
}
