//! Byte-level BPE for source code, written here so training data and inference share one
//! implementation (the way `header::build` is shared): the Python trainer only ever sees ids.
//!
//! Ids: `0..256` are raw bytes (0 doubles as the file separator), then four specials, then one id
//! per learned merge. Merges never cross a *chunk* boundary, and the chunker is built around code:
//! a newline is always a token of its own, camelCase words split at their humps, a lone space
//! sticks to the word after it.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

pub const PRE: u16 = 256;
pub const SUF: u16 = 257;
pub const MID: u16 = 258;
/// Ends the generated middle (the rest of the line).
pub const EOT: u16 = 259;
const FIRST_MERGE: u16 = 260;
pub const NEWLINE: u16 = b'\n' as u16;

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Lower,
    Upper,
    Digit,
    Under,
    Space,
    Newline,
    Punct,
}

fn class(b: u8) -> Class {
    match b {
        b'a'..=b'z' | 128..=255 => Class::Lower,
        b'A'..=b'Z' => Class::Upper,
        b'0'..=b'9' => Class::Digit,
        b'_' | b'$' => Class::Under,
        b' ' | b'\t' | b'\r' => Class::Space,
        b'\n' => Class::Newline,
        _ => Class::Punct,
    }
}

fn run(t: &[u8], mut i: usize, c: Class) -> usize {
    while i < t.len() && class(t[i]) == c {
        i += 1;
    }
    i
}

/// End of the word starting at `i`: a lowercase run, a digit run, `Capitalized`, or an acronym
/// that stops before the capital starting the next word (`HTTPServer` → `HTTP` `Server`).
fn word_end(t: &[u8], i: usize) -> usize {
    match class(t[i]) {
        Class::Upper => {
            let j = run(t, i, Class::Upper);
            match (j < t.len() && class(t[j]) == Class::Lower, j - i) {
                (true, 1) => run(t, j, Class::Lower),
                (true, _) => j - 1,
                _ => j,
            }
        }
        Class::Under => run(t, i, Class::Under),
        c => run(t, i, c),
    }
}

/// Splits `t` into the chunks BPE merges inside.
pub fn chunks(t: &[u8]) -> Vec<(usize, usize)> {
    let (mut out, mut i) = (Vec::new(), 0);
    while i < t.len() {
        let start = i;
        match class(t[i]) {
            Class::Newline => i += 1,
            Class::Space => {
                i = run(t, i, Class::Space);
                let at_line_start = start == 0 || t[start - 1] == b'\n';
                if i - start == 1 && !at_line_start && i < t.len() && matches!(class(t[i]), Class::Lower | Class::Upper | Class::Digit) {
                    i = word_end(t, i);
                }
            }
            Class::Punct => {
                while i < t.len() && class(t[i]) == Class::Punct && i - start < 8 {
                    i += 1;
                }
            }
            _ => i = word_end(t, i),
        }
        out.push((start, i));
    }
    out
}

pub struct Tokenizer {
    merges: Vec<(u16, u16)>,
    /// pair → (rank, merged id)
    ranks: HashMap<(u16, u16), (u32, u16)>,
    bytes: Vec<Vec<u8>>,
    cache: RefCell<HashMap<Vec<u8>, Vec<u16>>>,
}

fn merge_word(w: &[u16], pair: (u16, u16), to: u16) -> Vec<u16> {
    let mut out = Vec::with_capacity(w.len());
    let mut i = 0;
    while i < w.len() {
        if i + 1 < w.len() && (w[i], w[i + 1]) == pair {
            out.push(to);
            i += 2;
        } else {
            out.push(w[i]);
            i += 1;
        }
    }
    out
}

impl Tokenizer {
    fn from_merges(merges: Vec<(u16, u16)>) -> Self {
        let mut bytes: Vec<Vec<u8>> = (0..=255u8).map(|b| vec![b]).collect();
        bytes.extend((0..4).map(|_| Vec::new()));
        let mut ranks = HashMap::new();
        for (r, &(a, b)) in merges.iter().enumerate() {
            let id = FIRST_MERGE + r as u16;
            let mut joined = bytes[a as usize].clone();
            joined.extend_from_slice(&bytes[b as usize]);
            bytes.push(joined);
            ranks.insert((a, b), (r as u32, id));
        }
        Self { merges, ranks, bytes, cache: RefCell::new(HashMap::new()) }
    }

    /// An independent copy (own chunk cache), for worker threads.
    pub fn fork(&self) -> Self {
        Self::from_merges(self.merges.clone())
    }

    pub fn vocab_size(&self) -> usize {
        FIRST_MERGE as usize + self.merges.len()
    }

    pub fn token_bytes(&self, id: u16) -> &[u8] {
        &self.bytes[id as usize]
    }

    fn encode_chunk(&self, chunk: &[u8]) -> Vec<u16> {
        if let Some(hit) = self.cache.borrow().get(chunk) {
            return hit.clone();
        }
        let mut ids: Vec<u16> = chunk.iter().map(|&b| b as u16).collect();
        loop {
            let best = ids.windows(2).filter_map(|p| self.ranks.get(&(p[0], p[1])).map(|&(r, id)| (r, (p[0], p[1]), id))).min_by_key(|x| x.0);
            let Some((_, pair, id)) = best else { break };
            ids = merge_word(&ids, pair, id);
        }
        self.cache.borrow_mut().insert(chunk.to_vec(), ids.clone());
        ids
    }

    pub fn encode(&self, text: &str) -> Vec<u16> {
        let b = text.as_bytes();
        chunks(b).into_iter().flat_map(|(s, e)| self.encode_chunk(&b[s..e])).collect()
    }

    pub fn decode(&self, ids: &[u16]) -> Vec<u8> {
        ids.iter().flat_map(|&i| self.bytes[i as usize].iter().copied()).collect()
    }

    pub fn save(&self, path: &str) -> Result<(), String> {
        let body: String = self.merges.iter().map(|(a, b)| format!("{a} {b}\n")).collect();
        std::fs::write(path, body).map_err(|e| e.to_string())
    }

    pub fn load(path: &str) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let merges = text
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| {
                let mut it = l.split(' ').map(|n| n.parse::<u16>());
                match (it.next(), it.next()) {
                    (Some(Ok(a)), Some(Ok(b))) => Ok((a, b)),
                    _ => Err(format!("bad merge line {l:?}")),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self::from_merges(merges))
    }

    /// Learns `n_merges` merges from chunk frequencies. Ties break toward the smaller pair so the
    /// same corpus always gives the same vocabulary.
    pub fn train(counts: HashMap<Vec<u8>, u64>, n_merges: usize) -> Self {
        let mut words: Vec<(Vec<u16>, i64)> = counts.into_iter().map(|(w, c)| (w.iter().map(|&b| b as u16).collect(), c as i64)).collect();
        words.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        words.truncate(300_000);
        let mut pair_count: HashMap<(u16, u16), i64> = HashMap::new();
        let mut where_: HashMap<(u16, u16), HashSet<usize>> = HashMap::new();
        for (wi, (w, c)) in words.iter().enumerate() {
            for p in w.windows(2) {
                *pair_count.entry((p[0], p[1])).or_insert(0) += c;
                where_.entry((p[0], p[1])).or_default().insert(wi);
            }
        }
        let mut merges = Vec::new();
        for m in 0..n_merges {
            let Some((&best, &cnt)) = pair_count.iter().filter(|(_, &c)| c > 0).max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0))) else { break };
            if cnt < 2 {
                break;
            }
            let id = FIRST_MERGE + m as u16;
            merges.push(best);
            pair_count.remove(&best);
            for wi in where_.remove(&best).unwrap_or_default() {
                let c = words[wi].1;
                for p in words[wi].0.windows(2) {
                    if (p[0], p[1]) != best {
                        *pair_count.entry((p[0], p[1])).or_insert(0) -= c;
                    }
                }
                let merged = merge_word(&words[wi].0, best, id);
                for p in merged.windows(2) {
                    *pair_count.entry((p[0], p[1])).or_insert(0) += c;
                    where_.entry((p[0], p[1])).or_default().insert(wi);
                }
                words[wi].0 = merged;
            }
        }
        Self::from_merges(merges)
    }
}

/// Chunk frequencies of `text`, to be summed across files before `train`.
pub fn count_chunks(text: &str, into: &mut HashMap<Vec<u8>, u64>) {
    let b = text.as_bytes();
    for (s, e) in chunks(b) {
        *into.entry(b[s..e].to_vec()).or_insert(0) += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pieces(t: &str) -> Vec<&str> {
        chunks(t.as_bytes()).into_iter().map(|(s, e)| &t[s..e]).collect()
    }

    #[test]
    fn camel_case_and_acronyms_split_at_the_humps() {
        assert_eq!(pieces("getNomeCliente"), ["get", "Nome", "Cliente"]);
        assert_eq!(pieces("HTTPServer"), ["HTTP", "Server"]);
        assert_eq!(pieces("MAX_VALUE"), ["MAX", "_", "VALUE"]);
    }

    #[test]
    fn newline_is_a_chunk_of_its_own_and_a_lone_space_joins_the_next_word() {
        assert_eq!(pieces("a;\n    return x;"), ["a", ";", "\n", "    ", "return", " x", ";"]);
    }

    fn sample() -> HashMap<Vec<u8>, u64> {
        let mut counts = HashMap::new();
        for _ in 0..40 {
            count_chunks("public static final String name = value.getName();\n", &mut counts);
            count_chunks("    return this.name.equals(other.name);\n", &mut counts);
        }
        counts
    }

    #[test]
    fn training_is_deterministic_and_encoding_round_trips() {
        let a = Tokenizer::train(sample(), 60);
        let b = Tokenizer::train(sample(), 60);
        assert_eq!(a.merges, b.merges);
        let text = "public String getName() {\n    return name; // è già così\n}\n";
        let ids = a.encode(text);
        assert_eq!(a.decode(&ids), text.as_bytes());
        assert!(ids.len() < text.len(), "merges must shorten the text");
        assert_eq!(ids.iter().filter(|&&i| i == NEWLINE).count(), 3);
    }

    #[test]
    fn save_and_load_agree() {
        let a = Tokenizer::train(sample(), 60);
        let path = std::env::temp_dir().join("flc_vocab_test.txt");
        a.save(path.to_str().unwrap()).unwrap();
        let b = Tokenizer::load(path.to_str().unwrap()).unwrap();
        assert_eq!(a.encode("this.name.equals(x);"), b.encode("this.name.equals(x);"));
        assert_eq!(a.vocab_size(), b.vocab_size());
    }
}
