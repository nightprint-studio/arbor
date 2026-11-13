//! How many requests one run may make before it is stopped — the brake for the day a model goes round in
//! circles. A step is one request to the model, however many tools it calls in it. The user sets the number
//! (Settings › Imber › Conversations); every product that hosts Imber reads it through [`clamp_max_steps`] and
//! words the stop with [`too_many_steps_notice`], so the rule and the sentence live here once.

pub const DEFAULT_MAX_STEPS: u32 = 40;
pub const MIN_MAX_STEPS: u32 = 5;
pub const MAX_MAX_STEPS: u32 = 300;

/// The configured steps per run, kept to a sane range; `0` (unset) is the default.
pub fn clamp_max_steps(steps: u32) -> u32 {
    if steps == 0 { DEFAULT_MAX_STEPS } else { steps.clamp(MIN_MAX_STEPS, MAX_MAX_STEPS) }
}

/// The steps a run may make: the setting, or no cap at all for a conversation the user marked non-stop. A
/// non-stop run still ends when the model is done, when it is cancelled, and at the spending limits — those are
/// the brakes it keeps.
pub fn run_steps(setting: u32, non_stop: bool) -> u32 {
    if non_stop { u32::MAX } else { clamp_max_steps(setting) }
}

/// The notice put at the end of a run that used all its steps.
pub fn too_many_steps_notice(steps: u32) -> String {
    format!("Stopped after {steps} steps. Say \"continue\" to go on, or raise the limit in Settings › Imber › Conversations.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_is_the_default_and_the_rest_is_kept_in_range() {
        assert_eq!(clamp_max_steps(0), DEFAULT_MAX_STEPS);
        assert_eq!(clamp_max_steps(1), MIN_MAX_STEPS);
        assert_eq!(clamp_max_steps(80), 80);
        assert_eq!(clamp_max_steps(10_000), MAX_MAX_STEPS);
    }

    #[test]
    fn non_stop_lifts_the_cap_whatever_the_setting() {
        assert_eq!(run_steps(0, false), DEFAULT_MAX_STEPS);
        assert_eq!(run_steps(80, false), 80);
        assert_eq!(run_steps(80, true), u32::MAX);
    }

    #[test]
    fn the_notice_names_the_number_and_where_to_change_it() {
        let text = too_many_steps_notice(60);
        assert!(text.contains("60 steps") && text.contains("continue") && text.contains("Settings"));
    }
}
