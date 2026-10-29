//! Skills: folders of instructions the user keeps for the assistant, in the format Claude Code uses.
//!
//! A skill is `<name>/SKILL.md`: a small YAML frontmatter (`name`, `description`; any other key is
//! ignored) and a Markdown body. Only the *index* — name and one line — is sent with every request;
//! the body is fetched by the `load_skill` tool when the model decides a task matches, so a library of
//! fifty skills costs a few hundred tokens, not fifty files.
//!
//! **Ids.** A skill is identified by `<scope>/<name>` with scope `user` or `project`. The folder it was
//! found in (`.bennu` or `.claude`, which project) is a *label*, not part of the id: moving a skill
//! between `.claude/skills` and `.bennu/skills`, or opening the project from another path, must not
//! silently switch it off. Within one scope a name is unique — `.bennu` beats `.claude`, and the first
//! project of the working set beats the others — and a project skill shadows a user skill of the same
//! name in the index (both stay listed in the manager; the shadowed one is marked).
//!
//! **Opt-in.** Nothing is enabled until the user ticks it: the set of enabled ids (`skills_on`) is an
//! allow-list, so a skill file dropped into a project never reaches the model on its own.
//!
//! No script is ever run: the body is text for the model, and the bundled files are only *named*.

use std::path::{Path, PathBuf};

/// The most skills that are kept; the rest are reported as invalid so the manager says why they are missing.
pub(super) const MAX_SKILLS: usize = 50;
/// The most of one body that is sent. A longer one is cut at a line, and the model is told.
pub(super) const MAX_BODY_BYTES: usize = 32 * 1024;
/// A description is one line in the index; a longer one is clipped.
pub(super) const MAX_DESCRIPTION_CHARS: usize = 240;
const MAX_NAME_CHARS: usize = 64;
/// A `SKILL.md` larger than this is not read at all: it is not a skill, it is a mistake.
const MAX_FILE_BYTES: u64 = 512 * 1024;
/// How many sibling file names are listed to the model.
const MAX_SIBLINGS: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Skill {
    /// `<scope>/<name>` — what is stored in settings.
    pub id: String,
    pub name: String,
    pub description: String,
    /// `user` or `project`.
    pub scope: &'static str,
    /// Where it was found, for the badge: `.bennu`, `.claude` (project or user), or `bennu` (user config).
    pub source: String,
    /// The project it belongs to, when it is a project skill.
    pub project: Option<String>,
    pub dir: PathBuf,
    pub body: String,
    /// The body was cut at `MAX_BODY_BYTES`.
    pub truncated: bool,
    /// Size of the `SKILL.md` on disk.
    pub bytes: u64,
    /// A skill of higher precedence has the same name; this one is not offered to the model.
    pub shadowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Invalid {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Default)]
pub(super) struct Discovery {
    pub skills: Vec<Skill>,
    pub invalid: Vec<Invalid>,
}

// ── Parsing ──────────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Parsed {
    pub name: Option<String>,
    pub description: Option<String>,
    pub body: String,
}

fn unquote(value: &str) -> String {
    let v = value.trim();
    for q in ['"', '\''] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
            let inner = &v[1..v.len() - 1];
            return if q == '"' { inner.replace("\\\"", "\"").replace("\\\\", "\\") } else { inner.replace("''", "'") };
        }
    }
    v.to_string()
}

/// The `key: value` pairs of a frontmatter. Values may be quoted, or a block scalar (`>`, `|`, with an
/// optional chomp marker) whose indented lines are joined by spaces — descriptions are one line to us.
fn frontmatter_pairs(lines: &[&str]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        let indented = line.starts_with(' ') || line.starts_with('\t');
        let Some((key, rest)) = line.split_once(':').filter(|_| !indented && !line.trim_start().starts_with('#')) else { continue };
        let key = key.trim().to_ascii_lowercase();
        let rest = rest.trim();
        let mut value = if rest.starts_with('>') || rest.starts_with('|') { String::new() } else { unquote(rest) };
        let block = rest.starts_with('>') || rest.starts_with('|') || rest.is_empty();
        if block {
            // Continuation lines: indented (or blank) lines that follow.
            let mut parts: Vec<&str> = Vec::new();
            while i < lines.len() && (lines[i].trim().is_empty() || lines[i].starts_with(' ') || lines[i].starts_with('\t')) {
                let t = lines[i].trim();
                if !t.is_empty() && !t.starts_with("- ") {
                    parts.push(t);
                }
                i += 1;
            }
            if !parts.is_empty() {
                value = unquote(&parts.join(" "));
            }
        }
        out.push((key, value));
    }
    out
}

/// Split a `SKILL.md` into the two keys we use and the body. No frontmatter is not an error here: the
/// caller decides what a missing name or description means.
pub(super) fn parse(text: &str) -> Parsed {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let text = text.replace("\r\n", "\n");
    let mut lines = text.split('\n');
    if lines.next().map(str::trim_end) != Some("---") {
        return Parsed { name: None, description: None, body: text.trim().to_string() };
    }
    let rest: Vec<&str> = lines.collect();
    let Some(end) = rest.iter().position(|l| l.trim_end() == "---") else {
        // An unterminated frontmatter: treat the whole file as body rather than guessing.
        return Parsed { name: None, description: None, body: text.trim().to_string() };
    };
    let pairs = frontmatter_pairs(&rest[..end]);
    let get = |k: &str| pairs.iter().find(|(key, _)| key == k).map(|(_, v)| v.trim().to_string()).filter(|v| !v.is_empty());
    Parsed { name: get("name"), description: get("description"), body: rest[end + 1..].join("\n").trim().to_string() }
}

/// A name the model can type back and a setting can store: ASCII letters, digits, `-` and `_`.
pub(super) fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && name.chars().count() <= MAX_NAME_CHARS
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// `description` on one line and at most `MAX_DESCRIPTION_CHARS`, with an ellipsis when it was cut.
pub(super) fn clip_description(description: &str) -> String {
    let one_line = description.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= MAX_DESCRIPTION_CHARS {
        return one_line;
    }
    let cut: String = one_line.chars().take(MAX_DESCRIPTION_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

/// `text` cut to at most `limit` bytes, at a line break when there is one, and whether it was cut.
fn clip_body(text: &str, limit: usize) -> (String, bool) {
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

// ── Discovery ────────────────────────────────────────────────────────────────────────────────────

/// A folder that holds skill folders, and how it is labelled.
pub(super) struct Source {
    pub dir: PathBuf,
    pub scope: &'static str,
    pub label: &'static str,
    pub project: Option<String>,
}

/// Where skills are looked for, highest precedence first: each project's `.bennu/skills` then
/// `.claude/skills` (projects in the order given), then the user's own folders.
pub(super) fn sources(roots: &[PathBuf], user_bennu: PathBuf, user_claude: Option<PathBuf>) -> Vec<Source> {
    let mut out = Vec::new();
    for root in roots {
        let project = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| root.display().to_string());
        for (rel, label) in [(".bennu/skills", ".bennu"), (".claude/skills", ".claude")] {
            out.push(Source { dir: root.join(rel), scope: "project", label, project: Some(project.clone()) });
        }
    }
    out.push(Source { dir: user_bennu, scope: "user", label: "bennu", project: None });
    if let Some(dir) = user_claude {
        out.push(Source { dir, scope: "user", label: ".claude", project: None });
    }
    out
}

/// The user's folders: Bennu's own config folder and `~/.claude/skills`.
pub(super) fn user_sources() -> (PathBuf, Option<PathBuf>) {
    let bennu = arbor_core::prelude::bennu_config_path("skills");
    let claude = arbor_core::prelude::user_home().map(|h| h.join(".claude").join("skills"));
    (bennu, claude)
}

fn read_skill(dir: &Path, source: &Source) -> Result<Skill, String> {
    let file = dir.join("SKILL.md");
    let meta = std::fs::metadata(&file).map_err(|e| format!("cannot read SKILL.md: {e}"))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!("SKILL.md is {} KB, over the {} KB limit", meta.len() / 1024, MAX_FILE_BYTES / 1024));
    }
    let bytes = std::fs::read(&file).map_err(|e| format!("cannot read SKILL.md: {e}"))?;
    let text = String::from_utf8(bytes).map_err(|_| "SKILL.md is not UTF-8 text".to_string())?;
    let parsed = parse(&text);
    let folder = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let name = parsed.name.unwrap_or(folder);
    if !valid_name(&name) {
        return Err(format!("`{name}` is not a usable name — letters, digits, `-` and `_`, up to {MAX_NAME_CHARS}"));
    }
    let description = parsed.description.map(|d| clip_description(&d)).ok_or("missing `description` in the frontmatter")?;
    if parsed.body.is_empty() {
        return Err("the skill has no instructions after the frontmatter".into());
    }
    let (body, truncated) = clip_body(&parsed.body, MAX_BODY_BYTES);
    Ok(Skill {
        id: format!("{}/{}", source.scope, name),
        name,
        description,
        scope: source.scope,
        source: source.label.to_string(),
        project: source.project.clone(),
        dir: dir.to_path_buf(),
        body,
        truncated,
        bytes: meta.len(),
        shadowed: false,
    })
}

/// Every skill under `sources`, in precedence order, with the problems found along the way.
pub(super) fn discover(sources: &[Source]) -> Discovery {
    let mut found = Discovery::default();
    for source in sources {
        let Ok(entries) = std::fs::read_dir(&source.dir) else { continue };
        let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        for dir in dirs {
            if !dir.join("SKILL.md").is_file() {
                continue;
            }
            match read_skill(&dir, source) {
                Ok(skill) => found.skills.push(skill),
                Err(reason) => found.invalid.push(Invalid { path: dir.join("SKILL.md"), reason }),
            }
        }
    }
    resolve_conflicts(&mut found);
    found
}

/// Apply the precedence rules and the cap. `skills` arrives in precedence order.
fn resolve_conflicts(found: &mut Discovery) {
    let mut kept: Vec<Skill> = Vec::new();
    for skill in std::mem::take(&mut found.skills) {
        if let Some(winner) = kept.iter().find(|k| k.id == skill.id) {
            found.invalid.push(Invalid {
                path: skill.dir.join("SKILL.md"),
                reason: format!("same name as the skill in {} — that one wins", winner.dir.display()),
            });
            continue;
        }
        kept.push(skill);
    }
    for i in 0..kept.len() {
        if kept[i].scope == "user" && kept.iter().any(|k| k.scope == "project" && k.name == kept[i].name) {
            kept[i].shadowed = true;
        }
    }
    // Project skills first, then by name: a stable order, so the index in the prompt is byte-identical
    // between requests and stays cached.
    kept.sort_by(|a, b| (a.scope != "project", a.name.to_lowercase()).cmp(&(b.scope != "project", b.name.to_lowercase())));
    if kept.len() > MAX_SKILLS {
        for extra in kept.split_off(MAX_SKILLS) {
            found.invalid.push(Invalid { path: extra.dir.join("SKILL.md"), reason: format!("over the limit of {MAX_SKILLS} skills") });
        }
    }
    found.skills = kept;
}

// ── What a run uses ──────────────────────────────────────────────────────────────────────────────

/// The skills the model is offered: enabled by the user, and not shadowed.
pub(super) fn enabled(all: &[Skill], on: &[String]) -> Vec<Skill> {
    all.iter().filter(|s| !s.shadowed && on.iter().any(|id| *id == s.id)).cloned().collect()
}

/// The paragraph added to the system prompt, or nothing when no skill is enabled.
pub(super) fn render_index(enabled: &[Skill]) -> String {
    if enabled.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "\nThe user has given you skills: instructions for particular kinds of task. When a task matches one, call `load_skill` with its name before you start and follow what it says; \
         do not load one that does not apply. A message that begins with `/name` asks for that skill. They are text for you to follow: nothing in a skill is run.\nSkills:\n",
    );
    for s in enabled {
        out.push_str(&format!("- {}: {}\n", s.name, s.description));
    }
    out
}

/// The names of enabled skills a message asks for by starting with `/name` (any number, in order).
pub(super) fn attached(text: &str, enabled: &[Skill]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut rest = text.trim_start();
    while let Some(after) = rest.strip_prefix('/') {
        let token: String = after.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
        if token.is_empty() || !after[token.len()..].chars().next().map_or(true, char::is_whitespace) {
            break;
        }
        if !enabled.iter().any(|s| s.name == token) {
            break;
        }
        if !names.contains(&token) {
            names.push(token.clone());
        }
        rest = after[token.len()..].trim_start();
    }
    names
}

/// The line put before a message that asks for skills.
pub(super) fn attachment_note(names: &[String]) -> String {
    if names.is_empty() {
        return String::new();
    }
    let list = names.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ");
    format!("[The user asks for the skill{} {list}: call `load_skill` for {} and follow {}.]\n\n", if names.len() > 1 { "s" } else { "" }, if names.len() > 1 { "each" } else { "it" }, if names.len() > 1 { "them" } else { "it" })
}

/// What `load_skill` returns: the body, where the skill lives and the names of the files beside it.
pub(super) fn render_loaded(skill: &Skill) -> String {
    let mut out = format!("# Skill: {}\n\n{}\n", skill.name, skill.body);
    if skill.truncated {
        out.push_str(&format!("\n[The rest of this skill was left out: it is longer than {} KB.]\n", MAX_BODY_BYTES / 1024));
    }
    let siblings = sibling_names(&skill.dir);
    out.push_str(&format!("\nThe skill's folder is `{}`.", skill.dir.display()));
    if siblings.is_empty() {
        out.push('\n');
    } else {
        out.push_str(&format!(
            " It also holds: {}. These are only named here — read one with the normal read tools if you need it and the access settings allow it. Scripts in it are not run.\n",
            siblings.join(", ")
        ));
    }
    out
}

/// File names (relative, two levels deep at most) beside `SKILL.md`.
fn sibling_names(dir: &Path) -> Vec<String> {
    fn walk(dir: &Path, prefix: &str, depth: usize, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            if out.len() >= MAX_SIBLINGS {
                return;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let rel = format!("{prefix}{name}");
            if e.path().is_dir() {
                if depth < 2 {
                    walk(&e.path(), &format!("{rel}/"), depth + 1, out);
                }
            } else if !(prefix.is_empty() && name.eq_ignore_ascii_case("SKILL.md")) {
                out.push(rel);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, "", 1, &mut out);
    out
}

/// The listing as the manager receives it.
pub(super) fn wire(found: &Discovery) -> serde_json::Value {
    use serde_json::json;
    json!({
        "skills": found.skills.iter().map(|s| json!({
            "id": s.id, "name": s.name, "description": s.description, "scope": s.scope, "source": s.source,
            "project": s.project, "path": s.dir.display().to_string(), "body": s.body, "truncated": s.truncated,
            "bytes": s.bytes, "shadowed": s.shadowed,
        })).collect::<Vec<_>>(),
        "invalid": found.invalid.iter().map(|i| json!({ "path": i.path.display().to_string(), "reason": i.reason })).collect::<Vec<_>>(),
        "max": MAX_SKILLS,
    })
}

// ── Tests ────────────────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A scratch folder that removes itself.
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir = std::env::temp_dir().join(format!("bennu-skills-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
        fn skill(&self, base: &str, folder: &str, content: &str) {
            let dir = self.0.join(base).join(folder);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("SKILL.md"), content).unwrap();
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn doc(name: &str, description: &str) -> String {
        format!("---\nname: {name}\ndescription: {description}\n---\nDo the thing.\n")
    }

    #[test]
    fn a_plain_frontmatter_is_read() {
        let p = parse("---\nname: review\ndescription: Review a change\nallowed-tools: Read\n---\n\n# Steps\n1. Look.\n");
        assert_eq!(p.name.as_deref(), Some("review"));
        assert_eq!(p.description.as_deref(), Some("Review a change"));
        assert_eq!(p.body, "# Steps\n1. Look.");
    }

    #[test]
    fn crlf_and_a_byte_order_mark_are_tolerated() {
        let p = parse("\u{feff}---\r\nname: a\r\ndescription: b\r\n---\r\nBody\r\nmore\r\n");
        assert_eq!((p.name.as_deref(), p.description.as_deref()), (Some("a"), Some("b")));
        assert_eq!(p.body, "Body\nmore");
    }

    #[test]
    fn quotes_are_removed() {
        let p = parse("---\nname: \"x\"\ndescription: 'It''s: fine'\n---\nb");
        assert_eq!(p.name.as_deref(), Some("x"));
        assert_eq!(p.description.as_deref(), Some("It's: fine"));
    }

    #[test]
    fn a_folded_description_is_joined_on_one_line() {
        let p = parse("---\nname: x\ndescription: >-\n  First part\n  second part\nother: 1\n---\nb");
        assert_eq!(p.description.as_deref(), Some("First part second part"));
    }

    #[test]
    fn no_frontmatter_means_no_keys_and_the_whole_text_as_body() {
        let p = parse("# Just notes\ntext");
        assert_eq!((p.name, p.description), (None, None));
        assert_eq!(p.body, "# Just notes\ntext");
        let open = parse("---\nname: x\nno end");
        assert_eq!(open.name, None);
    }

    #[test]
    fn a_missing_description_makes_the_skill_invalid() {
        let s = Scratch::new();
        s.skill("p/.claude/skills", "nodesc", "---\nname: nodesc\n---\nBody");
        let d = discover(&sources(&[s.0.join("p")], s.0.join("none"), None));
        assert!(d.skills.is_empty());
        assert!(d.invalid[0].reason.contains("description"), "{:?}", d.invalid);
    }

    #[test]
    fn the_folder_name_stands_in_for_a_missing_name_and_a_bad_name_is_rejected() {
        let s = Scratch::new();
        s.skill("p/.claude/skills", "from-folder", "---\ndescription: d\n---\nBody");
        s.skill("p/.claude/skills", "bad", "---\nname: ../evil\ndescription: d\n---\nBody");
        let d = discover(&sources(&[s.0.join("p")], s.0.join("none"), None));
        assert_eq!(d.skills.iter().map(|k| k.name.as_str()).collect::<Vec<_>>(), ["from-folder"]);
        assert_eq!(d.invalid.len(), 1);
        assert!(valid_name("a_b-1") && !valid_name("a b") && !valid_name("-a") && !valid_name(""));
    }

    #[test]
    fn a_long_description_is_clipped_to_one_line() {
        let long = format!("word {}", "x ".repeat(400));
        let clipped = clip_description(&long);
        assert!(clipped.chars().count() <= MAX_DESCRIPTION_CHARS && clipped.ends_with('…'));
        assert_eq!(clip_description("a\n  b"), "a b");
    }

    #[test]
    fn a_long_body_is_cut_at_a_line_and_flagged() {
        let s = Scratch::new();
        s.skill("p/.bennu/skills", "big", &format!("---\nname: big\ndescription: d\n---\n{}", "line\n".repeat(20_000)));
        let d = discover(&sources(&[s.0.join("p")], s.0.join("none"), None));
        assert!(d.skills[0].truncated && d.skills[0].body.len() <= MAX_BODY_BYTES);
        assert!(render_loaded(&d.skills[0]).contains("left out"));
    }

    #[test]
    fn a_project_beats_the_user_and_bennu_beats_claude() {
        let s = Scratch::new();
        s.skill("p/.bennu/skills", "same", &doc("same", "from bennu"));
        s.skill("p/.claude/skills", "same", &doc("same", "from claude"));
        s.skill("user", "same", &doc("same", "from user"));
        s.skill("user", "only-user", &doc("only-user", "u"));
        let d = discover(&sources(&[s.0.join("p")], s.0.join("user"), None));
        let same: Vec<_> = d.skills.iter().filter(|k| k.name == "same").collect();
        assert_eq!(same.len(), 2, "one project skill and one (shadowed) user skill: {:?}", d.skills);
        let project = same.iter().find(|k| k.scope == "project").unwrap();
        assert_eq!((project.source.as_str(), project.description.as_str(), project.shadowed), (".bennu", "from bennu", false));
        assert!(same.iter().find(|k| k.scope == "user").unwrap().shadowed);
        assert!(!d.skills.iter().find(|k| k.name == "only-user").unwrap().shadowed);
        assert_eq!(d.invalid.len(), 1, "the losing .claude copy is reported");
    }

    #[test]
    fn only_enabled_unshadowed_skills_reach_the_index_in_a_stable_order() {
        let s = Scratch::new();
        for n in ["zeta", "alpha"] {
            s.skill("p/.claude/skills", n, &doc(n, &format!("does {n}")));
        }
        s.skill("user", "mid", &doc("mid", "does mid"));
        s.skill("user", "alpha", &doc("alpha", "user alpha"));
        let d = discover(&sources(&[s.0.join("p")], s.0.join("user"), None));
        let on: Vec<String> = ["user/mid", "project/zeta", "project/alpha", "user/alpha"].iter().map(|x| x.to_string()).collect();
        let list = enabled(&d.skills, &on);
        assert_eq!(list.iter().map(|k| k.id.as_str()).collect::<Vec<_>>(), ["project/alpha", "project/zeta", "user/mid"]);
        let index = render_index(&list);
        assert!(index.contains("- alpha: does alpha") && !index.contains("user alpha") && index.contains("load_skill"));
        assert_eq!(render_index(&enabled(&d.skills, &[])), "", "nothing is on by default");
    }

    #[test]
    fn the_count_is_capped_and_the_overflow_reported() {
        let s = Scratch::new();
        for i in 0..(MAX_SKILLS + 3) {
            let n = format!("s{i:03}");
            s.skill("p/.claude/skills", &n, &doc(&n, "d"));
        }
        let d = discover(&sources(&[s.0.join("p")], s.0.join("none"), None));
        assert_eq!(d.skills.len(), MAX_SKILLS);
        assert_eq!(d.invalid.iter().filter(|i| i.reason.contains("limit")).count(), 3);
    }

    #[test]
    fn a_leading_slash_name_attaches_only_enabled_skills() {
        let s = Scratch::new();
        s.skill("p/.claude/skills", "review", &doc("review", "d"));
        s.skill("p/.claude/skills", "ship", &doc("ship", "d"));
        let d = discover(&sources(&[s.0.join("p")], s.0.join("none"), None));
        let on = enabled(&d.skills, &["project/review".into(), "project/ship".into()]);
        assert_eq!(attached("/review please", &on), ["review"]);
        assert_eq!(attached("  /review /ship do it", &on), ["review", "ship"]);
        assert!(attached("look at /review", &on).is_empty(), "only at the start");
        assert!(attached("/reviewer x", &on).is_empty() && attached("/unknown x", &on).is_empty());
        assert!(attachment_note(&["review".into()]).contains("load_skill"));
    }

    #[test]
    fn a_loaded_skill_names_its_folder_and_siblings_but_not_itself() {
        let s = Scratch::new();
        s.skill("p/.claude/skills", "withfiles", &doc("withfiles", "d"));
        let dir = s.0.join("p/.claude/skills/withfiles");
        std::fs::write(dir.join("template.md"), "x").unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("scripts/run.sh"), "x").unwrap();
        let d = discover(&sources(&[s.0.join("p")], s.0.join("none"), None));
        let text = render_loaded(&d.skills[0]);
        assert!(text.contains("template.md") && text.contains("scripts/run.sh") && !text.contains("SKILL.md,"));
        assert!(text.contains("not run"));
    }
}
