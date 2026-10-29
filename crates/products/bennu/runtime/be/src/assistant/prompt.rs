//! What the model is told before the conversation starts.
//!
//! Short on purpose, and mostly about the shape of the work rather than the project: where it is, what
//! the tools are for, what "proposed" means. The instructions are sent — and cached — with every
//! request, so each sentence is paid for on every turn; the project itself is learned through tools,
//! which cost only when they are used.

use bennu_assistant::prelude::AccessMode;

pub(super) struct Context<'a> {
    pub root: &'a str,
    pub mode: AccessMode,
    pub allow_edits: bool,
    /// A conversation with no tools.
    pub chat: bool,
    /// The project's instruction files, already rendered (empty when there are none).
    pub instructions: &'a str,
    /// Git tools are available, and whether people in their results are hidden behind ids.
    pub git: bool,
    pub hide_authors: bool,
    /// The index of the skills the user enabled, already rendered (empty when there are none).
    pub skills: &'a str,
    /// The step plan is on: the instruction to keep one is included.
    pub plan: bool,
    /// The projects in play, when there are several: named in the instructions.
    pub projects: &'a [super::projects::Project],
    /// The live tools (managed processes, loopback HTTP, scenarios) are offered: the paragraph on how to use them is included.
    pub live: bool,
    /// The conversation works on an isolated copy of the project, and which kind.
    pub worktree: Option<bennu_assistant::prelude::WorktreeMode>,
}

pub(super) fn system(c: &Context) -> String {
    if c.chat {
        return chat_system(c);
    }
    let mut s = String::from(
        "You are Imber, the assistant built into Bennu, a code editor for Java, Rust and web projects. You are built on Claude, a model made by Anthropic. You work on the user's project through the tools you are given — there is no shell, and you cannot run arbitrary commands.\n\n",
    );
    s.push_str(&format!("The project is at `{}`. Paths may be given relative to it.\n", c.root));
    // The file the user is in is NOT here: it changes with every click, and anything that changes in the system
    // prompt invalidates the cached conversation after it. It travels with the user's message (`[Open file: …]`).
    s.push_str(
        "\nHow to work:\n\
- Look before you change. Use Bennu's tools for structure where they serve the file's language (usages, symbols, problems — and endpoints and beans in Java) — they are exact where text search is not — and the text search for strings, comments and configuration.\n\
- Read cheaply: every tool result is re-sent with each later request. Open an unfamiliar file with `file_outline`, then read only the part you need (`bennu_read_file` with a `symbol` or a line range); read several files in one `read_files` call. A file you already read comes back as one line when unchanged, or as a diff. A result that ends with a digest names an id: `expand_result` gives the detail you need, not all of it.\n\
- Prefer `rename_symbol` to editing text by hand where a language server or the Java index serves the file: it updates every reference, in every file, as one change (`add_parameter` is for Java only). Find the symbol's line and column with the symbol tools first.\n\
- Make edits with `edit_file` (exact replacement, small and local) or `create_file`. Read the file first so `old_text` matches exactly.\n\
- When a question, a fork or a decision is the user's — a name, a scope, a trade-off, an architectural or design choice, two reasonable ways to implement something, a requirement that is unclear — stop and ask with `ask_user` before building on a guess. Offer 2-4 concrete options, say which one you would pick and why, and let the user answer in their own words if none fits. Do not ask about what you can find out yourself with the tools, or about small details that are easy to change later; decide those and say so.\n\
- Reading or changing something outside the allowed area raises a prompt for the user; if they decline, accept it and find another way.\n\
- You may create and delete files freely in your own temp folder. In the project, deleting is allowed without asking only when the user's mode applies edits and the files are inside what they allowed (up to 50 files in one call); otherwise, and always for the project folder itself or anything outside what they allowed, it asks. Anything deleted in the project can be restored from Local History.\n\
- When the user asks how to do something in Bennu itself — a feature, a shortcut, where a panel is — call `bennu_help` and answer from what it returns. You do not know Bennu from your training: never describe its menus or shortcuts from memory, and if the help has nothing, say so.\n\
- Be brief. Say what you found and what you changed; do not narrate every step.\n\
- Write your answers in Markdown, which the panel renders: **bold** for what matters, short lists, inline `code` only for short identifiers (a class, a method, a file, an annotation name) and never for ordinary words or whole expressions, fenced blocks with the language (```java) for any snippet of more than a few tokens, a table when comparing things, and a heading only when an answer has real sections. Do not wrap a whole answer in a code block.\n",
    );
    s.push_str(DIAGRAMS);
    if c.plan {
        s.push_str(PLAN);
    }
    if c.git {
        s.push_str(GIT);
        if c.hide_authors {
            s.push_str(HIDDEN_AUTHORS);
        }
    }
    if let Some(mode) = c.worktree {
        s.push_str(worktree_block(mode));
    }
    if c.live {
        s.push_str(LIVE);
    }
    s.push_str(&kind_block(c.projects.first()));
    s.push_str(&super::projects::describe(c.projects));
    s.push_str(c.instructions);
    // After the instructions and before the per-mode paragraph: the standing text stays byte-identical
    // between requests, which is what keeps it cached.
    s.push_str(c.skills);
    if !c.allow_edits {
        s.push_str("\nThe user has turned editing off: you may read and search, not change anything. Explain what you would change instead.\n");
    } else {
        s.push_str(mode_block(c.mode));
    }
    s
}

/// What the run may do on its own, by mode. The last paragraph of the instructions, so the standing text before
/// it stays byte-identical between modes.
fn mode_block(mode: AccessMode) -> &'static str {
    match mode {
        AccessMode::Manual => "\nMode: Manual. Edits are proposed, not applied: the user reviews them. A tool result that says \"proposed\" means the file is unchanged for now — do not describe it as done, and do not make a later edit depend on it. Building and testing (bennu_build, bennu_test_run) ask the user before every call. Anything outside the area the user allowed raises a prompt, including a file a compile error points at; if they decline, say so and find another way.\n",
        AccessMode::AcceptEdits => "\nMode: Accept edits. Edits are applied as soon as you make them (the user can undo them). Check your work with the problems tool when it matters. The first build or test run of a run asks the user once; after a yes, later ones in the same run do not. Anything outside the area the user allowed raises a prompt, including a file a build error points at; if they decline, say so and find another way.\n",
        AccessMode::Autonomous => AUTONOMOUS,
    }
}

const AUTONOMOUS: &str = "\nMode: Autonomous. Edits are applied as soon as you make them (the user can undo them). Check your work with the problems tool when it matters. The project's own build and tests (bennu_build, bennu_test_run) run without asking; arbitrary commands and starting processes still ask. One thing more: when a build or check you ran yourself (bennu_build, bennu_check_file, bennu_problems) reports an error in a file of this project, you may read and fix that file without asking — it is opened for this run only, for at most 25 files, and never for build output, dependency manifests (pom.xml, Cargo.toml, package.json, lockfiles) or secrets. Everything else still asks the user: files of other projects (unless the conversation already covers the workspace), anything outside the workspace, deleting, and starting processes. A file you were not shown by a build or check is not yours to open, so run the build first. The user sees every file you open this way and can take it back.\n";

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

/// Diagrams are drawn through a tool, so the model is told when to reach for it and what to do when it fails.
/// The one text serves the agent and the chat; the tool is offered to both.
const DIAGRAMS: &str = "- Diagrams are supported through `show_diagram`, which takes Mermaid source (flowchart, sequence, class, state, ER, gantt…). Use it when a picture explains better than text — an architecture, a flow, a call chain, the entities of a model and how they relate. Keep a diagram small and readable. If the tool reports a syntax error, fix the source and send it again. Never paste a ```mermaid block in your text: the tool is how a diagram is shown.\n";

const PLAN: &str ="\nFor work with three or more steps, keep a plan with `update_plan`: write the steps first, then tick them off as you go. Send each update in the same turn as the other tools you are calling, not on its own — a turn that only updates the plan costs a whole extra request. Skip it for a question or a small change.\n";

const GIT: &str = "\nGit is available: corvus_status, corvus_diff, corvus_log and corvus_blame to read, corvus_stage, corvus_unstage and corvus_commit to change. Read the status and the diff before committing, write commit messages in the style of the project's recent ones (read corvus_log first), and commit only when the user asks. You cannot push.\n";

const LIVE: &str = "\nYou can run the program and watch it: `list_recipes` shows the project's named stacks, `run_recipe` starts one in dependency order (or `run_start` for a single process), `wait_ready` waits until a process is up, `run_logs` / `run_status` / `timeline` show what it did, `http_request` calls a service on localhost and returns a correlation id, `trace_request` follows that id through every service's logs, and `list_scenarios` / `run_scenario` / `save_scenario` run and write ordered checks with assertions. Prefer a recipe to hand-written commands, wait for readiness before calling a service, and when a request fails read the logs of the service that handled it before changing code. Always stop what you started (`run_stop`) when you are done; you can only stop your own processes, and a name or port another conversation holds is refused — read it, do not fight for it.\n\nReading logs: never dump a log. Start with `log_facets` (exceptions grouped by root cause and the first frame in the project, SQL queries, HTTP requests, warnings) or `run_logs` with `summary`; open one thing with `log_record` using the sample id it printed; then narrow with `level`, `pattern`, `last` / `after` / `before` and `context`. Use `wait_log` to wait for a line instead of calling `run_logs` again and again, and `next_since` / `next_offset` to read only what is new. A request's correlation id is the thread through every service: `trace_request` follows it. `console_logs` reads what the user's own Run console printed, `tail_file` a log file on disk. Every answer says what it left out: ask for more only when it matters.\n";

/// How to use the debugger tools. Appended by `with_debug` when the family is on, so the standing text
/// stays byte-identical between requests of a conversation.
const DEBUG_GUIDE: &str = "\nYou can debug the program instead of guessing. When a bug depends on runtime values (a wrong total, a null, a branch you cannot explain from the source), reproduce it under the debugger: put a breakpoint (`debug_set_breakpoint`) on the line just before the suspected one, or a logpoint with `log_message` to observe without stopping, start the program with `debug_launch` (or `debug_attach` to one already running), trigger the failing path (an `http_request`, or the program's own main), then `debug_wait_stop`. At a stop read `debug_stack`, `debug_variables` (pass `path` to open an object) and `debug_evaluate` (a path such as `order.items[0].price`, never a method call), and `debug_step` to follow the code; `debug_output` shows what the program printed. Form a hypothesis from the values, not before them. The user watches the same stop in their Debug panel and can take over. The session belongs to this conversation, not to this message: the program stays alive between messages (suspended, if it was stopped at a breakpoint, so the user can inspect it) and your breakpoints stay set, so a later message can pick up where this one stopped. It ends only when you call `debug_detach`, when the program exits, after a stretch with no debug tool used, or when the project closes. Call `debug_detach` as soon as you no longer need the session — never leave it running \"just in case\"; detaching a program you launched stops it, one you attached to keeps running. Do not leave the program suspended longer than you need: `debug_resume` when you have what you need. Values of variables named like secrets are masked. Give a breakpoint `capture` (paths such as `order.id`) when you already know what you will want to read: Bennu reads them the moment the program stops and hands them to you, even if you are not waiting. When the program stops while you are busy or idle, Bennu tells you with a system note carrying where it stopped, the top of the stack, those values and the top frame's locals — read it before asking again. To get somewhere, `debug_run_to` (a line) and `debug_step` with a `count` are cheaper than many calls; `debug_break_on_exception` stops where an exception is thrown. You do not change the program unless tools named `debug_throw`, `debug_force_return`, `debug_drop_frame` or `debug_set_value` are among yours: those change the running program, cannot be undone, and the user approves them once per conversation, so use one only when observing is not enough.\n";

/// The system prompt with the debugger paragraph added when the debug tools are offered.
pub(super) fn with_debug(mut system: String, on: bool) -> String {
    if on {
        system.push_str(DEBUG_GUIDE);
    }
    system
}

/// How to drive a running Bevy game. Appended by `with_bevy` when the read family is on.
const BEVY_GUIDE: &str = "\nYou can look inside a running Bevy game over its Remote Protocol (the game needs RemotePlugin and RemoteHttpPlugin). The loop: start the game (`run_recipe` / `run_start`), `brp_connect` (it waits for the port; the first build can take minutes, so raise `wait_s` and read `run_logs` meanwhile), then `brp_query` for entities and component values (short names like `Health` work), `brp_get` for one entity, `brp_resources` / `brp_get_resource` for singletons, `brp_types` to search the registry; `run_logs` for what the game printed; `run_stop` when done. Keep queries narrow: output is capped.\n";

/// The extra sentence when changing the game is allowed.
const BEVY_WRITE_GUIDE: &str = "You may also change the running game with `brp_mutate`, `brp_insert`, `brp_remove`, `brp_spawn`, `brp_despawn` and `brp_set_resource`. That is live state with no proposal and no undo: the user approves the first change of the conversation and every change is listed in the run's summary. Change the least you need, read the value back, and say what you changed. Source edits are still the fix; a live change is an experiment.\n";

/// The system prompt with the Bevy paragraph added when the BRP tools are offered (and the write sentence when
/// writing is allowed).
pub(super) fn with_bevy(mut system: String, read: bool, write: bool) -> String {
    if read {
        system.push_str(BEVY_GUIDE);
        if write {
            system.push_str(BEVY_WRITE_GUIDE);
        }
    }
    system
}

/// How to use the screenshot tools.
const SCREENSHOT_GUIDE: &str = "\nYou can look at a web front end running on this machine: `screenshot_page` returns a picture of a localhost page (and, with Playwright in the project, its console errors and failed requests); `page_diagnostics` returns only that text and is much cheaper. Start the app first and wait for it. A picture costs a thousand tokens or more and only the latest two stay in the conversation: when a page is blank or broken run `page_diagnostics` first, take a screenshot to check a visual result, and do not poll.\n";

/// The system prompt with the screenshot paragraph added when those tools are offered.
pub(super) fn with_screenshots(mut system: String, on: bool) -> String {
    if on {
        system.push_str(SCREENSHOT_GUIDE);
    }
    system
}

/// The decision level and the user's notes about decisions, as the very last paragraph: they change between
/// conversations and mid-conversation, so everything before them stays byte-identical (and cached).
pub(super) fn with_decisions(mut system: String, paragraph: &str) -> String {
    system.push_str(paragraph);
    system
}

/// What an isolated copy changes about the work. The instructions are sent every request, so this is short.
fn worktree_block(mode: bennu_assistant::prelude::WorktreeMode) -> &'static str {
    use bennu_assistant::prelude::WorktreeMode;
    match mode {
        WorktreeMode::Copy => "\nYou are working on an isolated text-only copy of the project (the path above is the copy): nothing you change reaches the user's real files until they choose \"Apply to project\". Code-intelligence tools (usages, symbols, problems, rename, beans, endpoints) are not available here — use search_text, find_files, reading, edit_file and, where offered, build and live tools. Edits are written to the copy straight away; say what you changed so the user can review the combined diff.\n",
        WorktreeMode::Git => "\nYou are working on an isolated git worktree of the project (the path above is the worktree, on its own branch), indexed like the real project, so all tools work on it. Nothing you change reaches the user's real files until they choose \"Apply to project\"; uncommitted changes the user had are in the worktree. Do not look for the original folder.\n",
    }
}

const HIDDEN_AUTHORS: &str = "People in git results appear as ids like Author-7f3a9c, to keep their names private. Use an id exactly as given whenever you refer to that person; it is turned back into their name for the user, and in anything you send to a tool.\n";

/// The instructions for a conversation with no tools: it can only talk, about what the user shows it.
fn chat_system(c: &Context) -> String {
    let mut s = String::from(
        "You are Imber, the assistant built into Bennu, a code editor for Java, Rust and web projects. You are built on Claude, a model made by Anthropic. This is a chat only: you cannot read or search the project and you cannot change anything. Your tools are `bennu_help`, which searches Bennu's own documentation — call it when the user asks how to do something in Bennu, answer from what it returns, and never describe Bennu's features from memory — `open_ui`, which opens one of Bennu's panels or windows for the user when they ask to see it or when showing it is the clearest answer — and `show_diagram`, which draws a Mermaid diagram. Answer from the conversation and from what the user pastes or selects. If you would need to see a file to answer well, say which one and ask the user to paste it.\n",
    );
    s.push_str("\nBe brief. Write your answers in Markdown, which the panel renders: **bold** for what matters, short lists, inline `code` only for short identifiers, fenced blocks with the language for any snippet of more than a few tokens, a table when comparing things.\n");
    s.push_str(DIAGRAMS);
    s.push_str(c.instructions);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(mode: AccessMode, allow_edits: bool) -> Context<'static> {
        Context { root: "C:/p", mode, allow_edits, chat: false, instructions: "", skills: "", git: false, hide_authors: false, plan: true, projects: &[], live: false, worktree: None }
    }

    #[test]
    fn it_introduces_itself_as_imber_in_the_agent_and_the_chat_prompt() {
        let agent = system(&ctx(AccessMode::Manual, true));
        let chat = system(&Context { chat: true, ..ctx(AccessMode::Manual, true) });
        for s in [agent, chat] {
            assert!(s.starts_with("You are Imber, the assistant built into Bennu"), "{s}");
            assert!(s.contains("built on Claude"));
        }
    }

    #[test]
    fn it_names_the_project_but_not_the_open_file() {
        // The open file changes with every click; in the system prompt it would invalidate the cached conversation.
        let s = system(&ctx(AccessMode::Manual, true));
        assert!(s.contains("`C:/p`") && !s.contains("looking at"));
    }

    #[test]
    fn the_mode_decides_what_it_is_told_about_edits() {
        assert!(system(&ctx(AccessMode::Manual, true)).contains("proposed, not applied"));
        assert!(system(&ctx(AccessMode::AcceptEdits, true)).contains("applied as soon as"));
        assert!(system(&ctx(AccessMode::AcceptEdits, false)).contains("editing off"));
    }

    #[test]
    fn each_mode_is_named_and_only_autonomous_may_open_files_by_itself() {
        let manual = system(&ctx(AccessMode::Manual, true));
        let accept = system(&ctx(AccessMode::AcceptEdits, true));
        let auto = system(&ctx(AccessMode::Autonomous, true));
        assert!(manual.contains("Mode: Manual") && accept.contains("Mode: Accept edits") && auto.contains("Mode: Autonomous"));
        for asks in [&manual, &accept] {
            assert!(asks.contains("raises a prompt, including a file a") && !asks.contains("fix that file without asking"), "only Autonomous opens a file by itself");
        }
        assert!(auto.contains("fix that file without asking") && auto.contains("bennu_build") && auto.contains(&bennu_assistant::prelude::MAX_AUTO_GRANTS.to_string()));
        assert!(auto.contains("secrets") && auto.contains("deleting") && auto.contains("starting processes"), "the limits are told, not only the permission");
        assert!(auto.contains("applied as soon as"));
    }

    #[test]
    fn the_mode_paragraph_is_last_and_a_chat_is_not_told_one() {
        let s = system(&Context { instructions: "\nINSTR\n", ..ctx(AccessMode::Autonomous, true) });
        assert!(s.find("INSTR").unwrap() < s.find("Mode: Autonomous").unwrap());
        let chat = system(&Context { chat: true, ..ctx(AccessMode::Autonomous, true) });
        assert!(!chat.contains("Mode:"), "a chat has no edits to be careful about");
        let off = system(&ctx(AccessMode::Autonomous, false));
        assert!(off.contains("editing off") && !off.contains("Mode: Autonomous"), "editing switched off outranks every mode");
    }

    #[test]
    fn a_chat_has_no_tools_in_its_instructions_and_says_so() {
        let s = system(&Context { chat: true, ..ctx(AccessMode::Manual, true) });
        assert!(s.contains("cannot read or search the project") && s.contains("bennu_help") && s.contains("open_ui") && !s.contains("edit_file") && !s.contains("rename_symbol"));
        assert!(s.contains("Markdown"));
    }

    #[test]
    fn the_project_instructions_come_after_the_rules() {
        let s = system(&Context { instructions: "\n--- CLAUDE.md ---\nUse tabs.\n", ..ctx(AccessMode::Manual, true) });
        assert!(s.contains("Use tabs.") && s.find("How to work").unwrap() < s.find("Use tabs.").unwrap());
        let chat = system(&Context { chat: true, instructions: "--- X ---", ..ctx(AccessMode::Manual, true) });
        assert!(chat.contains("--- X ---"));
    }

    #[test]
    fn the_skill_index_is_in_the_standing_part_and_only_when_there_is_one() {
        let with = system(&Context { skills: "
Skills:
- review: d
", ..ctx(AccessMode::Manual, true) });
        assert!(with.contains("- review: d") && with.find("Skills:").unwrap() < with.find("proposed, not applied").unwrap());
        assert!(!system(&ctx(AccessMode::Manual, true)).contains("Skills:"));
        let chat = system(&Context { chat: true, skills: "", ..ctx(AccessMode::Manual, true) });
        assert!(!chat.contains("load_skill"));
    }

    #[test]
    fn git_is_described_only_when_it_is_on_and_the_ids_only_when_they_are_in_use() {
        let off = system(&ctx(AccessMode::Manual, true));
        assert!(!off.contains("corvus_") && !off.contains("Author-"));
        let on = system(&Context { git: true, ..ctx(AccessMode::Manual, true) });
        assert!(on.contains("corvus_commit") && on.contains("You cannot push") && !on.contains("Author-"));
        let hidden = system(&Context { git: true, hide_authors: true, ..ctx(AccessMode::Manual, true) });
        assert!(hidden.contains("Author-7f3a9c"));
    }

    #[test]
    fn the_plan_is_asked_for_only_when_it_is_on() {
        assert!(system(&ctx(AccessMode::Manual, true)).contains("update_plan"));
        assert!(!system(&Context { plan: false, ..ctx(AccessMode::Manual, true) }).contains("update_plan"));
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
        let s = system(&Context { projects: &cargo, ..ctx(AccessMode::Manual, true) });
        assert!(s.contains("rust-analyzer") && s.contains("bennu_test_run") && !s.contains("Bevy") && !s.contains("give them root"));
        let bevy = [project(&[("Cargo", ".")], true)];
        assert!(system(&Context { projects: &bevy, ..ctx(AccessMode::Manual, true) }).contains("bennu_catalog"));
        let node = [project(&[("Node", ".")], false)];
        let n = system(&Context { projects: &node, ..ctx(AccessMode::Manual, true) });
        assert!(n.contains("Node project") && n.contains("no tool to run npm") && !n.contains("rust-analyzer"));
        let java = [project(&[("Maven", ".")], false)];
        let j = system(&Context { projects: &java, ..ctx(AccessMode::Manual, true) });
        assert!(!j.contains("rust-analyzer") && !j.contains("Node project"));
    }

    #[test]
    fn a_project_with_both_ecosystems_is_told_where_each_one_lives() {
        let mixed = [project(&[("Node", "."), ("Cargo", "src-tauri")], false)];
        let s = system(&Context { projects: &mixed, ..ctx(AccessMode::Manual, true) });
        assert!(s.contains("There is Rust here") && s.contains("There is a Node project here"));
        assert!(s.contains("C:/p/src-tauri"), "the Rust tools must be pointed at the folder with the Cargo.toml");
        assert_eq!(s.matches("give them root").count(), 1, "only the Rust manifest is below the root");
        assert_eq!(s.matches("There is no Java index").count(), 1, "said once, not per ecosystem");
    }

    #[test]
    fn java_beside_rust_keeps_its_own_tools() {
        let both = [project(&[("Maven", "."), ("Cargo", "native")], false)];
        let s = system(&Context { projects: &both, ..ctx(AccessMode::Manual, true) });
        assert!(s.contains("There is Rust here") && !s.contains("There is no Java index") && s.contains("C:/p/native"));
    }

    #[test]
    fn the_wording_is_not_java_only() {
        let s = system(&ctx(AccessMode::Manual, true));
        assert!(s.contains("Java, Rust and web projects") && s.contains("is for Java only"));
    }

    #[test]
    fn the_live_paragraph_names_the_tools_and_appears_only_when_they_are_on() {
        let off = system(&ctx(AccessMode::Manual, true));
        assert!(!off.contains("run_recipe") && !off.contains("trace_request"));
        let on = system(&Context { live: true, ..ctx(AccessMode::Manual, true) });
        for tool in ["list_recipes", "run_recipe", "wait_ready", "http_request", "trace_request", "run_scenario", "run_stop", "log_facets", "log_record", "wait_log", "console_logs", "tail_file"] {
            assert!(on.contains(tool), "{tool}");
        }
        assert!(on.contains("only stop your own"), "the multi-chat rule is told");
        let chat = system(&Context { chat: true, live: true, ..ctx(AccessMode::Manual, true) });
        assert!(!chat.contains("run_recipe"), "a chat has no tools");
    }

    #[test]
    fn the_debug_paragraph_is_added_only_when_the_tools_are_on_and_names_them() {
        let base = system(&ctx(AccessMode::Manual, true));
        assert_eq!(with_debug(base.clone(), false), base);
        let on = with_debug(base.clone(), true);
        for tool in ["debug_set_breakpoint", "debug_launch", "debug_wait_stop", "debug_variables", "debug_evaluate", "debug_step", "debug_detach"] {
            assert!(on.contains(tool), "{tool}");
        }
        assert!(on.starts_with(&base) && on.contains("never leave it running"));
    }

    #[test]
    fn the_bevy_and_screenshot_paragraphs_appear_only_when_their_tools_are_on() {
        let base = system(&ctx(AccessMode::Manual, true));
        assert_eq!(with_bevy(base.clone(), false, true), base, "no read tools, no paragraph");
        let read = with_bevy(base.clone(), true, false);
        for tool in ["brp_connect", "brp_query", "brp_get", "run_logs", "run_stop"] {
            assert!(read.contains(tool), "{tool}");
        }
        assert!(!read.contains("brp_mutate"));
        let write = with_bevy(base.clone(), true, true);
        assert!(write.contains("brp_mutate") && write.contains("no undo"));
        assert_eq!(with_screenshots(base.clone(), false), base);
        assert!(with_screenshots(base, true).contains("page_diagnostics"));
    }

    #[test]
    fn an_isolated_copy_is_described_by_its_kind() {
        use bennu_assistant::prelude::WorktreeMode;
        assert!(!system(&ctx(AccessMode::Manual, true)).contains("isolated"));
        let copy = system(&Context { worktree: Some(WorktreeMode::Copy), ..ctx(AccessMode::Manual, true) });
        assert!(copy.contains("isolated text-only copy") && copy.contains("not available"));
        let git = system(&Context { worktree: Some(WorktreeMode::Git), ..ctx(AccessMode::Manual, true) });
        assert!(git.contains("isolated git worktree") && git.contains("all tools work"));
    }

    #[test]
    fn the_agent_and_the_chat_are_both_told_how_to_show_a_diagram() {
        for chat in [false, true] {
            let s = system(&Context { chat, ..ctx(AccessMode::Manual, true) });
            assert!(s.contains("`show_diagram`") && s.contains("Mermaid") && s.contains("fix the source"), "chat={chat}");
            assert!(s.contains("Never paste a ```mermaid block"), "chat={chat}");
        }
    }

    #[test]
    fn it_asks_for_markdown() {
        assert!(system(&ctx(AccessMode::Manual, true)).contains("Markdown"));
    }

    #[test]
    fn it_never_promises_a_shell() {
        assert!(system(&ctx(AccessMode::Manual, true)).contains("no shell"));
    }
}
