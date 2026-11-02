//! A name as the words it says, whatever the case it is written in.
//!
//! `jaxbProcessor`, `jaxb_processor`, `JaxbProcessor` and `JAXB_PROCESSOR` are one name in four styles, and a project (or one
//! person, or two layers of the same application that talk through JSON) will write it in more than one. What the learned tables
//! count must not depend on that: they are keyed by the words, lower-cased and joined with `_`, and the spelling is only what
//! reaches the screen.

/// The words of `name`: a break at `_`, `$`, `-`, at a lower-to-upper change (`jaxbProcessor`) and where a run of capitals ends
/// before a lower-case letter (`XMLParser` → `xml`, `parser`). Digits stay with the word they follow (`item2Name` → `item2`, `name`).
pub(crate) fn words(name: &str) -> Vec<String> {
    let c: Vec<char> = name.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut word = String::new();
    for (i, &ch) in c.iter().enumerate() {
        if matches!(ch, '_' | '$' | '-') {
            if !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
            continue;
        }
        let prev = if i > 0 { Some(c[i - 1]) } else { None };
        let next = c.get(i + 1).copied();
        let lower_to_upper = prev.is_some_and(|p| (p.is_lowercase() || p.is_ascii_digit()) && ch.is_uppercase());
        let acronym_ends = prev.is_some_and(|p| p.is_uppercase()) && ch.is_uppercase() && next.is_some_and(|n| n.is_lowercase());
        if (lower_to_upper || acronym_ends) && !word.is_empty() {
            out.push(std::mem::take(&mut word));
        }
        word.extend(ch.to_lowercase());
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// The key a name is counted under: its words, lower-cased, joined with `_`. A name with no word in it (`_`, `$`) is kept as written.
pub(crate) fn canon(name: &str) -> String {
    let w = words(name);
    if w.is_empty() { name.to_string() } else { w.join("_") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_name_in_every_style_is_one_key() {
        for style in ["jaxbProcessor", "jaxb_processor", "JaxbProcessor", "JAXB_PROCESSOR", "jaxb-processor", "_jaxbProcessor", "jaxb$processor"] {
            assert_eq!(canon(style), "jaxb_processor", "{style}");
        }
    }

    #[test]
    fn acronyms_and_digits_stay_together() {
        assert_eq!(canon("XMLParser"), "xml_parser");
        assert_eq!(canon("getHTTPResponse"), "get_http_response");
        assert_eq!(canon("item2Name"), "item2_name");
        assert_eq!(canon("sha256Hash"), "sha256_hash");
        assert_eq!(canon("bao"), "bao");
    }

    #[test]
    fn a_name_without_words_is_left_as_written() {
        assert_eq!(canon("_"), "_");
        assert_eq!(canon("$"), "$");
        assert!(words("").is_empty());
    }
}
