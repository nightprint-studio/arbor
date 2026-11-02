//! What the project wrote after the same words: the rest of a line, copied from the lines that began alike.
//!
//! Every line of every file is counted at each place a caret lands — the start of a word — as the last five (and three) tokens before it
//! → the rest of the line. A caret that has seen its five tokens at least [`MIN_SIGHTINGS`] times offers what followed them most
//! often, with that share as its confidence. Longer contexts answer first; the shorter ones are the fallback.
//!
//! The bench (`mlbench::m5_ngram`) put this layer at 82.7% exact when the share is 0.7 or more, against 35% for the model, and found
//! that most of what it gets right is a line the model does not write: the two cover different lines. So a strong share goes before the
//! model — the bench found its confidence tracks its precision (judged as far as it writes), so a strong share goes ahead of the model.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Orders tried, longest first.
const ORDERS: [usize; 2] = [5, 3];
const MIN_SIGHTINGS: u32 = 2;
/// A line shorter than this has too little to be worth copying; one longer is generated code or data.
const MIN_LINE: usize = 25;
const MAX_LINE: usize = 220;
/// Entries kept; past this the file that is read next is not counted.
const MAX_ENTRIES: usize = 600_000;
/// The share below which nothing is offered.
pub(crate) const MIN_SHARE: f32 = 0.5;
/// The share from which the answer goes before the model's.
pub(crate) const STRONG_SHARE: f32 = 0.7;

/// The rest of a line the project usually writes after these words.
pub(crate) struct Copied {
    pub text: String,
    pub confidence: f32,
}

#[derive(Default)]
struct Held {
    /// `(order index, context hash, continuation id)` each file contributed, so it can be taken back out.
    by_file: HashMap<String, Vec<(u8, u64, u32)>>,
    /// Per order: context hash → `[(continuation id, count)]`.
    tables: [HashMap<u64, Vec<(u32, u32)>>; 2],
    ids: HashMap<String, u32>,
    texts: Vec<String>,
    entries: usize,
}

fn lock() -> MutexGuard<'static, Held> {
    static HELD: OnceLock<Mutex<Held>> = OnceLock::new();
    HELD.get_or_init(Mutex::default).lock().unwrap_or_else(|p| p.into_inner())
}

const KEYWORDS: &[&str] = &[
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "continue", "default", "do", "double", "else", "enum", "extends", "final", "finally", "float", "for", "if", "implements", "import", "instanceof", "int", "interface", "long", "new", "null", "package", "private", "protected", "public", "return", "short", "static", "super", "switch", "synchronized", "this", "throw", "throws", "try", "void", "volatile", "while", "true", "false", "var",
];

/// Tokens of a line: identifiers, numbers, a string or char literal as one, and single punctuation characters.
pub(crate) fn tokens(line: &str) -> Vec<&str> {
    let b = line.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c.is_ascii_alphabetic() || c == b'_' || c == b'$' {
            let s = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$') {
                i += 1;
            }
            out.push(&line[s..i]);
        } else if c.is_ascii_digit() {
            let s = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') {
                i += 1;
            }
            out.push(&line[s..i]);
        } else if c == b'"' || c == b'\'' {
            let s = i;
            i += 1;
            while i < b.len() && b[i] != c {
                i += if b[i] == b'\\' { 2 } else { 1 };
            }
            i = (i + 1).min(b.len());
            out.push(&line[s..i]);
        } else {
            // a run of non-ascii bytes is one token; ascii punctuation is one character
            let s = i;
            i += 1;
            while i < b.len() && !b[i].is_ascii() {
                i += 1;
            }
            out.push(&line[s..i]);
        }
    }
    out
}

fn hash_ctx<'a>(toks: impl Iterator<Item = &'a str>) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for t in toks {
        for b in t.bytes().chain(std::iter::once(1u8)) {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

/// Whether a caret may land at byte `i` of a line: the start of a word after a space, an opening parenthesis, a dot or a comma.
fn is_word_start(line: &str, i: usize) -> bool {
    let b = line.as_bytes();
    i > 0 && i < b.len() && line.is_char_boundary(i) && matches!(b[i - 1], b' ' | b'(' | b'.' | b',') && b[i] != b' '
}

fn counted(line: &str) -> bool {
    let t = line.trim();
    (MIN_LINE..=MAX_LINE).contains(&t.len())
        && !t.starts_with("import ")
        && !t.starts_with("package ")
        && !t.starts_with("//")
        && !t.starts_with('*')
        && !t.starts_with("/*")
}

/// The caret positions of `line` and the pairs (order, context, continuation) they give.
fn cuts(line: &str) -> Vec<(usize, u64, String)> {
    let indent = line.len() - line.trim_start().len();
    let mut out = Vec::new();
    for i in indent + 8..line.len().saturating_sub(8) {
        if !is_word_start(line, i) {
            continue;
        }
        let toks = tokens(&line[..i]);
        let rest = line[i..].trim_end();
        for (k, n) in ORDERS.iter().enumerate() {
            if toks.len() >= *n {
                out.push((k, hash_ctx(toks[toks.len() - n..].iter().copied()), rest.to_string()));
            }
        }
    }
    out
}

fn bump(table: &mut HashMap<u64, Vec<(u32, u32)>>, key: u64, cont: u32) {
    let v = table.entry(key).or_default();
    match v.iter_mut().find(|(c, _)| *c == cont) {
        Some(e) => e.1 += 1,
        None => v.push((cont, 1)),
    }
}

fn unbump(table: &mut HashMap<u64, Vec<(u32, u32)>>, key: u64, cont: u32) {
    let Some(v) = table.get_mut(&key) else { return };
    if let Some(pos) = v.iter().position(|(c, _)| *c == cont) {
        v[pos].1 -= 1;
        if v[pos].1 == 0 {
            v.swap_remove(pos);
        }
    }
    if v.is_empty() {
        table.remove(&key);
    }
}

pub(crate) fn add_file(file: &str, source: &str) {
    let mut h = lock();
    remove_locked(&mut h, file);
    let mut mine: Vec<(u8, u64, u32)> = Vec::new();
    for line in source.lines().filter(|l| counted(l)) {
        if h.entries >= MAX_ENTRIES {
            break;
        }
        for (k, key, rest) in cuts(line) {
            let cont = match h.ids.get(&rest) {
                Some(&i) => i,
                None => {
                    let i = h.texts.len() as u32;
                    h.texts.push(rest.clone());
                    h.ids.insert(rest, i);
                    i
                }
            };
            bump(&mut h.tables[k], key, cont);
            h.entries += 1;
            mine.push((k as u8, key, cont));
        }
    }
    h.by_file.insert(file.to_string(), mine);
}

fn remove_locked(h: &mut Held, file: &str) {
    let Some(mine) = h.by_file.remove(file) else { return };
    for (k, key, cont) in mine {
        unbump(&mut h.tables[k as usize], key, cont);
        h.entries = h.entries.saturating_sub(1);
    }
}

pub(crate) fn remove_file(file: &str) {
    remove_locked(&mut lock(), file);
}

/// Forgets everything counted, for *Forget what Bennu learned*.
pub(crate) fn clear() {
    *lock() = Held::default();
}

/// Files read, and contexts counted, for the memory breakdown.
pub(crate) fn tracked() -> (usize, usize) {
    let h = lock();
    (h.by_file.len(), h.tables.iter().map(HashMap::len).sum())
}

pub(crate) fn sizes() -> crate::flc_memory::Sizes {
    use crate::flc_memory::{text, Sizes};
    let h = lock();
    let mut s = Sizes::default();
    for (k, v) in &h.by_file {
        s.add_file(k, v.len() * std::mem::size_of::<(u8, u64, u32)>());
    }
    s.shared = h.texts.iter().map(|t| text(t)).sum::<usize>() + h.entries * std::mem::size_of::<(u32, u32)>();
    s.shared_entries = h.entries;
    s
}

/// What followed the last words before the caret most often, when that is at least [`MIN_SHARE`] of what followed them.
pub(crate) fn suggest(line_prefix: &str) -> Option<Copied> {
    if !is_word_start(&format!("{line_prefix}x"), line_prefix.len()) || line_prefix.trim().len() < 8 {
        return None;
    }
    let toks = tokens(line_prefix);
    let h = lock();
    for (k, n) in ORDERS.iter().enumerate() {
        if toks.len() < *n {
            continue;
        }
        let Some(list) = h.tables[k].get(&hash_ctx(toks[toks.len() - n..].iter().copied())) else { continue };
        let total: u32 = list.iter().map(|x| x.1).sum();
        if total < MIN_SIGHTINGS {
            continue;
        }
        // the most frequent; the text breaks a tie, so the answer does not depend on hash order
        let &(best, count) = list.iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| h.texts[b.0 as usize].cmp(&h.texts[a.0 as usize])))?;
        let share = count as f32 / total as f32;
        let whole = (share >= MIN_SHARE).then(|| Copied { text: h.texts[best as usize].clone(), confidence: share });
        if whole.as_ref().is_some_and(|w| w.confidence >= STRONG_SHARE) {
            return whole;
        }
        // not sure of the whole line: draw the words most of the sightings agree on, however they go on
        let texts: Vec<(&str, u32)> = list.iter().map(|&(c, n)| (h.texts[c as usize].as_str(), n)).collect();
        return agreed_start(&texts, total).or(whole);
    }
    None
}

/// The start every sighting agrees on, word by word, for as long as at least [`STRONG_SHARE`] of them do: `[("findBy(x)", 3),
/// ("findAll()", 1)]` agree on `find` alone, and on `findBy` if the second were not there. Two words at least, or it is not worth drawing.
fn agreed_start(texts: &[(&str, u32)], total: u32) -> Option<Copied> {
    let spans: Vec<(Vec<(usize, usize)>, u32)> = texts
        .iter()
        .map(|&(t, n)| (tokens(t).iter().map(|w| (w.as_ptr() as usize - t.as_ptr() as usize, w.as_ptr() as usize - t.as_ptr() as usize + w.len())).collect(), n))
        .collect();
    let mut alive: Vec<usize> = (0..texts.len()).collect();
    let (mut agreed, mut share, mut end) = (0, 1.0f32, 0);
    let mut reference = 0;
    for k in 0.. {
        let mut votes: HashMap<&str, u32> = HashMap::new();
        for &i in &alive {
            if let Some(&(a, b)) = spans[i].0.get(k) {
                *votes.entry(&texts[i].0[a..b]).or_default() += spans[i].1;
            }
        }
        let Some((&word, &mass)) = votes.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0))) else { break };
        let now = mass as f32 / total as f32;
        if now < STRONG_SHARE {
            break;
        }
        alive.retain(|&i| spans[i].0.get(k).is_some_and(|&(a, b)| &texts[i].0[a..b] == word));
        reference = alive[0];
        (agreed, share, end) = (k + 1, now, spans[reference].0[k].1);
    }
    (agreed >= 2).then(|| Copied { text: texts[reference].0[..end].to_string(), confidence: share })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// The table is process-wide, so the tests that fill it take turns.
    static TURN: StdMutex<()> = StdMutex::new(());

    const LINE: &str = "        return orderRepository.findByCustomerId(customerId, pageable);";

    fn fill(files: usize) {
        clear();
        for i in 0..files {
            add_file(&format!("/p/F{i}.java"), &format!("class F{i} {{\n    int a{i};\n{LINE}\n}}\n"));
        }
    }

    #[test]
    fn what_followed_the_same_words_most_often_is_copied_with_its_share() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        fill(3);
        let c = suggest("        return orderRepository.").expect("seen three times");
        assert_eq!(c.text, "findByCustomerId(customerId, pageable);");
        assert!((c.confidence - 1.0).abs() < 1e-6);
        // five tokens of context: the call's name and its opening parenthesis are in it
        let args = suggest("        return orderRepository.findByCustomerId(").expect("seen three times");
        assert_eq!(args.text, "customerId, pageable);");
    }

    #[test]
    fn once_is_not_a_habit_and_a_split_vote_below_half_is_nothing() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        fill(1);
        assert!(suggest("        return orderRepository.").is_none(), "one sighting");
        clear();
        for (i, tail) in ["findAll(pageable);", "findOne(id);", "count();", "exists(id);"].iter().enumerate() {
            add_file(&format!("/p/G{i}.java"), &format!("class G{i} {{\n        return orderRepository.{tail} // aaaaaaaaaaaa\n}}\n"));
        }
        assert!(suggest("        return orderRepository.").is_none(), "four ways, none half");
    }

    #[test]
    fn a_file_taken_out_takes_its_counts_with_it() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        fill(3);
        remove_file("/p/F0.java");
        remove_file("/p/F1.java");
        assert!(suggest("        return orderRepository.").is_none());
        clear();
        assert_eq!(tracked(), (0, 0));
    }

    #[test]
    fn a_caret_in_the_middle_of_a_word_or_a_short_line_gets_nothing() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        fill(3);
        assert!(suggest("        return orderRepository.find").is_none(), "mid-word");
        assert!(suggest("a.").is_none(), "too little before the caret");
        assert!(suggest("").is_none());
    }

    #[test]
    fn tokens_keep_a_string_whole_and_never_split_a_character() {
        assert_eq!(tokens("log.info(\"a b\", x);"), ["log", ".", "info", "(", "\"a b\"", ",", "x", ")", ";"]);
        assert_eq!(tokens("größe = 1;"), ["gr", "öß", "e", "=", "1", ";"]);
        let _ = cuts("        String größe = naïve(\"ü\", größe);");
    }
    #[test]
    fn when_the_whole_line_is_not_sure_the_words_everyone_agrees_on_are_drawn() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        clear();
        // four ways of ending the same start: no ending reaches half, the start does
        let ends = ["(customerId);", "(name);", "(id, pageable);", "(code, status);", "(customerId);"];
        for (i, tail) in ends.iter().enumerate() {
            add_file(&format!("/p/H{i}.java"), &format!("class H{i} {{\n        return orderRepository.findBy{tail} // aaaaaaaaaaaa\n}}\n"));
        }
        let c = suggest("        return orderRepository.").expect("the start is agreed");
        assert!(c.text.starts_with("findBy"), "{:?}", c.text);
        assert!(!c.text.contains("(customerId"), "the ending is not agreed: {:?}", c.text);
        assert!(c.confidence >= STRONG_SHARE);
    }

    #[test]
    fn a_start_of_one_word_is_not_worth_drawing() {
        let texts = [("findByA(x);", 2), ("findAll();", 2), ("count();", 1)];
        assert!(agreed_start(&texts, 5).is_none());
        let longer = [("findBy(a);", 2), ("findBy(b);", 2)];
        let c = agreed_start(&longer, 4).expect("two words agreed");
        assert_eq!(c.text, "findBy(");
    }
}
