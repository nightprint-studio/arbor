//! From "the text around the caret" to the token ids the model reads.
//!
//! Layout (prefix–suffix–middle): `[context header] PRE [0 at file start] prefix SUF suffix MID`, after which the
//! model writes the rest of the caret's line and `EOT`. The suffix is what follows the end of the caret's line —
//! the model sees where the code is going, not only where it was. The header is the host's (the members of the
//! types the code mentions); everything else is here, and is the same function training used.

use crate::tokenizer::{Tokenizer, MID, NEWLINE, PRE, SUF};
use std::ops::Range;

/// Longest stretch of code before the caret, in tokens (≈ 40 lines).
pub const PREFIX_TOKENS: usize = 480;
/// Longest stretch of code after the caret's line, in tokens (≈ 10 lines).
pub const SUFFIX_TOKENS: usize = 128;
/// How far back / forward to look before counting tokens.
const PREFIX_BYTES: usize = 2400;
const SUFFIX_BYTES: usize = 700;

pub struct Prompt {
    pub ids: Vec<u16>,
    /// Token ranges inside `ids`.
    pub prefix_at: Range<usize>,
    pub suffix_at: Range<usize>,
}

/// The last stretch of `prefix` that fits `PREFIX_TOKENS`, starting on a line boundary: its token ids, whether the
/// window is the whole file start (the model saw a separator there), and the byte offset where it begins.
fn prefix_window(tk: &Tokenizer, prefix: &str) -> (Vec<u16>, bool, usize) {
    let b = prefix.as_bytes();
    let mut start = b.len().saturating_sub(PREFIX_BYTES);
    if start > 0 {
        if let Some(nl) = b[start..].iter().position(|&c| c == b'\n') {
            start += nl + 1;
        }
    }
    while !prefix.is_char_boundary(start) {
        start += 1;
    }
    let ids = tk.encode(&prefix[start..]);
    if ids.len() <= PREFIX_TOKENS {
        return (ids, start == 0, start);
    }
    // drop whole lines from the top until it fits
    let from = ids.len() - PREFIX_TOKENS - 1;
    match ids[from..].iter().position(|&i| i == NEWLINE) {
        Some(k) => {
            let cut = from + k + 1;
            let dropped = tk.decode(&ids[..cut]).len();
            (ids[cut..].to_vec(), false, start + dropped)
        }
        None => (ids[ids.len() - PREFIX_TOKENS..].to_vec(), false, start),
    }
}

/// The part of `prefix` the prompt will actually contain, for a host that ranks its header by what the nearby
/// code mentions.
pub fn code_window<'a>(tk: &Tokenizer, prefix: &'a str) -> &'a str {
    let (_, _, from) = prefix_window(tk, prefix);
    &prefix[from.min(prefix.len())..]
}

/// The first lines of `suffix` (which starts at the newline ending the caret's line).
fn suffix_window(tk: &Tokenizer, suffix: &str) -> Vec<u16> {
    let mut end = suffix.len().min(SUFFIX_BYTES);
    while !suffix.is_char_boundary(end) {
        end -= 1;
    }
    let text = if end == suffix.len() { suffix } else { suffix[..end].rfind('\n').map_or("", |p| &suffix[..=p]) };
    let mut ids = tk.encode(text);
    if ids.len() > SUFFIX_TOKENS {
        ids.truncate(SUFFIX_TOKENS);
        if let Some(last) = ids.iter().rposition(|&i| i == NEWLINE) {
            ids.truncate(last + 1);
        }
    }
    ids
}

pub fn build(tk: &Tokenizer, header: &str, prefix: &str, suffix: &str) -> Prompt {
    let (prefix_ids, at_file_start, _) = prefix_window(tk, prefix);
    let suffix_ids = suffix_window(tk, suffix);
    let mut ids = tk.encode(header);
    ids.push(PRE);
    if at_file_start {
        ids.push(0);
    }
    let p0 = ids.len();
    ids.extend_from_slice(&prefix_ids);
    let p1 = ids.len();
    ids.push(SUF);
    let s0 = ids.len();
    ids.extend_from_slice(&suffix_ids);
    let s1 = ids.len();
    ids.push(MID);
    Prompt { ids, prefix_at: p0..p1, suffix_at: s0..s1 }
}

impl Prompt {
    /// Drops tokens from the top of the prefix until the prompt is at most `limit` long. Training discards examples
    /// that do not fit, so the model never saw one; at inference the caret has to be answered anyway, and the
    /// oldest code is the least informative.
    pub fn fit(&mut self, limit: usize) {
        let over = self.ids.len().saturating_sub(limit).min(self.prefix_at.len());
        if over == 0 {
            return;
        }
        let from = self.prefix_at.start;
        self.ids.drain(from..from + over);
        self.prefix_at.end -= over;
        self.suffix_at = self.suffix_at.start - over..self.suffix_at.end - over;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn tokenizer() -> Tokenizer {
        let mut counts = HashMap::new();
        crate::tokenizer::count_chunks("int count = items.size();\nreturn count;\n", &mut counts);
        Tokenizer::train(counts, 20)
    }

    #[test]
    fn layout_is_header_pre_prefix_suf_suffix_mid() {
        let tk = tokenizer();
        let p = build(&tk, "//@ T: a\n", "int x = 1;\nint y", "\n}\n");
        // the prefix starts the file, so the separator follows PRE
        assert_eq!(p.ids[p.prefix_at.start - 2], PRE);
        assert_eq!(p.ids[p.prefix_at.start - 1], 0);
        assert_eq!(p.ids[p.prefix_at.end], SUF);
        assert_eq!(*p.ids.last().unwrap(), MID);
        assert_eq!(p.suffix_at.end, p.ids.len() - 1);
    }

    #[test]
    fn a_prefix_that_starts_the_file_is_marked() {
        let tk = tokenizer();
        let p = build(&tk, "", "int", "");
        assert_eq!(p.ids[0], PRE);
        assert_eq!(p.ids[1], 0);
    }

    #[test]
    fn fit_cuts_the_oldest_code_and_keeps_the_ranges_consistent() {
        let tk = tokenizer();
        let prefix = "int a = 1;\n".repeat(60);
        let mut p = build(&tk, "", &prefix, "\n}\n");
        let before = p.ids.len();
        p.fit(before - 10);
        assert_eq!(p.ids.len(), before - 10);
        assert_eq!(p.ids[p.prefix_at.end], SUF);
        assert_eq!(*p.ids.last().unwrap(), MID);
    }
}
