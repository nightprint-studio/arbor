//! What the model is told before the conversation starts.
//!
//! Short on purpose, and mostly about the shape of the work rather than the project: where it is, what
//! the tools are for, what "proposed" means. The instructions are sent — and cached — with every
//! request, so each sentence is paid for on every turn; the project itself is learned through tools,
//! which cost only when they are used.

pub(super) struct Context<'a> {
    pub root: &'a str,
    /// The file the user is in, if any.
    pub file: Option<&'a str>,
    pub auto: bool,
    pub allow_edits: bool,
    /// A conversation with no tools.
    pub chat: bool,
    /// The project's instruction files, already rendered (empty when there are none).
    pub instructions: &'a str,
    /// Git tools are available, and whether people in their results are hidden behind ids.
    pub git: bool,
    pub hide_authors: bool,
    /// The step plan is on: the instruction to keep one is included.
    pub plan: bool,
    /// The projects in play, when there are several: named in the instructions.
    pub projects: &'a [super::projects::Project],
}

pub(super) fn system(c: &Context) -> String {
    if c.chat {
        return chat_system(c);
    }
    let mut s = String::from(
        "You are the assistant built into Bennu, a code editor for Java, Rust and web projects. You work on the user's project through the tools you are given — there is no shell, and you cannot run arbitrary commands.\n\n",
    );
    s.push_str(&format!("The project is at `{}`. Paths may be given relative to it.\n", c.root));
    if let Some(f) = c.file {
        s.push_str(&format!("The user is looking at `{f}`.\n"));
    }
    s.push_str(
        "\nHow to work:\n\
- Look before you change. Use Bennu's tools for structure where they serve the file's language (usages, symbols, problems — and endpoints and beans in Java) — they are exact where text search is not — and the text search for strings, comments and configuration.\n\
- Prefer `rename_symbol` to editing text by hand where a language server or the Java index serves the file: it updates every reference, in every file, as one change (`add_parameter` is for Java only). Find the symbol's line and column with the symbol tools first.\n\
- Make edits with `edit_file` (exact replacement, small and local) or `create_file`. Read the file first so `old_text` matches exactly.\n\
- When a decision is the user's — a name, a scope, a trade-off — ask with `ask_user` instead of guessing. Offer 2-4 concrete options.\n\
- Reading or changing something outside the allowed area raises a prompt for the user; if they decline, accept it and find another way.\n\
- Deleting always asks the user, and what is deleted can be restored from Local History.\n\
- When the user asks how to do something in Bennu itself — a feature, a shortcut, where a panel is — call `bennu_help` and answer from what it returns. You do not know Bennu from your training: never describe its menus or shortcuts from memory, and if the help has nothing, say so.\n\
- Be brief. Say what you found and what you changed; do not narrate every step.\n\
- Write your answers in Markdown, which the panel renders: **bold** for what matters, short lists, inline `code` only for short identifiers (a class, a method, a file, an annotation name) and never for ordinary words or whole expressions, fenced blocks with the language (```java) for any snippet of more than a few tokens, a table when comparing things, and a heading only when an answer has real sections. Do not wrap a whole answer in a code block.\n",
    );
    if c.plan {
        s.push_str(PLAN);
    }
    if c.git {
        s.push_str(GIT);
        if c.hide_authors {
            s.push_str(HIDDEN_AUTHORS);
        }
    }
    s.push_str(&kind_block(c.projects.first()));
    s.push_str(&super::projects::describe(c.projects));
    s.push_str(c.instructions);
    if !c.allow_edits {
        s.push_str("\nThe user has turned editing off: you may read and search, not change anything. Explain what you would change instead.\n");
    } else if c.auto {
        s.push_str("\nEdits are applied as soon as you make them (the user can undo them). Check your work with the problems tool when it matters.\n");
    } else {
        s.push_str("\nEdits are proposed, not applied: the user reviews them. A tool result that says \"proposed\" means the file is unchanged for now — do not describe it as done, and do not make a later edit depend on it.\n");
    }
    s
}

/// What the project's build manifests change about the work: which tools serve it and which do not, and
/// where to point them. A sentence or two for each ecosystem the project has — they are sent with every
/// request — and nothing for plain Java, which is what the rest of the text assumes.
fn kind_block(project: Option<&super::projects::Project>) -> String {
    let Some(p) = project else { return String::new() };
    let java = p.has("Maven") || p.has("Gradle");
    let root = p.root.trim_end_matches(['/', '\\']);
    // The folder to give a tool as `root`, when its manifest is not at the project's root.
    let point_at = |dirs: &[&str]| -> String {
        let below: Vec<String> = dirs.iter().filter(|d| **d != ".").map(|d| format!("`{root}/{d}`")).collect();
        if below.is_empty() { String::new() } else { format!(" Bennu's tools work on the folder that holds the manifest, so give them root {}.", below.join(" or ")) }
    };
    let mut s = String::new();
    let cargo = p.dirs("Cargo");
    if !cargo.is_empty() {
        s.push_str("\nThere is Rust here. Navigation comes from rust-analyzer, which may still be loading: an empty answer then means not ready, not nothing there. ");
        if !java {
            s.push_str("There is no Java index — bennu_class_index, beans, endpoints and add_parameter do not apply. ");
        }
        s.push_str("Check edits with bennu_problems or bennu_build (cargo check) and run tests with bennu_test_run.");
        s.push_str(&point_at(&cargo));
        s.push('\n');
        if p.bevy {
            s.push_str("It uses Bevy: look at bennu_catalog (components, systems, conflicts) before changing a system; an unordered conflict is a frame-order question, not an error.\n");
        }
    }
    let node = p.dirs("Node");
    if !node.is_empty() {
        s.push_str("\nThere is a Node project here. Navigation works only if its language server (TypeScript, Svelte, Angular) is installed; otherwise use search_text, find_files and read the files. ");
        if !java && cargo.is_empty() {
            s.push_str("There is no Java index — ignore the bean, endpoint and class tools and add_parameter. ");
        }
        s.push_str("There is no tool to run npm scripts or a build or test here: say what to run instead of claiming it passes.");
        s.push_str(&point_at(&node));
        s.push('\n');
    }
    s
}

const PLAN: &str = "\nFor work with three or more steps, keep a plan with `update_plan`: write the steps first, then tick them off as you go. Send each update in the same turn as the other tools you are calling, not on its own — a turn that only updates the plan costs a whole extra request. Skip it for a question or a small change.\n";

const GIT: &str = "\nGit is available: corvus_status, corvus_diff, corvus_log and corvus_blame to read, corvus_stage, corvus_unstage and corvus_commit to change. Read the status and the diff before committing, write commit messages in the style of the project's recent ones (read corvus_log first), and commit only when the user asks. You cannot push.\n";

const HIDDEN_AUTHORS: &str = "People in git results appear as ids like Author-7f3a9c, to keep their names private. Use an id exactly as given whenever you refer to that person; it is turned back into their name for the user, and in anything you send to a tool.\n";

/// The instructions for a conversation with no tools: it can only talk, about what the user shows it.
fn chat_system(c: &Context) -> String {
    let mut s = String::from(
        "You are the assistant built into Bennu, a code editor for Java, Rust and web projects. This is a chat only: you cannot read or search the project and you cannot change anything. Your one tool, `bennu_help`, searches Bennu's own documentation — call it when the user asks how to do something in Bennu, answer from what it returns, and never describe Bennu's features from memory. Answer from the conversation and from what the user pastes or selects. If you would need to see a file to answer well, say which one and ask the user to paste it.\n",
    );
    if let Some(f) = c.file {
        s.push_str(&format!("The user is looking at `{f}`, but you cannot read it; you only see what they send.\n"));
    }
    s.push_str("\nBe brief. Write your answers in Markdown, which the panel renders: **bold** for what matters, short lists, inline `code` only for short identifiers, fenced blocks with the language for any snippet of more than a few tokens, a table when comparing things.\n");
    s.push_str(c.instructions);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(auto: bool, allow_edits: bool) -> Context<'static> {
        Context { root: "C:/p", file: Some("src/A.java"), auto, allow_edits, chat: false, instructions: "", git: false, hide_authors: false, plan: true, projects: &[] }
    }

    #[test]
    fn it_names_the_project_and_the_open_file() {
        let s = system(&ctx(false, true));
        assert!(s.contains("`C:/p`") && s.contains("`src/A.java`"));
    }

    #[test]
    fn the_mode_decides_what_it_is_told_about_edits() {
        assert!(system(&ctx(false, true)).contains("proposed, not applied"));
        assert!(system(&ctx(true, true)).contains("applied as soon as"));
        assert!(system(&ctx(true, false)).contains("editing off"));
    }

    #[test]
    fn a_chat_has_no_tools_in_its_instructions_and_says_so() {
        let s = system(&Context { chat: true, ..ctx(false, true) });
        assert!(s.contains("cannot read or search the project") && s.contains("bennu_help") && !s.contains("edit_file") && !s.contains("rename_symbol"));
        assert!(s.contains("Markdown"));
    }

    #[test]
    fn the_project_instructions_come_after_the_rules() {
        let s = system(&Context { instructions: "\n--- CLAUDE.md ---\nUse tabs.\n", ..ctx(false, true) });
        assert!(s.contains("Use tabs.") && s.find("How to work").unwrap() < s.find("Use tabs.").unwrap());
        let chat = system(&Context { chat: true, instructions: "--- X ---", ..ctx(false, true) });
        assert!(chat.contains("--- X ---"));
    }

    #[test]
    fn git_is_described_only_when_it_is_on_and_the_ids_only_when_they_are_in_use() {
        let off = system(&ctx(false, true));
        assert!(!off.contains("corvus_") && !off.contains("Author-"));
        let on = system(&Context { git: true, ..ctx(false, true) });
        assert!(on.contains("corvus_commit") && on.contains("You cannot push") && !on.contains("Author-"));
        let hidden = system(&Context { git: true, hide_authors: true, ..ctx(false, true) });
        assert!(hidden.contains("Author-7f3a9c"));
    }

    #[test]
    fn the_plan_is_asked_for_only_when_it_is_on() {
        assert!(system(&ctx(false, true)).contains("update_plan"));
        assert!(!system(&Context { plan: false, ..ctx(false, true) }).contains("update_plan"));
    }

    fn project(modules: &[(&'static str, &str)], bevy: bool) -> super::super::projects::Project {
        super::super::projects::Project {
            name: "p".into(),
            root: "C:/p".into(),
            modules: modules.iter().map(|(kind, dir)| super::super::projects::Module { kind, dir: dir.to_string() }).collect(),
            bevy,
        }
    }

    #[test]
    fn the_manifests_of_the_project_decide_which_tools_it_is_told_about() {
        let cargo = [project(&[("Cargo", ".")], false)];
        let s = system(&Context { projects: &cargo, ..ctx(false, true) });
        assert!(s.contains("rust-analyzer") && s.contains("bennu_test_run") && !s.contains("Bevy") && !s.contains("give them root"));
        let bevy = [project(&[("Cargo", ".")], true)];
        assert!(system(&Context { projects: &bevy, ..ctx(false, true) }).contains("bennu_catalog"));
        let node = [project(&[("Node", ".")], false)];
        let n = system(&Context { projects: &node, ..ctx(false, true) });
        assert!(n.contains("Node project") && n.contains("no tool to run npm") && !n.contains("rust-analyzer"));
        let java = [project(&[("Maven", ".")], false)];
        let j = system(&Context { projects: &java, ..ctx(false, true) });
        assert!(!j.contains("rust-analyzer") && !j.contains("Node project"));
    }

    #[test]
    fn a_project_with_both_ecosystems_is_told_where_each_one_lives() {
        let mixed = [project(&[("Node", "."), ("Cargo", "src-tauri")], false)];
        let s = system(&Context { projects: &mixed, ..ctx(false, true) });
        assert!(s.contains("There is Rust here") && s.contains("There is a Node project here"));
        assert!(s.contains("C:/p/src-tauri"), "the Rust tools must be pointed at the folder with the Cargo.toml");
        assert_eq!(s.matches("give them root").count(), 1, "only the Rust manifest is below the root");
        assert_eq!(s.matches("There is no Java index").count(), 1, "said once, not per ecosystem");
    }

    #[test]
    fn java_beside_rust_keeps_its_own_tools() {
        let both = [project(&[("Maven", "."), ("Cargo", "native")], false)];
        let s = system(&Context { projects: &both, ..ctx(false, true) });
        assert!(s.contains("There is Rust here") && !s.contains("There is no Java index") && s.contains("C:/p/native"));
    }

    #[test]
    fn the_wording_is_not_java_only() {
        let s = system(&ctx(false, true));
        assert!(s.contains("Java, Rust and web projects") && s.contains("is for Java only"));
    }

    #[test]
    fn it_asks_for_markdown() {
        assert!(system(&ctx(false, true)).contains("Markdown"));
    }

    #[test]
    fn it_never_promises_a_shell() {
        assert!(system(&ctx(false, true)).contains("no shell"));
    }
}
