//! The words and the request of a scheduled run, as pure functions of the job and the user's settings.

use bennu_assistant::prelude::AssistantConfig;
use imber_schedule::prelude::{brief, describe_when};
use serde_json::{json, Value};

use super::unattended::FAMILIES_OFF;

/// Put to the model at the head of every scheduled message, before the saved prompt: what is different about a
/// run nobody is watching. The standing instructions stay the same, so the cached prefix is untouched.
pub const SCHEDULED_NOTE: &str = "[This is a scheduled run. Nobody is watching and nobody can answer: do not ask questions. \
Edits you make are left as proposals for the user to review later; nothing is written to the project, and commands, processes and deletions are not available. \
Decide conservatively, keep the answer short, and say plainly what needs a person.]";

/// How the scheduled message opens in the thread, and so in the conversation's title.
pub fn heading(job_name: &str) -> String {
    format!("[Scheduled: {job_name}] ")
}

/// Where a scheduled run starts from.
pub struct Wake<'a> {
    pub job_name: &'a str,
    /// The conversation to continue; a new one when absent.
    pub conversation_id: Option<&'a str>,
    pub prompt: &'a str,
    pub root: &'a str,
    /// Where today, this week and this month began (for the spending limits).
    pub periods: (i64, i64, i64),
}

/// The arguments of `assistant_send` for a scheduled run: the user's defaults (model, effort, instructions, plan,
/// skills, decision level), and none of the freedoms. A run nobody watches is manual, stays inside the project,
/// has no git, no sub-agents, no isolated copy, and is not offered the tools it could not use.
pub fn send_args(cfg: &AssistantConfig, wake: &Wake<'_>) -> Value {
    let mut families_off: Vec<String> = cfg.tool_families_off.clone();
    for id in FAMILIES_OFF {
        if !families_off.iter().any(|f| f == id) {
            families_off.push(id.to_string());
        }
    }
    json!({
        "conversation_id": wake.conversation_id,
        "text": format!("{}{}", heading(wake.job_name), wake.prompt),
        "root": wake.root,
        "model": cfg.model,
        "effort": cfg.effort,
        "mode": "manual",
        "scope": "project",
        "allow_edits": cfg.allow_edits,
        "allow_temp": cfg.allow_temp,
        "max_projects": 1,
        "max_context": cfg.max_context,
        "chat": false,
        "git": false,
        "bevy_write": false,
        "debug_mutate": false,
        "subagents_enabled": false,
        "instructions": cfg.use_instructions,
        "plan": cfg.plan_enabled,
        "families_off": families_off,
        "skills_on": cfg.skills_on,
        "skill_folders": cfg.skill_folders,
        "decision_level": cfg.decision_level,
        "decision_notes": cfg.decision_notes,
        "debug_idle_minutes": cfg.debug_idle_minutes,
        "max_steps": cfg.max_steps,
        "periods": { "day": wake.periods.0.max(0), "week": wake.periods.1.max(0), "month": wake.periods.2.max(0) },
    })
}

/// The approval card for `schedule_create`: a title and what the schedule will do, in words. Imber creates future
/// behaviour with this, so the user reads it first, in every mode.
pub fn creation_card(input: &Value) -> (String, String) {
    let text = |k: &str| input.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let when = describe_when(input.get("when"));
    let mut when_cap = when.chars();
    let when_cap = when_cap.next().map(|c| c.to_uppercase().collect::<String>() + when_cap.as_str()).unwrap_or_default();
    let what = match text("action") {
        Some("awake") => "wakes this conversation",
        _ => "starts a new conversation",
    };
    let ask = text("prompt").map(|p| format!(" and asks: \u{201c}{}\u{201d}", brief(p, 160))).unwrap_or_default();
    let title = format!("Create schedule \u{201c}{}\u{201d}?", text("name").unwrap_or("Imber job"));
    let detail = format!(
        "{when_cap} \u{b7} read-only \u{b7} {what}{ask}. It runs only while Bennu is open; it never changes the project (edits come back as proposals) and never runs commands."
    );
    (title, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wake<'a>(conversation: Option<&'a str>) -> Wake<'a> {
        Wake { job_name: "Morning check", conversation_id: conversation, prompt: "What changed on main?", root: "/work/shop", periods: (10, 5, 1) }
    }

    #[test]
    fn a_scheduled_message_opens_with_the_schedule_s_name() {
        let args = send_args(&AssistantConfig::default(), &wake(None));
        assert_eq!(args["text"], "[Scheduled: Morning check] What changed on main?");
        assert_eq!(heading("x"), "[Scheduled: x] ");
    }

    #[test]
    fn a_scheduled_run_gets_the_defaults_and_none_of_the_freedoms() {
        let cfg = AssistantConfig { mode: "autonomous".into(), scope: "disk".into(), git_enabled: true, subagents_enabled: true, bevy_write_enabled: true, debug_mutate_enabled: true, ..Default::default() };
        let a = send_args(&cfg, &wake(Some("c-7")));
        assert_eq!(a["conversation_id"], "c-7");
        assert_eq!((a["mode"].as_str(), a["scope"].as_str()), (Some("manual"), Some("project")), "whatever the user's own settings are");
        for off in ["git", "bevy_write", "debug_mutate", "subagents_enabled", "chat"] {
            assert_eq!(a[off], false, "{off}");
        }
        assert_eq!(a["max_projects"], 1);
        assert_eq!(a["model"], cfg.model.as_str());
        let families: Vec<&str> = a["families_off"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        for id in FAMILIES_OFF {
            assert!(families.contains(&id), "{id}");
        }
        assert_eq!(a["periods"]["day"], 10);
    }

    #[test]
    fn the_users_own_families_off_are_kept_once() {
        let cfg = AssistantConfig { tool_families_off: vec!["history".into(), "run".into()], ..Default::default() };
        let a = send_args(&cfg, &wake(None));
        let families: Vec<&str> = a["families_off"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert_eq!(families.iter().filter(|f| **f == "run").count(), 1);
        assert!(families.contains(&"history"));
    }

    #[test]
    fn the_arguments_are_a_send_the_assistant_can_read() {
        let a = send_args(&AssistantConfig::default(), &wake(None));
        let parsed: crate::assistant::SendArgs = serde_json::from_value(a).expect("a scheduled run is an ordinary send");
        assert_eq!(parsed.root, "/work/shop");
        assert!(!parsed.chat && !parsed.git && parsed.worktree.is_none() && parsed.attachments.is_empty());
    }

    #[test]
    fn the_approval_card_spells_out_when_what_and_how_free() {
        let (title, detail) = creation_card(&json!({
            "name": "Morning check", "action": "prompt", "prompt": "What changed on main?",
            "when": {"kind": "weekly", "days": ["mon", "fri"], "time": "09:00"}
        }));
        assert_eq!(title, "Create schedule \u{201c}Morning check\u{201d}?");
        assert!(detail.starts_with("Every mon, fri at 09:00 \u{b7} read-only \u{b7} starts a new conversation"), "{detail}");
        assert!(detail.contains("What changed on main?") && detail.contains("only while Bennu is open") && detail.contains("never changes the project"), "{detail}");
        let (_, awake) = creation_card(&json!({"name": "n", "action": "awake", "when": {"kind": "every", "minutes": 30}}));
        assert!(awake.contains("wakes this conversation") && awake.starts_with("Every 30 min"), "{awake}");
    }
}
