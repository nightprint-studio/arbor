//! Editing: the read-modify-write door every file change goes through (applied or proposed by mode), and the
//! tools built on it — `edit_file`, `create_file`, `delete_file` and the two index-backed refactorings.

use super::*;

impl ToolHost {
    pub(super) fn run_edit(&self, input: &Value) -> Result<String, String> {
        let s = |k: &str| input.get(k).and_then(Value::as_str).ok_or_else(|| format!("`{k}` is required"));
        let file = self.resolve(s("file")?);
        let (old_text, new_text) = (s("old_text")?, s("new_text")?);
        let expected = input.get("expected_count").and_then(Value::as_u64).map(|n| n as usize);

        let (text, stamp) = self.read_text(&file)?;
        let next = text_tools::replace(&text, old_text, new_text, expected)?;
        self.submit(&file, &text, next, Some(stamp), None)
    }

    pub(super) fn run_create(&self, input: &Value) -> Result<String, String> {
        let file = self.resolve(input.get("file").and_then(Value::as_str).ok_or("`file` is required")?);
        let text = input.get("text").and_then(Value::as_str).ok_or("`text` is required")?;
        if file.exists() {
            return Err(format!("{} already exists — use edit_file to change it", file.display()));
        }
        if let Some(parent) = file.parent() {
            // A proposal must not create folders before the user has agreed to the file.
            if self.session.applies_edits() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        self.submit(&file, "", text.to_string(), None, None)
    }

    /// Apply an edit or hold it for review, by mode.
    pub(super) fn submit(&self, file: &Path, old: &str, new: String, stamp: Option<String>, move_to: Option<String>) -> Result<String, String> {
        let path = file.display().to_string();
        let edit = PendingEdit {
            root: self.root_of(file),
            label: self.display_path(&path),
            file: path,
            new_text: new.clone(),
            stamp,
            move_to,
            base: old.to_string(),
        };
        let delta = compare_text(old, &new, 3);
        if delta.identical {
            return Err("the edit changes nothing".into());
        }
        // Held from the first change until the run ends, so a second conversation cannot edit it meanwhile.
        self.reserve_files(&[file])?;
        // For the run's closing summary: every file the run changed or proposed, once.
        self.session.live_log().file(&self.display_path(&edit.file));
        if self.session.applies_edits() {
            self.write_text(&edit)?;
            if let Some(to) = &edit.move_to {
                self.call_handler("bennu_rename_path", json!({"args": {"root": edit.root, "file": edit.file, "new_path": to}}))?;
            }
            self.record_applied(file, old, &new);
            return Ok(format!("applied: {} (+{} −{})", file.display(), delta.added, delta.removed));
        }
        let contested = self.propose(edit, old);
        let warning = if contested {
            " Another proposal that has not been decided also edits this file: whichever the user applies first makes the other outdated, so keep this change small and do not assume it lands."
        } else {
            ""
        };
        Ok(format!(
            "proposed, not applied: {} (+{} −{}). The user reviews proposed changes themselves; do not assume this one is in the file until a later message says so.{warning}",
            file.display(), delta.added, delta.removed
        ))
    }

    /// How a file is named in a card or a receipt: relative to the project the user is in, `name:path` in
    /// another of the projects in play, as it is otherwise.
    pub(super) fn display_path(&self, file: &str) -> String {
        if let Ok(rel) = Path::new(file).strip_prefix(self.project()) {
            return rel.to_string_lossy().replace('\\', "/");
        }
        projects::labelled(&self.session.projects, Path::new(file)).unwrap_or_else(|| file.to_string())
    }

    pub(super) fn file_entry(&self, file: &str, old: &str, new: &str) -> Value {
        let delta = compare_text(old, new, 3);
        json!({ "path": self.display_path(file), "added": delta.added, "removed": delta.removed, "hunks": delta.hunks })
    }

    /// An edit applied in auto mode still leaves a card: what changed stays visible in the thread.
    pub(super) fn record_applied(&self, file: &Path, old: &str, new: &str) {
        let id = unique_id("pa");
        let entry = self.file_entry(&file.display().to_string(), old, new);
        self.session.view().push(json!({ "type": "proposal", "proposal": { "id": id, "status": "applied", "files": [entry] } }));
        self.session.push_message();
    }

    /// Gather a proposed edit into this run's proposal card.
    /// Returns whether another undecided proposal also edits this file.
    pub(super) fn propose(&self, mut edit: PendingEdit, old: &str) -> bool {
        let mut entry = self.file_entry(&edit.file, old, &edit.new_text);
        if let Some(to) = &edit.move_to {
            // The card says the file moves too, not only that its text changes.
            let name = Path::new(to).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| to.clone());
            entry["path"] = json!(format!("{} → {name}", self.display_path(&edit.file)));
        }
        edit.label = entry["path"].as_str().unwrap_or_default().to_string();
        let existing = self.session.proposal_id.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let id = match existing {
            Some(id) => id,
            None => {
                let id = format!("pr-{}", self.session.run_id);
                *self.session.proposal_id.lock().unwrap_or_else(|p| p.into_inner()) = Some(id.clone());
                self.session.view().push(json!({
                    "type": "proposal",
                    "proposal": { "id": id, "status": "pending", "files": [], "conversationId": self.session.conversation_id },
                }));
                id
            }
        };
        let contested = runs::pending_elsewhere(&edit.file, &id);
        if contested {
            entry["shared"] = json!(true);
        }
        self.session.view().update("/proposal/id", &id, |b| {
            if let Some(files) = b["proposal"]["files"].as_array_mut() {
                // The same file edited twice replaces its entry: the card shows the file's final change.
                files.retain(|f| f["path"] != entry["path"]);
                files.push(entry.clone());
            }
        });
        let mut all = runs::take_proposal(&id).unwrap_or_default();
        all.retain(|e| e.file != edit.file);
        all.push(edit);
        runs::store_proposal(&id, all);
        self.session.push_message();
        contested
    }

    /// The position a refactoring tool was pointed at, as a byte offset into the file's text.
    pub(super) fn position(&self, input: &Value) -> Result<(PathBuf, String, String, usize), String> {
        let raw = input.get("file").and_then(Value::as_str).ok_or("`file` is required")?;
        let file = self.resolve(raw);
        let line = input.get("line").and_then(Value::as_u64).ok_or("`line` is required")? as usize;
        let column = input.get("column").and_then(Value::as_u64).unwrap_or(1) as usize;
        let (text, stamp) = self.read_text(&file)?;
        let offset = text_tools::byte_offset(&text, line, column)?;
        Ok((file, text, stamp, offset))
    }

    pub(super) fn run_rename(&self, input: &Value) -> Result<String, String> {
        let new_name = input.get("new_name").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).ok_or("`new_name` is required")?;
        let (file, text, _stamp, offset) = self.position(input)?;
        let applied = self.call_handler(
            "bennu_rename_apply",
            json!({"args": {"file": file.display().to_string(), "source": text, "offset": offset, "new_name": new_name}}),
        )?;
        let edits = edits_of(&applied);
        if edits.is_empty() {
            return Err("nothing to rename there — the position is not on a renameable name, or the project index is still building. Check the line and column against the file.".into());
        }
        let moved = match (applied.pointer("/file_rename/from").and_then(Value::as_str), applied.pointer("/file_rename/to").and_then(Value::as_str)) {
            (Some(from), Some(to)) => Some((from.to_string(), to.to_string())),
            _ => None,
        };
        self.apply_plan(edits, moved, &format!("rename to {new_name}"))
    }

    pub(super) fn run_add_parameter(&self, input: &Value) -> Result<String, String> {
        let s = |k: &str| input.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).ok_or_else(|| format!("`{k}` is required"));
        let (type_text, name) = (s("type")?, s("name")?);
        let (file, text, _stamp, offset) = self.position(input)?;
        if file.extension().map(|e| !e.eq_ignore_ascii_case("java")).unwrap_or(true) {
            return Err("add_parameter works on Java methods only. For this file use edit_files, or rename_symbol to rename.".into());
        }
        let mut args = json!({
            "file": file.display().to_string(), "source": text, "offset": offset,
            "type_text": type_text, "name": name,
        });
        if let Some(v) = input.get("default_value").and_then(Value::as_str) {
            args["default_value"] = json!(v);
        }
        if let Some(b) = input.get("before").and_then(Value::as_u64) {
            args["before"] = json!(b);
        }
        let plan = self.call_handler("bennu_add_parameter_plan", json!({ "args": args }))?;
        if plan.is_null() {
            return Err("no method of this project is declared or called there, or the project index is still building. Check the line and column.".into());
        }
        if let Some(why) = plan.pointer("/preview/blocked").and_then(Value::as_str) {
            return Err(format!("not safe to do: {why}"));
        }
        let edits = edits_of(&plan["preview"]);
        if edits.is_empty() {
            return Err("the plan has no edits".into());
        }
        self.apply_plan(edits, None, &format!("add parameter {type_text} {name}"))
    }

    /// Apply a refactoring plan — spans grouped by file — as one change: every file the plan touches is
    /// checked against the access policy first, every file's new text is computed before any is
    /// written, and then each goes through the same door as an `edit_file` (proposed or applied by mode).
    pub(super) fn apply_plan(&self, edits: Vec<(String, Span)>, file_move: Option<(String, String)>, what: &str) -> Result<String, String> {
        let mut by_file: Vec<(String, Vec<Span>)> = Vec::new();
        for (file, span) in edits {
            match by_file.iter_mut().find(|(f, _)| *f == file) {
                Some((_, spans)) => spans.push(span),
                None => by_file.push((file, vec![span])),
            }
        }
        // All of it or none of it: a refactoring that lands in half the files is a broken build.
        for (file, _) in &by_file {
            self.authorize(&self.resolve(file), Op::Write)?;
        }
        // The whole set or none of it: another conversation holding one file refuses the refactoring up front.
        let resolved: Vec<PathBuf> = by_file.iter().map(|(f, _)| self.resolve(f)).collect();
        self.reserve_files(&resolved.iter().map(PathBuf::as_path).collect::<Vec<_>>())?;
        let mut prepared = Vec::new();
        for (file, spans) in &by_file {
            let (old, stamp) = self.read_text(&self.resolve(file))?;
            let new = text_tools::apply_spans(&old, spans).map_err(|e| format!("{file}: {e}"))?;
            prepared.push((file.clone(), old, new, stamp));
        }
        let mut lines = Vec::new();
        for (file, old, new, stamp) in prepared {
            // A public type's file moves with it: the move rides on the edit of the file it applies to.
            let moved = file_move.as_ref().filter(|(from, _)| same_path(from, &file)).map(|(_, to)| to.clone());
            lines.push(self.submit(&self.resolve(&file), &old, new, Some(stamp), moved)?);
        }
        Ok(format!("{what}: {} file(s).\n{}", lines.len(), lines.join("\n")))
    }

    pub(super) fn run_delete(&self, input: &Value) -> Result<String, String> {
        let paths: Vec<String> = input
            .get("paths")
            .and_then(Value::as_array)
            .ok_or("`paths` is required")?
            .iter()
            .filter_map(Value::as_str)
            .map(|p| self.resolve(p).display().to_string())
            .collect();
        if paths.is_empty() {
            return Err("no paths given".into());
        }
        // The assistant's own temp folder is scratch: removed outright, with no history and no reservation.
        let (temp, paths): (Vec<String>, Vec<String>) = {
            let policy = self.session.policy();
            paths.into_iter().partition(|p| policy.is_temp(&bennu_assistant::prelude::normalize(Path::new(p))))
        };
        let mut results = Vec::new();
        for p in &temp {
            let path = Path::new(p);
            let outcome = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
            outcome.map_err(|e| format!("{p}: {e}"))?;
            results.push(format!("{p} (temp, not in Local History)"));
        }
        if paths.is_empty() {
            return Ok(format!("deleted: {}", results.join(", ")));
        }
        let held: Vec<&Path> = paths.iter().map(Path::new).collect();
        self.reserve_files(&held)?;
        // Each project records its own history, so the paths go to the project that owns them.
        let mut by_root: Vec<(String, Vec<String>)> = Vec::new();
        for p in paths {
            let root = self.root_of(Path::new(&p));
            match by_root.iter_mut().find(|(r, _)| *r == root) {
                Some((_, list)) => list.push(p),
                None => by_root.push((root, vec![p])),
            }
        }
        for (root, list) in by_root {
            results.push(self.call_handler("bennu_delete_paths", json!({"args": {"root": root, "paths": list}}))?.to_string());
        }
        Ok(format!("deleted (kept in Local History, restorable): {}", results.join(", ")))
    }
}

/// The `edits` array of a rename-apply or add-parameter reply, as spans grouped later by file.
fn edits_of(value: &Value) -> Vec<(String, Span)> {
    // A rename-apply reply has `edits`; an add-parameter plan has them nested per file.
    let flat: Vec<&Value> = match value.get("edits").and_then(Value::as_array) {
        Some(list) => list.iter().collect(),
        None => value
            .get("files")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|f| f.get("edits").and_then(Value::as_array).into_iter().flatten())
            .collect(),
    };
    flat.into_iter()
        .filter_map(|e| {
            Some((
                e.get("file")?.as_str()?.to_string(),
                Span {
                    start: e.get("start")?.as_u64()? as usize,
                    end: e.get("end")?.as_u64()? as usize,
                    old: e.get("old").and_then(Value::as_str).unwrap_or("").to_string(),
                    new_text: e.get("new_text")?.as_str()?.to_string(),
                },
            ))
        })
        .collect()
}

/// Whether two paths name the same file, whatever the separators and (on Windows) the case.
fn same_path(a: &str, b: &str) -> bool {
    a.replace('\\', "/").eq_ignore_ascii_case(&b.replace('\\', "/"))
}
