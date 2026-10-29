//! The recogniser's `initial_prompt`: a short glossary of the identifiers the speaker is likely
//! to say, so "get user by id" comes out as `getUserById` and the project name is spelled right.
//!
//! Product-agnostic: the caller (Bennu) gathers the names from its index; this only orders,
//! de-duplicates and trims them to a budget (Whisper reads ~224 prompt tokens; identifiers
//! tokenise densely, so the budget is in characters and deliberately conservative).

/// Names the caller already collected, most relevant group first.
#[derive(Debug, Default, Clone, serde::Deserialize)]
pub struct VocabInput {
    pub project: String,
    /// Identifiers of the file / class being edited.
    pub active: Vec<String>,
    /// Neighbours: callers, callees, siblings.
    pub neighbours: Vec<String>,
    /// Recently used symbols.
    pub recent: Vec<String>,
}

const MAX_CHARS: usize = 600;
const MIN_LEN: usize = 3;

fn glossary_label(language: &str) -> &'static str {
    match language {
        "it" => "Glossario tecnico:",
        "en" => "Technical glossary:",
        _ => "Glossary:",
    }
}

fn is_identifier_like(s: &str) -> bool {
    s.chars().count() >= MIN_LEN
        && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        && s.chars().any(|c| c.is_alphabetic())
}

/// Build the prompt; empty when there is nothing useful to say.
pub fn build_initial_prompt(input: &VocabInput, language: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    let ordered = std::iter::once(&input.project)
        .chain(&input.active)
        .chain(&input.neighbours)
        .chain(&input.recent)
        .map(|s| s.trim())
        .filter(|s| is_identifier_like(s) || (!s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || " -_".contains(c)) && s.len() >= MIN_LEN))
        .filter(|s| seen.insert(s.to_lowercase()));

    let label = glossary_label(language);
    let mut out = String::from(label);
    let mut added = 0;
    for name in ordered {
        if out.chars().count() + name.chars().count() + 2 > MAX_CHARS {
            break;
        }
        out.push_str(if added == 0 { " " } else { ", " });
        out.push_str(name);
        added += 1;
    }
    if added == 0 {
        return String::new();
    }
    out.push('.');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> VocabInput {
        VocabInput {
            project: "payments-api".into(),
            active: vec!["OrderService".into(), "calculateTotal".into()],
            neighbours: vec!["orderservice".into(), "TaxPolicy".into()],
            recent: vec!["x".into(), "retry_count".into()],
        }
    }

    #[test]
    fn orders_dedupes_and_filters() {
        let p = build_initial_prompt(&input(), "it");
        assert_eq!(p, "Glossario tecnico: payments-api, OrderService, calculateTotal, TaxPolicy, retry_count.");
    }

    #[test]
    fn language_selects_the_label() {
        assert!(build_initial_prompt(&input(), "en").starts_with("Technical glossary:"));
        assert!(build_initial_prompt(&input(), "auto").starts_with("Glossary:"));
    }

    #[test]
    fn stays_within_the_budget() {
        let many = VocabInput { active: (0..500).map(|i| format!("Symbol{i}Name")).collect(), ..Default::default() };
        assert!(build_initial_prompt(&many, "en").chars().count() <= MAX_CHARS + 1);
    }

    #[test]
    fn nothing_to_say_is_empty() {
        assert!(build_initial_prompt(&VocabInput::default(), "it").is_empty());
    }
}
