//! Phoneme-string parser — faithful 1:1 port of `discordier/sam-js`
//! (`src/parser/*.es6`). Turns a phoneme/stress string such as `"DHAX KAET"`
//! into a list of `(phoneme, length, stress)` tuples the renderer consumes.
//!
//! The original keeps three parallel growable arrays (index / length / stress)
//! and mutates them in place across several rewrite passes; [`Phonemes`] models
//! that, with `get`/`set`/`insert` mirroring the JS callbacks. END is `None`
//! (JS returned `null` for an out-of-range read).

use super::parse_tables::*;

/// The three parallel arrays SAM threads through every pass.
struct Phonemes {
    index: Vec<i32>,
    length: Vec<i32>,
    stress: Vec<i32>,
}

impl Phonemes {
    fn new() -> Self {
        Phonemes { index: Vec::new(), length: Vec::new(), stress: Vec::new() }
    }

    fn len(&self) -> i32 {
        self.index.len() as i32
    }

    /// `getPhoneme`: `None` for any out-of-range position (JS `null`/`undefined`).
    fn get(&self, pos: i32) -> Option<i32> {
        if pos < 0 || pos >= self.len() {
            None
        } else {
            Some(self.index[pos as usize])
        }
    }

    fn set(&mut self, pos: i32, value: i32) {
        if pos >= 0 && pos < self.len() {
            self.index[pos as usize] = value;
        }
    }

    /// `insertPhoneme`: grow all three arrays by one at `pos`.
    fn insert(&mut self, pos: i32, value: i32, stress: i32, length: i32) {
        let p = pos.clamp(0, self.len()) as usize;
        self.index.insert(p, value);
        self.length.insert(p, length);
        self.stress.insert(p, stress);
    }

    fn get_stress(&self, pos: i32) -> i32 {
        if pos < 0 || pos >= self.len() { 0 } else { self.stress[pos as usize] }
    }

    fn set_stress(&mut self, pos: i32, v: i32) {
        if pos >= 0 && pos < self.len() {
            self.stress[pos as usize] = v;
        }
    }

    fn get_length(&self, pos: i32) -> i32 {
        if pos < 0 || pos >= self.len() { 0 } else { self.length[pos as usize] }
    }

    fn set_length(&mut self, pos: i32, v: i32) {
        if pos >= 0 && pos < self.len() {
            self.length[pos as usize] = v;
        }
    }

    fn push(&mut self, value: i32) {
        self.index.push(value);
        self.length.push(0);
        self.stress.push(0);
    }
}

/// Shorthand: does an in-range phoneme carry `flag`.
#[inline]
fn hf(p: i32, flag: u16) -> bool {
    phoneme_has_flag(Some(p), flag)
}

// ── Parser1: phoneme string → index/stress arrays ────────────────────────────

/// Find a two-character phoneme (whose second char is not `*`).
fn full_match(s: &str) -> Option<i32> {
    PHONEME_NAME_TABLE
        .iter()
        .position(|name| *name == s && name.as_bytes().get(1) != Some(&b'*'))
        .map(|i| i as i32)
}

/// Find a single-character phoneme (`<c>*`).
fn single_match(c: char) -> Option<i32> {
    let mut buf = [0u8; 4];
    let target = {
        let cs = c.encode_utf8(&mut buf);
        format!("{cs}*")
    };
    PHONEME_NAME_TABLE
        .iter()
        .position(|name| *name == target)
        .map(|i| i as i32)
}

/// Parse the phoneme/stress string into the index + stress arrays.
/// Returns `Err` on an unparseable character (matches JS `throw`).
fn parser1(input: &str, ph: &mut Phonemes) -> Result<(), ()> {
    let chars: Vec<char> = input.chars().collect();
    let mut src = 0usize;
    while src < chars.len() {
        let sign1 = chars[src];
        let two: String = chars[src..(src + 2).min(chars.len())].iter().collect();

        if let Some(m) = full_match(&two) {
            src += 2; // consumed both characters
            ph.push(m);
            continue;
        }
        if let Some(m) = single_match(sign1) {
            src += 1;
            ph.push(m);
            continue;
        }

        // Otherwise it must be a stress marker: scan the table from the top down.
        let mut m = STRESS_TABLE.len(); // starts out of range (JS `undefined`)
        while m > 0 && STRESS_TABLE.get(m).copied() != Some(sign1) {
            m -= 1;
        }
        if m == 0 {
            return Err(()); // could not parse char
        }
        // Set stress for the prior phoneme (the last one pushed).
        let last = ph.len() - 1;
        ph.set_stress(last, m as i32);
        src += 1;
    }
    Ok(())
}

// ── Parser2: phonetic rewrite rules ──────────────────────────────────────────

fn handle_uw_ch_j(ph: &mut Phonemes, phoneme: i32, pos: i32) {
    match phoneme {
        // 'UW': <ALVEOLAR> UW -> <ALVEOLAR> UX
        53 => {
            if phoneme_has_flag(ph.get(pos - 1), FLAG_ALVEOLAR) {
                ph.set(pos, 16); // UX
            }
        }
        // 'CH' -> 'CH' '**'(43)
        42 => {
            let s = ph.get_stress(pos);
            ph.insert(pos + 1, 43, s, 0);
        }
        // 'J*' -> 'J*' '**'(45)
        44 => {
            let s = ph.get_stress(pos);
            ph.insert(pos + 1, 45, s, 0);
        }
        _ => {}
    }
}

fn change_ax(ph: &mut Phonemes, position: i32, suffix: i32) {
    ph.set(position, 13); // 'AX'
    let s = ph.get_stress(position);
    ph.insert(position + 1, suffix, s, 0);
}

fn parser2(ph: &mut Phonemes) {
    let mut pos = -1;
    loop {
        pos += 1;
        let mut phoneme = match ph.get(pos) {
            Some(p) => p,
            None => break,
        };
        if phoneme == 0 {
            continue;
        }

        if hf(phoneme, FLAG_DIPHTHONG) {
            // Insert YX (ends in IY) or WX following the diphthong.
            let suffix = if hf(phoneme, FLAG_DIP_YX) { 21 } else { 20 };
            let s = ph.get_stress(pos);
            ph.insert(pos + 1, suffix, s, 0);
            handle_uw_ch_j(ph, phoneme, pos);
            continue;
        }
        if phoneme == 78 {
            change_ax(ph, pos, 24); // 'UL' -> 'AX' 'L*'
            continue;
        }
        if phoneme == 79 {
            change_ax(ph, pos, 27); // 'UM' -> 'AX' 'M*'
            continue;
        }
        if phoneme == 80 {
            change_ax(ph, pos, 28); // 'UN' -> 'AX' 'N*'
            continue;
        }
        if hf(phoneme, FLAG_VOWEL) && ph.get_stress(pos) != 0 {
            // <STRESSED VOWEL> <SILENCE> <STRESSED VOWEL> -> insert Q between.
            if ph.get(pos + 1) == Some(0) {
                if let Some(p2) = ph.get(pos + 2) {
                    if hf(p2, FLAG_VOWEL) && ph.get_stress(pos + 2) != 0 {
                        ph.insert(pos + 2, 31, 0, 0); // 'Q'
                    }
                }
            }
            continue;
        }

        let prior = if pos == 0 { None } else { ph.get(pos - 1) };

        if phoneme == P_R {
            // Rules for phonemes before R.
            if prior == Some(P_T) {
                ph.set(pos - 1, 42); // T* R* -> CH R*
            } else if prior == Some(P_D) {
                ph.set(pos - 1, 44); // D* R* -> J* R*
            } else if phoneme_has_flag(prior, FLAG_VOWEL) {
                ph.set(pos, 18); // <VOWEL> R* -> <VOWEL> RX
            }
            continue;
        }

        if phoneme == 24 && phoneme_has_flag(prior, FLAG_VOWEL) {
            ph.set(pos, 19); // <VOWEL> L* -> <VOWEL> LX
            continue;
        }
        if prior == Some(60) && phoneme == 32 {
            ph.set(pos, 38); // G S -> G Z
            continue;
        }
        if phoneme == 60 {
            // G <VOWEL OR DIPHTHONG NOT ENDING WITH IY> -> GX ...
            let next = ph.get(pos + 1);
            if !phoneme_has_flag(next, FLAG_DIP_YX) && next.is_some() {
                ph.set(pos, 63); // 'GX'
            }
            continue;
        }
        if phoneme == 72 {
            // K <VOWEL OR DIPHTHONG NOT ENDING WITH IY> -> KX ...
            let y = ph.get(pos + 1);
            if !phoneme_has_flag(y, FLAG_DIP_YX) || y.is_none() {
                ph.set(pos, 75);
                phoneme = 75;
            }
        }

        // Replace unvoiced stop consonants with their softer version after S*.
        if hf(phoneme, FLAG_UNVOICED_STOPCONS) && prior == Some(32) {
            ph.set(pos, phoneme - 12);
        } else if !hf(phoneme, FLAG_UNVOICED_STOPCONS) {
            handle_uw_ch_j(ph, phoneme, pos);
        }

        if phoneme == 69 || phoneme == 57 {
            // Soften T/D following an unstressed vowel and preceding a pause.
            if pos > 0 && phoneme_has_flag(ph.get(pos - 1), FLAG_VOWEL) {
                let mut next = ph.get(pos + 1);
                if next == Some(0) {
                    next = ph.get(pos + 2);
                }
                if phoneme_has_flag(next, FLAG_VOWEL) && ph.get_stress(pos + 1) == 0 {
                    ph.set(pos, 30); // 'DX'
                }
            }
            continue;
        }
    }
}

// ── CopyStress ───────────────────────────────────────────────────────────────

fn copy_stress(ph: &mut Phonemes) {
    let mut position = 0;
    while let Some(phoneme) = ph.get(position) {
        if hf(phoneme, FLAG_CONSONANT) {
            if let Some(next) = ph.get(position + 1) {
                if hf(next, FLAG_VOWEL) {
                    let stress = ph.get_stress(position + 1);
                    if stress != 0 && stress < 0x80 {
                        ph.set_stress(position, stress + 1);
                    }
                }
            }
        }
        position += 1;
    }
}

// ── SetPhonemeLength ─────────────────────────────────────────────────────────

fn set_phoneme_length(ph: &mut Phonemes) {
    let mut position = 0;
    while let Some(phoneme) = ph.get(position) {
        let stress = ph.get_stress(position);
        let combined = COMBINED_PHONEME_LENGTH_TABLE[phoneme as usize];
        if stress == 0 || stress > 0x7F {
            ph.set_length(position, (combined & 0xFF) as i32);
        } else {
            ph.set_length(position, (combined >> 8) as i32);
        }
        position += 1;
    }
}

// ── AdjustLengths ────────────────────────────────────────────────────────────

fn adjust_lengths(ph: &mut Phonemes) {
    // Part 1: lengthen non-fricative/voiced phonemes between a vowel and
    // following punctuation by 1.5.
    let mut position = 0i32;
    while let Some(cur) = ph.get(position) {
        if !hf(cur, FLAG_PUNCT) {
            position += 1;
            continue;
        }
        let loop_index = position;
        // Back up to the first vowel (while `--position > 1`).
        loop {
            position -= 1;
            if position <= 1 {
                break;
            }
            if phoneme_has_flag(ph.get(position), FLAG_VOWEL) {
                break;
            }
        }
        if position == 0 {
            break;
        }
        while position < loop_index {
            let p = ph.get(position);
            if !phoneme_has_flag(p, FLAG_FRICATIVE) || phoneme_has_flag(p, FLAG_VOICED) {
                let a = ph.get_length(position);
                ph.set_length(position, (a >> 1) + a + 1);
            }
            position += 1;
        }
        position += 1; // the outer for-loop's increment
    }

    // Part 2: shorten vowels / set nasal+stop / shorten stop pairs / liquids.
    let mut loop_index = -1i32;
    loop {
        loop_index += 1;
        let phoneme = match ph.get(loop_index) {
            Some(p) => p,
            None => break,
        };
        let mut position = loop_index;

        if hf(phoneme, FLAG_VOWEL) {
            position += 1;
            let next = ph.get(position);
            if !phoneme_has_flag(next, FLAG_CONSONANT) {
                // RX/LX followed by a consonant: shorten the vowel by 1.
                if (next == Some(18) || next == Some(19))
                    && phoneme_has_flag(ph.get(position + 1), FLAG_CONSONANT)
                {
                    ph.set_length(loop_index, ph.get_length(loop_index) - 1);
                }
                continue;
            }
            // `next` is a consonant (so it is Some).
            let flags = PHONEME_FLAGS[next.unwrap() as usize];
            if (flags & FLAG_VOICED) == 0 {
                // <VOWEL> <UNVOICED PLOSIVE> -> decrease vowel by 1/8.
                if (flags & FLAG_UNVOICED_STOPCONS) != 0 {
                    let a = ph.get_length(loop_index);
                    ph.set_length(loop_index, a - (a >> 3));
                }
                continue;
            }
            // <VOWEL> <VOICED CONSONANT> -> increase vowel by 1/4 + 1.
            let a = ph.get_length(loop_index);
            ph.set_length(loop_index, (a >> 2) + a + 1);
            continue;
        }

        if hf(phoneme, FLAG_NASAL) {
            // <NASAL> <STOP CONSONANT> -> nasal = 5, consonant = 6.
            position += 1;
            if let Some(p2) = ph.get(position) {
                if hf(p2, FLAG_STOPCONS) {
                    ph.set_length(position, 6);
                    ph.set_length(position - 1, 5);
                }
            }
            continue;
        }

        if hf(phoneme, FLAG_STOPCONS) {
            // <STOP CONSONANT> {optional silence} <STOP CONSONANT> -> halve both.
            let p;
            loop {
                position += 1;
                let cur = ph.get(position);
                if cur != Some(0) {
                    p = cur;
                    break;
                }
            }
            if let Some(pp) = p {
                if hf(pp, FLAG_STOPCONS) {
                    ph.set_length(position, (ph.get_length(position) >> 1) + 1);
                    ph.set_length(loop_index, (ph.get_length(loop_index) >> 1) + 1);
                }
            }
            continue;
        }

        if position > 0
            && hf(phoneme, FLAG_LIQUIC)
            && phoneme_has_flag(ph.get(position - 1), FLAG_STOPCONS)
        {
            // <STOP CONSONANT> <LIQUID> -> decrease liquid by 2.
            ph.set_length(position, ph.get_length(position) - 2);
        }
    }
}

// ── ProlongPlosiveStopConsonants ─────────────────────────────────────────────

fn prolong_plosive_stop_consonants(ph: &mut Phonemes) {
    let mut pos = -1i32;
    loop {
        pos += 1;
        let index = match ph.get(pos) {
            Some(p) => p,
            None => break,
        };
        if !hf(index, FLAG_STOPCONS) {
            continue;
        }
        if hf(index, FLAG_UNVOICED_STOPCONS) {
            // Move to the next non-empty phoneme and validate flags.
            let mut x = pos;
            let next_non_empty;
            loop {
                x += 1;
                let cur = ph.get(x);
                if cur != Some(0) {
                    next_non_empty = cur;
                    break;
                }
            }
            if let Some(nn) = next_non_empty {
                if phoneme_has_flag(Some(nn), FLAG_0008) || nn == 36 || nn == 37 {
                    continue;
                }
            }
        }
        let l1 = (COMBINED_PHONEME_LENGTH_TABLE[(index + 1) as usize] & 0xFF) as i32;
        let l2 = (COMBINED_PHONEME_LENGTH_TABLE[(index + 2) as usize] & 0xFF) as i32;
        let s = ph.get_stress(pos);
        ph.insert(pos + 1, index + 1, s, l1);
        ph.insert(pos + 2, index + 2, s, l2);
        pos += 2;
    }
}

/// One parsed phoneme: `(phoneme index, length in frames, stress)`.
pub type PhonemeTuple = (i32, i32, i32);

/// Parse a phoneme/stress string into render tuples. Returns `None` on empty or
/// unparseable input (matches JS `Parser` returning `false`). Pause phonemes
/// (index 0) are dropped, exactly as the JS port does.
pub fn parse(input: &str) -> Option<Vec<PhonemeTuple>> {
    if input.is_empty() {
        return None;
    }
    let mut ph = Phonemes::new();
    if parser1(input, &mut ph).is_err() {
        return None;
    }
    parser2(&mut ph);
    copy_stress(&mut ph);
    set_phoneme_length(&mut ph);
    adjust_lengths(&mut ph);
    prolong_plosive_stop_consonants(&mut ph);

    let tuples: Vec<PhonemeTuple> = (0..ph.index.len())
        .filter(|&i| ph.index[i] != 0)
        .map(|i| (ph.index[i], ph.length[i], ph.stress[i]))
        .collect();
    Some(tuples)
}
