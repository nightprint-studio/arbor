//! A tiny TOML writer, because the files must read like documents.
//!
//! `toml`'s serializer is right but not pretty: a JSON body ends up as one line of `\n` escapes, which is
//! unreadable in a diff and defeats the point of keeping requests as files. This writer emits the few shapes
//! Hirundo needs — strings (multi-line ones as `'''` literals), integers, booleans, tables — in the order the
//! caller gives. Reading stays with `toml`, so anything this writes is checked by a round-trip test.

/// A TOML basic string: `"…"` with the escapes TOML requires.
pub fn basic(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Whether `s` can be written between `'''` as it is: it has a newline worth keeping, and nothing a literal
/// string cannot hold (a `'''`, a carriage return or another control character, a trailing `'` that would
/// run into the closing quotes).
fn fits_literal(s: &str) -> bool {
    s.contains('\n') && !s.contains("'''") && !s.ends_with('\'') && s.chars().all(|c| c == '\n' || c == '\t' || ((c as u32) >= 0x20 && c as u32 != 0x7f))
}

#[derive(Default)]
pub struct Writer {
    out: String,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    /// `key = value`, multi-line as a literal block when it can be.
    pub fn string(&mut self, key: &str, value: &str) {
        self.out.push_str(key);
        self.out.push_str(" = ");
        if fits_literal(value) {
            self.out.push_str("'''\n");
            self.out.push_str(value);
            self.out.push_str("'''");
        } else {
            self.out.push_str(&basic(value));
        }
        self.out.push('\n');
    }

    /// Like [`Self::string`], but nothing is written for an empty value.
    pub fn text(&mut self, key: &str, value: &str) {
        if !value.is_empty() {
            self.string(key, value);
        }
    }

    pub fn int(&mut self, key: &str, value: i64) {
        self.out.push_str(&format!("{key} = {value}\n"));
    }

    pub fn bool(&mut self, key: &str, value: bool) {
        self.out.push_str(&format!("{key} = {value}\n"));
    }

    /// `[name]`, after a blank line.
    pub fn table(&mut self, name: &str) {
        self.header(&format!("[{name}]"));
    }

    /// `[[name]]`, after a blank line.
    pub fn array_table(&mut self, name: &str) {
        self.header(&format!("[[{name}]]"));
    }

    fn header(&mut self, line: &str) {
        if !self.out.is_empty() {
            self.out.push('\n');
        }
        self.out.push_str(line);
        self.out.push('\n');
    }

    pub fn finish(self) -> String {
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_back(text: &str, key: &str) -> String {
        let doc: toml::Table = toml::from_str(text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        doc[key].as_str().unwrap().to_string()
    }

    #[test]
    fn a_multi_line_value_is_a_literal_block_and_reads_back_exactly() {
        for value in ["{\n  \"a\": 1\n}", "line\n", "\nleading newline", "tab\there\nx", "a\\b\nc \"quoted\"\n"] {
            let mut w = Writer::new();
            w.string("body", value);
            let text = w.finish();
            assert!(text.contains("'''"), "{text}");
            assert_eq!(read_back(&text, "body"), value, "{text}");
        }
    }

    #[test]
    fn what_a_literal_block_cannot_hold_falls_back_to_an_escaped_string() {
        for value in ["has ''' inside\nx", "ends with a quote'\nx'", "carriage\r\nreturn", "bell\u{7}\nx", "single line"] {
            let mut w = Writer::new();
            w.string("k", value);
            let text = w.finish();
            assert!(!text.contains("'''") || value.contains("'''"), "{text}");
            assert_eq!(read_back(&text, "k"), value, "{text}");
        }
    }

    #[test]
    fn tables_come_after_a_blank_line_and_an_empty_text_is_skipped() {
        let mut w = Writer::new();
        w.string("name", "x");
        w.text("docs", "");
        w.int("order", 3);
        w.bool("on", true);
        w.table("auth");
        w.string("type", "bearer");
        w.array_table("type");
        w.string("id", "dev");
        assert_eq!(w.finish(), "name = \"x\"\norder = 3\non = true\n\n[auth]\ntype = \"bearer\"\n\n[[type]]\nid = \"dev\"\n");
    }
}
