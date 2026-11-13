//! How many tokens one response may use.
//!
//! The provider wants a `max_tokens` on every request, and everything the model produces in that response
//! counts against it: its thinking, the text, and the arguments of the tools it calls — a whole file passed to
//! `edit_file` included. A low fixed number was the cause of replies cut off mid-argument, which is the most
//! expensive place to be cut. Output is paid for as it is written, not as it is allowed, so there is nothing to
//! gain from asking for less than the model can give: the brake on spending is the spending limits.
//!
//! What does bound it is the window: input and output share it, so a conversation that has filled most of it
//! leaves less room for the answer. [`output_budget`] is the model's ceiling, cut to what the window has left —
//! worked out before **every** request, because the conversation grows within a run.

use crate::message::Request;
use crate::tokens::{estimate, history_tokens, tools_tokens};

/// Never ask for less than this: an answer with no room to say anything is worse than a request the provider
/// refuses with a reason (which then triggers compaction).
pub const MIN_OUTPUT: u32 = 4_096;

/// The input of `request` as the provider will count it, estimated: instructions, tool definitions and history.
pub fn input_estimate(request: &Request) -> u64 {
    estimate(&request.system) + tools_tokens(&request.tools) + history_tokens(&request.messages).total()
}

/// The `max_tokens` for a request: the model's `ceiling`, less only when `window` minus the `input` (and a
/// margin for the estimate being an estimate) leaves less room than that.
pub fn output_budget(ceiling: u32, window: u64, input: u64) -> u32 {
    // The estimate is by characters; a tenth of it (at least two thousand tokens) covers how far it can be off.
    let margin = (input / 10).max(2_000);
    let room = window.saturating_sub(input + margin).min(u64::from(u32::MAX)) as u32;
    ceiling.min(room).max(MIN_OUTPUT.min(ceiling))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_roomy_window_gets_the_whole_ceiling() {
        assert_eq!(output_budget(128_000, 1_000_000, 50_000), 128_000);
    }

    #[test]
    fn a_full_window_leaves_what_is_left() {
        // 1M window, 900k of input: 900k + 90k margin leaves 10k.
        assert_eq!(output_budget(128_000, 1_000_000, 900_000), 10_000);
    }

    #[test]
    fn there_is_always_room_to_say_something_but_never_more_than_the_ceiling() {
        assert_eq!(output_budget(128_000, 200_000, 199_000), MIN_OUTPUT);
        assert_eq!(output_budget(1_000, 200_000, 199_000), 1_000);
    }
}
