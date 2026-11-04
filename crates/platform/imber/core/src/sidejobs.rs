//! Which model does the work around the conversation.
//!
//! Summarising an old conversation and (when sub-agents exist) reading code to answer a question are
//! jobs where a small model is enough and the large one is a waste: the conversation itself keeps the
//! large model, the side jobs get a cheaper one. The user can name it in the assistant settings; left
//! empty, it is the cheapest Haiku-class model in the price list.
//!
//! Hook for sub-agents: a read-only exploration agent calls [`resolve_side_model`] with
//! [`SideJob::Exploration`] and uses the id it returns for its own requests; nothing else about the
//! choice belongs to the caller.

use crate::models::ModelInfo;

/// The model a side job uses when the user has not chosen one, if the price list has it.
pub const DEFAULT_SIDE_MODEL: &str = "claude-haiku-5-5";

/// The jobs that run beside the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideJob {
    /// Folding the early part of a conversation into a note.
    Compaction,
    /// A read-only sub-agent looking things up for the main one.
    Exploration,
}

fn price_sum(m: &ModelInfo) -> f64 {
    m.price.input + m.price.output
}

/// The id of the model a side job runs on. `chosen` is the user's setting (empty for "pick for me"), `table`
/// the price list, `main` the conversation's own model — the last resort, so a side job never fails for lack
/// of a cheaper one.
pub fn resolve_side_model(_job: SideJob, chosen: &str, table: &[ModelInfo], main: &str) -> String {
    let chosen = chosen.trim();
    if !chosen.is_empty() && table.iter().any(|m| m.id == chosen) {
        return chosen.to_string();
    }
    if table.iter().any(|m| m.id == DEFAULT_SIDE_MODEL) {
        return DEFAULT_SIDE_MODEL.to_string();
    }
    table
        .iter()
        .filter(|m| m.id.contains("haiku"))
        .min_by(|a, b| price_sum(a).total_cmp(&price_sum(b)))
        .map(|m| m.id.clone())
        .unwrap_or_else(|| main.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Rates;

    fn model(id: &str, input: f64, output: f64) -> ModelInfo {
        ModelInfo {
            id: id.into(),
            name: id.into(),
            tagline: String::new(),
            context_max: 200_000,
            efforts: Vec::new(),
            price: Rates { input, output, cache_read: None, cache_write_5m: None, cache_write_1h: None, over_tokens: None },
            long_prompt: None,
        }
    }

    fn table() -> Vec<ModelInfo> {
        vec![model("claude-opus-5-5", 4.0, 20.0), model("claude-haiku-4-5", 1.0, 5.0), model("claude-haiku-5-5", 0.1, 0.5)]
    }

    #[test]
    fn the_users_choice_wins_when_the_price_list_knows_it() {
        assert_eq!(resolve_side_model(SideJob::Compaction, "claude-haiku-4-5", &table(), "claude-opus-5-5"), "claude-haiku-4-5");
        assert_eq!(resolve_side_model(SideJob::Compaction, "not-a-model", &table(), "claude-opus-5-5"), DEFAULT_SIDE_MODEL, "an unknown id is not sent");
    }

    #[test]
    fn left_empty_it_is_the_default_haiku() {
        assert_eq!(resolve_side_model(SideJob::Exploration, "  ", &table(), "claude-opus-5-5"), DEFAULT_SIDE_MODEL);
    }

    #[test]
    fn without_the_default_it_is_the_cheapest_haiku_then_the_main_model() {
        let t = vec![model("claude-opus-5-5", 4.0, 20.0), model("claude-haiku-4-5", 1.0, 5.0), model("claude-haiku-3", 0.25, 1.25)];
        assert_eq!(resolve_side_model(SideJob::Compaction, "", &t, "claude-opus-5-5"), "claude-haiku-3");
        let only_big = vec![model("claude-opus-5-5", 4.0, 20.0)];
        assert_eq!(resolve_side_model(SideJob::Compaction, "", &only_big, "claude-opus-5-5"), "claude-opus-5-5");
    }
}
