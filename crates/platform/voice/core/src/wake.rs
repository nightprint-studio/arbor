//! Wake-phrase matching on a transcript.
//!
//! The recogniser hears "Ehi Imber" as "hey amber", "ei ember", "ehi imbar", sometimes glued
//! ("heyimber"). Matching therefore compares *letters only*, spaces removed, with a normalised
//! edit distance, over the first few words of the utterance only — a phrase in the middle of a
//! sentence never wakes anything.

/// Phrases used when the user has not configured their own. Variants such as "amber" or "ember"
/// are covered by the edit-distance tolerance, not listed.
pub const DEFAULT_PHRASES: &[&str] = &["ehi imber", "hey imber", "ei imber"];

/// Minimum similarity (1 − distance / length) for a match.
const THRESHOLD: f32 = 0.74;
/// The wake phrase is at most this many words longer than configured (ASR splits words).
const EXTRA_WORDS: usize = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct WakeMatch {
    pub phrase: String,
    pub similarity: f32,
    /// What was said after the phrase — the start of the request, possibly empty.
    pub remainder: String,
}

/// Lowercase letters/digits only, accents folded.
pub fn normalize(text: &str) -> String {
    text.chars()
        .flat_map(|c| c.to_lowercase())
        .filter_map(|c| match c {
            'à' | 'á' | 'â' | 'ä' => Some('a'),
            'è' | 'é' | 'ê' | 'ë' => Some('e'),
            'ì' | 'í' | 'î' | 'ï' => Some('i'),
            'ò' | 'ó' | 'ô' | 'ö' => Some('o'),
            'ù' | 'ú' | 'û' | 'ü' => Some('u'),
            c if c.is_alphanumeric() => Some(c),
            _ => None,
        })
        .collect()
}

/// Levenshtein distance over chars.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur.push(sub.min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

fn similarity(a: &str, b: &str) -> f32 {
    let longest = a.chars().count().max(b.chars().count());
    if longest == 0 {
        return 0.0;
    }
    1.0 - edit_distance(a, b) as f32 / longest as f32
}

/// Does `transcript` begin with one of `phrases`?
pub fn match_wake(transcript: &str, phrases: &[String]) -> Option<WakeMatch> {
    let words: Vec<&str> = transcript.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }
    let mut best: Option<(f32, usize, &String)> = None;
    for phrase in phrases {
        let target = normalize(phrase);
        if target.chars().count() < 4 {
            continue;
        }
        let phrase_words = phrase.split_whitespace().count().max(1);
        for k in 1..=(phrase_words + EXTRA_WORDS).min(words.len()) {
            let heard = normalize(&words[..k].join(" "));
            let sim = similarity(&heard, &target);
            if sim >= THRESHOLD && best.map_or(true, |(s, _, _)| sim > s) {
                best = Some((sim, k, phrase));
            }
        }
    }
    best.map(|(similarity, k, phrase)| WakeMatch {
        phrase: phrase.clone(),
        similarity,
        remainder: words[k..]
            .join(" ")
            .trim_start_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phrases() -> Vec<String> {
        DEFAULT_PHRASES.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn exact_phrases_match_and_keep_the_remainder() {
        let m = match_wake("Ehi Imber, scrivi un test per il parser", &phrases()).unwrap();
        assert_eq!(m.remainder, "scrivi un test per il parser");
        assert!(m.similarity > 0.95);
        assert!(match_wake("hey imber", &phrases()).unwrap().remainder.is_empty());
    }

    #[test]
    fn phonetic_variants_match() {
        for heard in ["hey amber what is this", "ehi ember", "ei imbar", "hey imbear", "heyimber do it", "Hey, Amber."] {
            assert!(match_wake(heard, &phrases()).is_some(), "{heard}");
        }
    }

    #[test]
    fn ordinary_speech_does_not_wake() {
        for heard in [
            "amber light means stop",
            "the member of the team",
            "hey there how are you",
            "ehi che bello",
            "please remember to commit",
            "imber",
            "",
        ] {
            assert!(match_wake(heard, &phrases()).is_none(), "{heard}");
        }
    }

    #[test]
    fn the_phrase_must_open_the_utterance() {
        assert!(match_wake("so I said hey imber to him", &phrases()).is_none());
    }

    #[test]
    fn custom_phrases_work_and_short_ones_are_ignored() {
        let custom = vec!["ok computer".to_string(), "x".to_string()];
        assert!(match_wake("okay computer open the file", &custom).is_some());
        assert!(match_wake("x marks", &custom).is_none());
    }

    #[test]
    fn edit_distance_basics() {
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("", "abc"), 3);
    }
}
