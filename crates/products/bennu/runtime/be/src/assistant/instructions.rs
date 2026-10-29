//! The project's own instructions for the assistant: `CLAUDE.md`, `AGENTS.md`, and a file of the user's.
//!
//! Teams already write down how their project wants to be worked on — the build command, the conventions,
//! what not to touch — in the files other coding assistants read. Reading the same files means the
//! assistant follows rules that already exist instead of asking for them to be written a second time.
//!
//! Only the project root's files and the user's own are read, not those of every folder down to the open
//! file: the instructions travel in the cached part of the request, and a text that changed with every file
//! the user opened would be paid for in full each time.

use std::path::{Path, PathBuf};

/// The most of one file that is sent. A file longer than this is cut at a line, and says so.
const MAX_FILE_BYTES: usize = 16 * 1024;
/// The most of all of them together.
const MAX_TOTAL_BYTES: usize = 40 * 1024;

/// Where, under a project root, instruction files are looked for — in this order.
const PROJECT_FILES: [&str; 3] = ["CLAUDE.md", ".claude/CLAUDE.md", "AGENTS.md"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Instruction {
    /// What it is called in the panel and in the request: the file name, or `your instructions`.
    pub name: String,
    pub text: String,
    /// The file was longer than what is sent.
    pub truncated: bool,
}

/// The user's own file, which applies to every project.
pub(super) fn user_file() -> PathBuf {
    arbor_core::prelude::bennu_config_path("assistant-instructions.md")
}

/// `text` cut to at most `limit` bytes, at a line break when there is one, and whether it was cut.
fn clip(text: &str, limit: usize) -> (String, bool) {
    if text.len() <= limit {
        return (text.to_string(), false);
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let cut = &text[..end];
    let at = cut.rfind('\n').unwrap_or(end);
    (cut[..at].to_string(), true)
}

/// Put the files' texts in order, drop a second copy of a text already there (a project that mirrors
/// `CLAUDE.md` as `AGENTS.md` should not pay for it twice), and keep within the total.
fn assemble(found: Vec<(String, String)>) -> Vec<Instruction> {
    let mut out: Vec<Instruction> = Vec::new();
    let mut total = 0usize;
    for (name, text) in found {
        let text = text.trim();
        if text.is_empty() || out.iter().any(|i| i.text == text) {
            continue;
        }
        let room = MAX_TOTAL_BYTES.saturating_sub(total).min(MAX_FILE_BYTES);
        if room == 0 {
            break;
        }
        let (body, truncated) = clip(text, room);
        total += body.len();
        out.push(Instruction { name, text: body, truncated });
    }
    out
}

/// The instructions that apply under `root`: the user's file first, then the project's.
pub(super) fn load(root: &Path) -> Vec<Instruction> {
    let mut found: Vec<(String, String)> = Vec::new();
    if let Ok(text) = std::fs::read_to_string(user_file()) {
        found.push(("your instructions".to_string(), text));
    }
    for rel in PROJECT_FILES {
        if let Ok(text) = std::fs::read_to_string(root.join(rel)) {
            found.push((rel.to_string(), text));
        }
    }
    assemble(found)
}

/// The text added to the system prompt, or nothing when there are no instructions.
pub(super) fn render(instructions: &[Instruction]) -> String {
    if instructions.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "\nThe user keeps instructions for this work in files; they follow. Treat them as the user's own rules \
         for this project and follow them, except where they would have you do something the rules above forbid \
         (running a shell, going past an access prompt).\n",
    );
    for i in instructions {
        out.push_str(&format!("\n--- {} ---\n{}\n", i.name, i.text));
        if i.truncated {
            out.push_str("[the rest of this file was left out for length]\n");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(n, t)| (n.to_string(), t.to_string())).collect()
    }

    #[test]
    fn files_keep_their_order_and_empty_ones_are_skipped() {
        let got = assemble(named(&[("a", "one"), ("b", "  \n"), ("c", "three")]));
        assert_eq!(got.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["a", "c"]);
    }

    #[test]
    fn a_mirrored_file_is_not_sent_twice() {
        let got = assemble(named(&[("CLAUDE.md", "Use tabs.\n"), ("AGENTS.md", "Use tabs.")]));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "CLAUDE.md");
    }

    #[test]
    fn a_long_file_is_cut_at_a_line_and_says_so() {
        let long = "line\n".repeat(10_000);
        let got = assemble(named(&[("CLAUDE.md", &long)]));
        assert!(got[0].truncated);
        assert!(got[0].text.len() <= MAX_FILE_BYTES);
        assert!(got[0].text.ends_with("line"), "cut at a line break, not mid-word");
    }

    #[test]
    fn the_total_is_bounded_across_files() {
        let big = "x\n".repeat(7_000); // ~14 KB each
        let got = assemble(named(&[("a", &big), ("b", &format!("{big}b")), ("c", &format!("{big}c")), ("d", &format!("{big}d"))]));
        let total: usize = got.iter().map(|i| i.text.len()).sum();
        assert!(total <= MAX_TOTAL_BYTES, "{total}");
    }

    #[test]
    fn a_cut_never_lands_inside_a_character() {
        let text = "è".repeat(MAX_FILE_BYTES); // two bytes each
        let (cut, truncated) = clip(&text, MAX_FILE_BYTES);
        assert!(truncated);
        assert!(cut.chars().all(|c| c == 'è'));
    }

    #[test]
    fn nothing_to_say_renders_as_nothing() {
        assert_eq!(render(&[]), "");
    }

    #[test]
    fn the_rendering_names_each_file_and_flags_a_cut() {
        let text = render(&[
            Instruction { name: "CLAUDE.md".into(), text: "Run `mvn -q verify`.".into(), truncated: true },
        ]);
        assert!(text.contains("--- CLAUDE.md ---") && text.contains("mvn -q verify") && text.contains("left out for length"));
        assert!(text.contains("rules above forbid"), "the safety rules still win");
    }
}
