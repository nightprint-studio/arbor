//! Authors the model never sees by name.
//!
//! Git results are full of people: the author of every commit and every blamed line, in names and in e-mail
//! addresses, and again inside messages (`Signed-off-by:`, `Co-authored-by:`). Sending those to the provider
//! tells it who works on the project, which the work being asked about does not need. So each person gets a
//! stable made-up id, `Author-7f3a9c`, and the model is only ever shown that.
//!
//! It is **reversible**, which is what makes it usable rather than merely safe. The mapping stays on this
//! machine; whenever an id comes back — in what the model writes to the user, or in an argument it sends to a
//! tool (a commit message that says `Co-authored-by: Author-7f3a9c`) — it is replaced with the real name
//! before it is shown or acted on. The model can talk about "Author-7f3a9c" and the user reads the name.
//!
//! The id is derived from the name and a random salt kept beside the mapping: the same person is always the
//! same id (so "who wrote most of this" still works), and an id does not give the name back to anyone
//! without the file.

use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Keys of a git result whose string values are people. Their values are collected as names to hide.
const PERSON_KEYS: [&str; 6] = ["author", "committer", "author_name", "committer_name", "author_email", "email"];

const PREFIX: &str = "Author-";

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct Authors {
    salt: String,
    /// id → the real name or address it stands for.
    ids: BTreeMap<String, String>,
    /// Set when something was added since it was loaded.
    #[serde(skip)]
    dirty: bool,
}

fn email_re() -> Regex {
    Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").expect("a valid pattern")
}

fn id_re() -> Regex {
    Regex::new(r"Author-[0-9a-f]{6}").expect("a valid pattern")
}

pub(super) fn file() -> PathBuf {
    // Not `.json`: the conversations beside it are, and are listed by that extension.
    arbor_core::prelude::bennu_data_dir().join("assistant").join("authors.map")
}

impl Authors {
    /// A new mapping with a fresh salt.
    fn fresh() -> Self {
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(1);
        let mut h = DefaultHasher::new();
        (seed, std::process::id()).hash(&mut h);
        Self { salt: format!("{:016x}", h.finish()), ..Default::default() }
    }

    pub(super) fn load() -> Self {
        std::fs::read_to_string(file())
            .ok()
            .and_then(|t| serde_json::from_str::<Authors>(&t).ok())
            .unwrap_or_else(Self::fresh)
    }

    pub(super) fn save(&mut self) {
        if !self.dirty {
            return;
        }
        let path = file();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(self) {
            if std::fs::write(&path, text).is_ok() {
                self.dirty = false;
            }
        }
    }

    /// The id standing for `real`: always the same for the same text, and never shared by two.
    pub(super) fn id_for(&mut self, real: &str) -> String {
        let real = real.trim();
        let mut round = 0u32;
        loop {
            let mut h = DefaultHasher::new();
            (&self.salt, real.to_lowercase(), round).hash(&mut h);
            let id = format!("{PREFIX}{:06x}", h.finish() & 0xff_ffff);
            match self.ids.get(&id) {
                None => {
                    self.ids.insert(id.clone(), real.to_string());
                    self.dirty = true;
                    return id;
                }
                Some(existing) if existing.eq_ignore_ascii_case(real) => return id,
                // Two names landed on one id: take the next one for the newcomer.
                Some(_) => round += 1,
            }
        }
    }

    /// Hide every address, and each of `names`, in `text`.
    pub(super) fn scrub(&mut self, text: &str, names: &[String]) -> String {
        let mut out = text.to_string();
        // Addresses first: a name can be inside one (`jdoe@x.org`), and the address is the longer match.
        let found: Vec<String> = email_re().find_iter(&out).map(|m| m.as_str().to_string()).collect();
        for e in found {
            let id = self.id_for(&e);
            out = out.replace(&e, &id);
        }
        // Longest names first, so "Mario Rossi" goes before "Mario".
        let mut sorted: Vec<&String> = names.iter().filter(|n| n.trim().len() >= 3).collect();
        sorted.sort_by_key(|n| std::cmp::Reverse(n.len()));
        for n in sorted {
            if out.contains(n.as_str()) {
                let id = self.id_for(n);
                out = out.replace(n.as_str(), &id);
            }
        }
        out
    }

    /// Replace every known id in `text` with the real name.
    pub(super) fn restore(&self, text: &str) -> String {
        if !text.contains(PREFIX) {
            return text.to_string();
        }
        id_re()
            .replace_all(text, |c: &regex::Captures| self.ids.get(&c[0]).cloned().unwrap_or_else(|| c[0].to_string()))
            .into_owned()
    }

    /// Replace each name this mapping already knows in `text` with its id — for what the user typed, so
    /// that a name they use is not the one thing the provider is told.
    pub(super) fn hide_known(&self, text: &str) -> String {
        let mut out = text.to_string();
        let mut known: Vec<(&String, &String)> = self.ids.iter().collect();
        known.sort_by_key(|(_, real)| std::cmp::Reverse(real.len()));
        for (id, real) in known {
            if real.len() >= 3 && out.contains(real.as_str()) {
                out = out.replace(real.as_str(), id);
            }
        }
        out
    }
}

/// The people named in a JSON value: the string values under person keys, anywhere inside it.
fn collect_names(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(map) => {
            for (k, val) in map {
                if PERSON_KEYS.contains(&k.as_str()) {
                    match val {
                        Value::String(s) if !s.trim().is_empty() => out.push(s.clone()),
                        Value::Array(a) => out.extend(a.iter().filter_map(|x| x.as_str().map(str::to_string))),
                        _ => {}
                    }
                }
                collect_names(val, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect_names(x, out)),
        _ => {}
    }
}

fn map_strings(v: &mut Value, f: &mut dyn FnMut(&str) -> String) {
    match v {
        Value::String(s) => *s = f(s),
        Value::Array(a) => a.iter_mut().for_each(|x| map_strings(x, f)),
        Value::Object(m) => m.values_mut().for_each(|x| map_strings(x, f)),
        _ => {}
    }
}

/// A tool's result with its people replaced by ids.
pub(super) fn scrub_value(authors: &mut Authors, value: &mut Value) {
    let mut names = Vec::new();
    collect_names(value, &mut names);
    map_strings(value, &mut |s| authors.scrub(s, &names));
}

/// The arguments of a tool call with any id the model wrote replaced by the real name.
pub(super) fn restore_value(authors: &Authors, value: &mut Value) {
    map_strings(value, &mut |s| authors.restore(s));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn authors() -> Authors {
        Authors { salt: "pepper".into(), ..Default::default() }
    }

    #[test]
    fn the_same_person_is_always_the_same_id_and_two_people_are_not() {
        let mut a = authors();
        let mario = a.id_for("Mario Rossi");
        assert_eq!(a.id_for("Mario Rossi"), mario);
        assert_eq!(a.id_for("mario rossi"), mario, "case does not make a second person");
        assert_ne!(a.id_for("Luisa Bianchi"), mario);
        assert!(mario.starts_with("Author-") && mario.len() == "Author-".len() + 6);
    }

    #[test]
    fn an_id_goes_back_to_the_name_it_stands_for() {
        let mut a = authors();
        let id = a.id_for("Mario Rossi");
        assert_eq!(a.restore(&format!("{id} wrote this, and {id} fixed it")), "Mario Rossi wrote this, and Mario Rossi fixed it");
    }

    #[test]
    fn an_id_nobody_assigned_is_left_alone() {
        assert_eq!(authors().restore("see Author-abcdef"), "see Author-abcdef");
    }

    #[test]
    fn addresses_and_names_inside_text_are_hidden() {
        let mut a = authors();
        let out = a.scrub("Fix NPE\n\nSigned-off-by: Mario Rossi <m.rossi@acme.it>", &["Mario Rossi".to_string()]);
        assert!(!out.contains("Mario") && !out.contains("acme.it") && !out.contains("m.rossi"), "{out}");
        assert!(out.contains("Signed-off-by: Author-"), "{out}");
    }

    #[test]
    fn a_result_is_scrubbed_everywhere_including_under_other_keys() {
        let mut a = authors();
        let mut v = json!([
            {"commit": "abc", "author": "Mario Rossi", "summary": "Revert Mario Rossi's change", "date": "2026-01-02"},
            {"commit": "def", "author": "Luisa Bianchi", "summary": "Add tests, thanks luisa@acme.it"},
        ]);
        scrub_value(&mut a, &mut v);
        let text = v.to_string();
        assert!(!text.contains("Mario") && !text.contains("Luisa") && !text.contains("acme.it"), "{text}");
        assert_eq!(v[0]["commit"], "abc", "what is not a person is untouched");
        assert_eq!(v[0]["date"], "2026-01-02");
    }

    #[test]
    fn what_the_model_writes_back_is_restored_before_a_tool_sees_it() {
        let mut a = authors();
        let id = a.id_for("Mario Rossi");
        let mut args = json!({"message": format!("Fix\n\nCo-authored-by: {id}"), "paths": ["a.java"]});
        restore_value(&a, &mut args);
        assert_eq!(args["message"], "Fix\n\nCo-authored-by: Mario Rossi");
        assert_eq!(args["paths"][0], "a.java");
    }

    #[test]
    fn a_name_the_user_types_is_hidden_once_it_is_known() {
        let mut a = authors();
        let id = a.id_for("Mario Rossi");
        assert_eq!(a.hide_known("who is Mario Rossi?"), format!("who is {id}?"));
        assert_eq!(a.hide_known("who is Anna?"), "who is Anna?", "an unknown name is left as typed");
    }

    #[test]
    fn longer_names_win_over_their_parts() {
        let mut a = authors();
        let out = a.scrub("Mario Rossi and Mario", &["Mario".to_string(), "Mario Rossi".to_string()]);
        let id_full = a.id_for("Mario Rossi");
        assert!(out.starts_with(&id_full), "{out}");
    }

    #[test]
    fn a_collision_gives_the_newcomer_another_id() {
        let mut a = authors();
        // Force the id a name would get to be taken by someone else.
        let wanted = {
            let mut probe = authors();
            probe.id_for("Anna Verdi")
        };
        a.ids.insert(wanted.clone(), "Someone Else".into());
        let got = a.id_for("Anna Verdi");
        assert_ne!(got, wanted);
        assert_eq!(a.restore(&got), "Anna Verdi");
        assert_eq!(a.restore(&wanted), "Someone Else");
    }
}
