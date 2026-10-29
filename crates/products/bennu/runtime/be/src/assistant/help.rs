//! `bennu_help` — the assistant's way of knowing how Bennu itself works.
//!
//! A model has never heard of Bennu: asked how to rename a symbol or where the problems list is, it either
//! invents a menu that does not exist or describes some other editor. Bennu's own documentation is the cure,
//! but it is far too long to send with every request. So the documentation is indexed (see
//! `scripts/build-help-index.mjs`, which cuts it into sections), and this is the search the assistant calls
//! when someone asks how to do something: the few sections that match, with where they are from, and
//! nothing when nothing matches — which the instructions tell it to say rather than guess around.
//!
//! The search is deliberately plain: words in the query against the words of a section, a title counting for
//! more than the body. A corpus of a few hundred sections written by one person in one vocabulary does not
//! need more, and a ranking that can be read is one that can be fixed.

use std::sync::LazyLock;

use serde::Deserialize;

/// The index, built from the documentation pages and embedded in the binary.
const INDEX_JSON: &str = include_str!("../../../../assistant/help.json");

/// How much of a section is returned. Long sections are cut, with a note, so one answer stays small.
const SECTION_CHARS: usize = 1500;
/// The most sections one search returns.
const MAX_RESULTS: usize = 6;

#[derive(Debug, Deserialize)]
pub(super) struct Section {
    pub page: String,
    pub title: String,
    pub text: String,
}

static INDEX: LazyLock<Vec<Section>> = LazyLock::new(|| serde_json::from_str(INDEX_JSON).unwrap_or_default());

/// Words too common to say anything about what is being asked, in the two languages the docs and the users
/// write in.
const STOP: &[&str] = &[
    "the", "and", "for", "how", "can", "does", "with", "what", "where", "from", "that", "this", "you", "your", "are", "bennu",
    "come", "che", "per", "con", "una", "uno", "del", "della", "dei", "nel", "nella", "sono", "posso", "fare", "dove", "cosa",
    "do", "is", "it", "to", "in", "of", "on", "at", "be", "an", "or", "as", "if", "di", "da", "il", "la", "le", "lo", "un", "si",
];

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 2 && !STOP.contains(w))
        .map(str::to_string)
        .collect()
}

/// How well `section` answers `terms`: a word in the title counts three, in the page name two, in the body
/// once per occurrence up to three — so a section that is about the thing beats one that mentions it.
fn score(section: &Section, terms: &[String]) -> u32 {
    let title = section.title.to_lowercase();
    let page = section.page.to_lowercase();
    let body = section.text.to_lowercase();
    let mut total = 0u32;
    for t in terms {
        if title.contains(t.as_str()) {
            total += 3;
        }
        if page.contains(t.as_str()) {
            total += 2;
        }
        total += (body.matches(t.as_str()).count() as u32).min(3);
    }
    // Every term present somewhere is worth more than one term many times.
    let covered = terms.iter().filter(|t| title.contains(t.as_str()) || body.contains(t.as_str())).count() as u32;
    total + covered * 2
}

fn clip(text: &str) -> String {
    if text.chars().count() <= SECTION_CHARS {
        return text.to_string();
    }
    let cut: String = text.chars().take(SECTION_CHARS).collect();
    format!("{cut}\n… (section continues; ask again with more specific words for the rest)")
}

/// Search `sections` for `query`. `Err` for an empty query; otherwise the best sections, or a line saying none matched.
pub(super) fn search_in(sections: &[Section], query: &str, limit: usize) -> Result<String, String> {
    let terms = words(query);
    if terms.is_empty() {
        return Err("give some words to look for, e.g. \"rename a symbol\" or \"run configuration\"".into());
    }
    let mut scored: Vec<(u32, &Section)> = sections.iter().map(|s| (score(s, &terms), s)).filter(|(n, _)| *n > 0).collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.truncate(limit.clamp(1, MAX_RESULTS));
    if scored.is_empty() {
        return Ok("Nothing in Bennu's documentation matches. Say so; do not guess at how Bennu does it.".into());
    }
    let parts: Vec<String> = scored
        .iter()
        .map(|(_, s)| format!("## {} › {}\n{}", s.page, s.title, clip(&s.text)))
        .collect();
    Ok(parts.join("\n\n"))
}

/// Search Bennu's documentation.
pub(super) fn search(query: &str, limit: usize) -> Result<String, String> {
    search_in(&INDEX, query, limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(page: &str, title: &str, text: &str) -> Section {
        Section { page: page.into(), title: title.into(), text: text.into() }
    }

    fn corpus() -> Vec<Section> {
        vec![
            s("Refactoring", "Rename a symbol", "Press Shift+F6 on a name. Every reference is updated."),
            s("Navigation", "Go to a class", "Press Ctrl+N and type part of a name. Rename is not covered here."),
            s("Running", "Run configurations", "Edit configurations lets you set arguments and VM flags."),
        ]
    }

    #[test]
    fn a_section_about_the_thing_beats_one_that_mentions_it() {
        let out = search_in(&corpus(), "how do I rename a symbol", 3).unwrap();
        assert!(out.starts_with("## Refactoring › Rename a symbol"), "{out}");
    }

    #[test]
    fn it_finds_by_a_word_in_the_body_too() {
        let out = search_in(&corpus(), "VM flags", 3).unwrap();
        assert!(out.contains("Running › Run configurations"));
    }

    #[test]
    fn nothing_matching_says_so_and_tells_the_assistant_not_to_invent() {
        let out = search_in(&corpus(), "quantum teleportation", 3).unwrap();
        assert!(out.contains("do not guess"));
    }

    #[test]
    fn a_query_of_only_common_words_is_refused() {
        assert!(search_in(&corpus(), "how can I do that", 3).is_err());
        assert!(search_in(&corpus(), "", 3).is_err());
    }

    #[test]
    fn the_number_of_results_is_bounded_and_long_sections_are_cut() {
        let many: Vec<Section> = (0..20).map(|i| s("P", &format!("rename {i}"), &"rename ".repeat(1000))).collect();
        let out = search_in(&many, "rename", 99).unwrap();
        assert_eq!(out.matches("## P ›").count(), MAX_RESULTS);
        assert!(out.contains("section continues"));
    }

    #[test]
    fn the_embedded_index_is_there_and_answers_a_real_question() {
        assert!(INDEX.len() > 100, "the help index is empty — run scripts/build-help-index.mjs");
        let out = search("Go to class file symbol", 3).unwrap();
        assert!(out.contains("Go to class, file, symbol"), "{out}");
    }
}
