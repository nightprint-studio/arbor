//! One `.properties` file: which bundle it belongs to, which locale it is, and the keys it
//! declares with the byte span of each.
//!
//! The parser is the Java one, not a generic key/value one, because the differences are exactly
//! where a legacy bundle lives: `=`, `:` and bare whitespace are all separators, a trailing
//! backslash continues the line, and a separator can be escaped INTO a key. Getting that wrong
//! does not fail loudly — it silently reports a key as missing while the application renders it
//! perfectly.

/// One declared key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The key, unescaped — what a reference has to match.
    pub key: String,
    /// The value as written, with continuations joined. Not unescaped beyond the line joins:
    /// this is shown to a reader, and a legacy bundle's `à` is more honest left alone than
    /// half-decoded.
    pub value: String,
    /// Byte offset of the key text in the file (the go-to target).
    pub start: usize,
    /// Byte offset one past the key text.
    pub end: usize,
    /// 1-based line of the declaration.
    pub line: u32,
}

/// One `.properties` file.
#[derive(Debug, Clone)]
pub struct Bundle {
    /// Absolute path, forward-slashed.
    pub path: String,
    /// The bundle this file is one translation of — the file name with the locale suffix and the
    /// extension removed (`messages_it_IT.properties` → `messages`).
    pub base: String,
    /// The locale suffix, empty for the default file (`it`, `it_IT`).
    pub locale: String,
    pub entries: Vec<Entry>,
}

impl Bundle {
    /// Parse one file. `path` is kept as given (normalized to forward slashes).
    pub fn parse(path: &str, text: &str) -> Self {
        let path = path.replace('\\', "/");
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        let (base, locale) = split_locale(strip_extension(&name));
        Bundle { path, base, locale, entries: entries(text) }
    }

    /// The entry declaring `key`, if this file does.
    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.key == key)
    }

    /// How this file is named in a list: `messages_it.properties`.
    pub fn file_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// The locale as a person reads it — the default file says so rather than showing nothing.
    pub fn locale_label(&self) -> &str {
        if self.locale.is_empty() {
            "default"
        } else {
            &self.locale
        }
    }
}

/// `messages_it_IT.properties` → `messages_it_IT`. Only the `.properties` extension is stripped;
/// a dot inside the stem (`com.acme.messages`) is part of the name.
fn strip_extension(name: &str) -> &str {
    name.strip_suffix(".properties").unwrap_or(name)
}

/// Split a stem into its bundle base and locale suffix.
///
/// Conservative on purpose. `labels_admin.properties` is a bundle called `labels_admin`, not the
/// `admin` translation of `labels` — so a segment only counts as a locale when it has the SHAPE
/// of one: a two-or-three-letter lowercase language, optionally followed by a two-letter
/// uppercase country. Anything else stays part of the base, which is the answer that degrades
/// harmlessly (one bundle too many, never a key filed under the wrong name).
fn split_locale(stem: &str) -> (String, String) {
    let parts: Vec<&str> = stem.split('_').collect();
    let n = parts.len();
    let is_lang =
        |s: &str| (2..=3).contains(&s.len()) && s.chars().all(|c| c.is_ascii_lowercase());
    let is_country = |s: &str| s.len() == 2 && s.chars().all(|c| c.is_ascii_uppercase());

    if n >= 3 && is_country(parts[n - 1]) && is_lang(parts[n - 2]) {
        return (parts[..n - 2].join("_"), format!("{}_{}", parts[n - 2], parts[n - 1]));
    }
    if n >= 2 && is_lang(parts[n - 1]) {
        return (parts[..n - 1].join("_"), parts[n - 1].to_string());
    }
    (stem.to_string(), String::new())
}

/// Parse the declarations out of a `.properties` text.
fn entries(text: &str) -> Vec<Entry> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut line = 1u32;
    let mut i = 0usize;

    while i < bytes.len() {
        let line_start = i;
        let line_end = memchr_nl(bytes, i);
        let this_line = line;
        line += 1;

        // Leading whitespace, then blanks and comments.
        let mut p = line_start;
        while p < line_end && (bytes[p] == b' ' || bytes[p] == b'\t' || bytes[p] == b'\x0c') {
            p += 1;
        }
        if p >= line_end || bytes[p] == b'#' || bytes[p] == b'!' {
            i = line_end + 1;
            continue;
        }

        // The key runs to the first UNESCAPED separator: `=`, `:` or whitespace.
        let key_start = p;
        let mut key_end = line_end;
        let mut q = p;
        while q < line_end {
            if bytes[q] == b'\\' {
                q += 2;
                continue;
            }
            if matches!(bytes[q], b'=' | b':' | b' ' | b'\t' | b'\x0c') {
                key_end = q;
                break;
            }
            q += 1;
        }
        let key = unescape(&text[key_start..key_end.min(line_end)]);

        // The value: the rest of this line, plus every line a trailing backslash continues into.
        let mut value_start = key_end.min(line_end);
        while value_start < line_end
            && matches!(bytes[value_start], b' ' | b'\t' | b'\x0c' | b'=' | b':')
        {
            value_start += 1;
        }
        let mut value = text[value_start..line_end].trim_end().to_string();
        let mut end = line_end;
        while continues(&value) {
            value.pop(); // the backslash itself
            let next_start = end + 1;
            if next_start >= bytes.len() {
                break;
            }
            let next_end = memchr_nl(bytes, next_start);
            value.push_str(text[next_start..next_end].trim_start().trim_end());
            end = next_end;
            line += 1;
        }

        if !key.is_empty() {
            out.push(Entry { key, value, start: key_start, end: key_end.min(line_end), line: this_line });
        }
        i = end + 1;
    }
    out
}

/// Whether a value line is continued by the next one — an ODD number of trailing backslashes,
/// since `\\` at the end is an escaped backslash and terminates.
fn continues(value: &str) -> bool {
    value.bytes().rev().take_while(|b| *b == b'\\').count() % 2 == 1
}

/// Byte index of the newline at or after `from`, or the end of the text.
fn memchr_nl(bytes: &[u8], from: usize) -> usize {
    bytes[from..].iter().position(|b| *b == b'\n').map(|p| from + p).unwrap_or(bytes.len())
}

/// Undo the escapes a key may carry (`\ `, `\=`, `\:`, `\\`). Unknown escapes keep the
/// character, which is what `Properties` does.
fn unescape(raw: &str) -> String {
    if !raw.contains('\\') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_locale_suffix_is_recognised_only_when_it_looks_like_one() {
        assert_eq!(split_locale("messages"), ("messages".into(), String::new()));
        assert_eq!(split_locale("messages_it"), ("messages".into(), "it".into()));
        assert_eq!(split_locale("messages_it_IT"), ("messages".into(), "it_IT".into()));
        // The trap: a bundle whose own name has an underscore.
        assert_eq!(split_locale("labels_admin"), ("labels_admin".into(), String::new()));
        assert_eq!(split_locale("global_it_IT"), ("global".into(), "it_IT".into()));
    }

    #[test]
    fn every_java_separator_ends_a_key() {
        let b = Bundle::parse("/p/m.properties", "a=1\nb:2\nc 3\n");
        assert_eq!(b.entries.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!(b.entries.iter().map(|e| e.value.as_str()).collect::<Vec<_>>(), ["1", "2", "3"]);
    }

    #[test]
    fn the_key_span_is_the_key_text_so_a_go_to_selects_it() {
        let text = "# a comment\nlogin.title = Accedi\n";
        let b = Bundle::parse("/p/m.properties", text);
        let e = &b.entries[0];
        assert_eq!(&text[e.start..e.end], "login.title");
        assert_eq!(e.line, 2);
        assert_eq!(e.value, "Accedi");
    }

    #[test]
    fn a_trailing_backslash_continues_the_value() {
        let b = Bundle::parse(
            "/p/m.properties",
            "long=first \\\n  second\nnext=x\n",
        );
        assert_eq!(b.entries.len(), 2);
        assert_eq!(b.entries[0].value, "first second");
        assert_eq!(b.entries[1].key, "next", "the continuation did not eat the next entry");
        assert_eq!(b.entries[1].line, 3);
    }

    #[test]
    fn blank_lines_and_both_comment_markers_declare_nothing() {
        let b = Bundle::parse("/p/m.properties", "\n# one\n! two\n   \nreal=1\n");
        assert_eq!(b.entries.len(), 1);
        assert_eq!(b.entries[0].key, "real");
    }

    #[test]
    fn an_escaped_separator_stays_in_the_key() {
        let b = Bundle::parse("/p/m.properties", "a\\=b=value\n");
        assert_eq!(b.entries[0].key, "a=b");
        assert_eq!(b.entries[0].value, "value");
    }

    #[test]
    fn a_key_with_no_value_is_still_declared() {
        // Legacy bundles are full of these — a key placed for a translator who never came.
        let b = Bundle::parse("/p/m.properties", "empty=\nalso.empty\n");
        assert_eq!(b.entries.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["empty", "also.empty"]);
        assert!(b.entries.iter().all(|e| e.value.is_empty()));
    }
}
