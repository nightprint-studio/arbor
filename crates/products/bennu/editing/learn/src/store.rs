//! The learner: counters per (origin, context), kept in a folder the host names, behind two switches.
//!
//! Two files, both plain JSON so the user can read them or delete them:
//!
//! * `learn.json` — the counters. This is all the ranking ever reads, and it holds no code: an origin tag, a
//!   normalised context and weights.
//! * `events.jsonl` — the raw ghost and what it became, one per line, **only** when `keep_text` is on. It exists
//!   for the day a model is retrained on corrections, and is capped so it cannot grow without bound.
//!
//! Off by default: with `enabled` false nothing is recorded and nothing is written. [`Learner::reset`] removes both.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use serde::Serialize;

use crate::edit::{classify, Outcome};
use crate::tally::Tally;

const COUNTERS_FILE: &str = "learn.json";
const EVENTS_FILE: &str = "events.jsonl";
/// The raw log is cut to its newer half when it passes this many bytes.
const EVENTS_CAP_BYTES: u64 = 1 << 20;
/// More counters than this and the one that has seen least is dropped: contexts are names a project writes, and a
/// file that grows with every new method name would be a leak of its own.
const MAX_KEYS: usize = 4000;
/// Separates origin from context in a counter's key; neither may contain it.
const KEY_SEP: char = '\t';

/// The two switches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// Record anything at all.
    pub enabled: bool,
    /// Also keep the raw ghost and its outcome in `events.jsonl`.
    pub keep_text: bool,
}

/// One suggestion and what the user did with it.
#[derive(Clone, Debug)]
pub struct Event<'a> {
    /// Which layer produced it (`habit`, `model`, `creation`, …).
    pub origin: &'a str,
    /// What it was suggested for, normalised by the host (a type and a parameter name, never a path).
    pub context: &'a str,
    pub ghost: &'a str,
    /// What stood where the ghost was drawn once the user was done; `None` if it was never accepted.
    pub after: Option<&'a str>,
}

#[derive(Serialize)]
struct RawLine<'a> {
    at: u64,
    origin: &'a str,
    context: &'a str,
    ghost: &'a str,
    after: Option<&'a str>,
    outcome: Outcome,
}

pub struct Learner {
    dir: PathBuf,
    settings: Settings,
    tallies: HashMap<String, Tally>,
    dirty: bool,
}

impl Learner {
    /// Open the learner kept in `dir`. A missing or unreadable counters file is an empty one: learning starts over,
    /// it does not fail.
    pub fn open(dir: impl Into<PathBuf>, settings: Settings) -> Self {
        let dir = dir.into();
        let tallies = fs::read_to_string(dir.join(COUNTERS_FILE))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { dir, settings, tallies, dirty: false }
    }

    pub fn settings(&self) -> Settings {
        self.settings
    }

    pub fn set_settings(&mut self, settings: Settings) {
        self.settings = settings;
    }

    /// Count one event. Returns whether anything was recorded (false when switched off).
    pub fn record(&mut self, event: &Event<'_>, now: u64) -> bool {
        if !self.settings.enabled || event.origin.contains(KEY_SEP) || event.context.contains(KEY_SEP) {
            return false;
        }
        let outcome = classify(event.ghost, event.after);
        self.tallies.entry(key(event.origin, event.context)).or_default().add(&outcome, now);
        self.dirty = true;
        if self.tallies.len() > MAX_KEYS {
            self.drop_least_seen();
        }
        if self.settings.keep_text {
            // A failed append loses a training example, not a counter: the counters are already updated.
            let _ = self.append_raw(event, outcome, now);
        }
        true
    }

    /// The counters of one origin in one context, if there are any.
    pub fn tally(&self, origin: &str, context: &str) -> Option<&Tally> {
        self.tallies.get(&key(origin, context))
    }

    /// Every counter, as `(origin, context, tally)`, for a page that shows what was learned.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str, &Tally)> {
        self.tallies.iter().filter_map(|(key, tally)| {
            let (origin, context) = key.split_once(KEY_SEP)?;
            Some((origin, context, tally))
        })
    }

    /// Forget one counter. `true` when there was one.
    pub fn forget(&mut self, origin: &str, context: &str) -> bool {
        let removed = self.tallies.remove(&key(origin, context)).is_some();
        self.dirty |= removed;
        removed
    }

    /// How useful this kind of suggestion has been in this context, once enough have been seen there; else over every
    /// context of the origin. `None` when neither has enough to say.
    pub fn useful_rate(&self, origin: &str, context: &str) -> Option<f32> {
        self.tally(origin, context).and_then(Tally::useful_rate).or_else(|| self.origin_tally(origin).useful_rate())
    }

    fn drop_least_seen(&mut self) {
        let least = self.tallies.iter().min_by(|a, b| a.1.total().total_cmp(&b.1.total())).map(|(k, _)| k.clone());
        if let Some(k) = least {
            self.tallies.remove(&k);
        }
    }

    /// Everything one origin has seen, over every context.
    pub fn origin_tally(&self, origin: &str) -> Tally {
        let prefix = format!("{origin}{KEY_SEP}");
        let mut sum = Tally::default();
        for (_, t) in self.tallies.iter().filter(|(k, _)| k.starts_with(&prefix)) {
            sum.ignored += t.ignored;
            sum.kept += t.kept;
            sum.reformatted += t.reformatted;
            sum.shortened += t.shortened;
            sum.extended += t.extended;
            sum.replaced += t.replaced;
            sum.undone += t.undone;
        }
        sum
    }

    /// Write the counters if they changed. Atomic: a crash leaves the old file, not half of a new one.
    pub fn flush(&mut self) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        fs::create_dir_all(&self.dir)?;
        let tmp = self.dir.join(format!("{COUNTERS_FILE}.tmp"));
        fs::write(&tmp, serde_json::to_vec(&self.tallies).map_err(io::Error::other)?)?;
        fs::rename(&tmp, self.dir.join(COUNTERS_FILE))?;
        self.dirty = false;
        Ok(())
    }

    /// Forget everything: the counters in memory and both files on disk.
    pub fn reset(&mut self) -> io::Result<()> {
        self.tallies.clear();
        self.dirty = false;
        for name in [COUNTERS_FILE, EVENTS_FILE] {
            match fs::remove_file(self.dir.join(name)) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(())
    }

    fn append_raw(&self, event: &Event<'_>, outcome: Outcome, now: u64) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(EVENTS_FILE);
        self.trim_raw(&path)?;
        let line = RawLine { at: now, origin: event.origin, context: event.context, ghost: event.ghost, after: event.after, outcome };
        let mut file = fs::OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{}", serde_json::to_string(&line).map_err(io::Error::other)?)
    }

    /// Keep the newer half of the raw log once it is too big, starting on a whole line.
    fn trim_raw(&self, path: &std::path::Path) -> io::Result<()> {
        let Ok(meta) = fs::metadata(path) else { return Ok(()) };
        if meta.len() <= EVENTS_CAP_BYTES {
            return Ok(());
        }
        let text = fs::read_to_string(path)?;
        let mut from = text.len() / 2;
        while !text.is_char_boundary(from) {
            from += 1;
        }
        let newer = text[from..].split_once('\n').map_or("", |(_, rest)| rest);
        fs::write(path, newer)
    }
}

fn key(origin: &str, context: &str) -> String {
    format!("{origin}{KEY_SEP}{context}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn scratch() -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!("bennu-learn-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn on() -> Settings {
        Settings { enabled: true, keep_text: false }
    }

    fn kept<'a>(origin: &'a str, context: &'a str) -> Event<'a> {
        Event { origin, context, ghost: "foo()", after: Some("foo()") }
    }

    #[test]
    fn switched_off_it_records_and_writes_nothing() {
        let dir = scratch();
        let mut learner = Learner::open(&dir, Settings::default());
        assert!(!learner.record(&kept("habit", "Foo.bar"), 10));
        learner.flush().unwrap();
        assert!(!dir.exists());
    }

    #[test]
    fn counters_survive_a_reopen() {
        let dir = scratch();
        let mut learner = Learner::open(&dir, on());
        for _ in 0..5 {
            learner.record(&kept("habit", "Foo.bar"), 10);
        }
        learner.flush().unwrap();
        let again = Learner::open(&dir, on());
        assert_eq!(again.tally("habit", "Foo.bar").and_then(Tally::as_drawn_rate), Some(1.0));
        assert!(again.tally("habit", "Other").is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_context_with_enough_events_speaks_for_itself_and_others_fall_back_to_the_origin() {
        let mut learner = Learner::open(scratch(), on());
        for _ in 0..5 {
            learner.record(&kept("habit", "filter#0"), 1);
        }
        for _ in 0..5 {
            learner.record(&Event { origin: "habit", context: "map#0", ghost: "a", after: Some("b") }, 1);
        }
        assert_eq!(learner.useful_rate("habit", "filter#0"), Some(1.0));
        assert_eq!(learner.useful_rate("habit", "map#0"), Some(0.0));
        // a context never seen: the origin as a whole, half kept
        assert_eq!(learner.useful_rate("habit", "other#1"), Some(0.5));
    }

    #[test]
    fn one_counter_can_be_listed_and_forgotten() {
        let mut learner = Learner::open(scratch(), on());
        learner.record(&kept("habit", "filter#0"), 1);
        learner.record(&kept("model", "-"), 1);
        assert_eq!(learner.entries().count(), 2);
        assert!(learner.forget("habit", "filter#0"));
        assert!(!learner.forget("habit", "filter#0"));
        assert_eq!(learner.entries().map(|(o, c, _)| (o.to_string(), c.to_string())).collect::<Vec<_>>(), [("model".to_string(), "-".to_string())]);
    }

    #[test]
    fn the_counters_are_bounded() {
        let mut learner = Learner::open(scratch(), on());
        for i in 0..(MAX_KEYS + 50) {
            learner.record(&kept("habit", &format!("m{i}#0")), 1);
        }
        assert!(learner.tallies.len() <= MAX_KEYS);
    }

    #[test]
    fn an_origin_is_summed_over_its_contexts() {
        let mut learner = Learner::open(scratch(), on());
        learner.record(&kept("model", "A.a"), 1);
        learner.record(&kept("model", "B.b"), 1);
        learner.record(&kept("habit", "A.a"), 1);
        assert_eq!(learner.origin_tally("model").kept, 2.0);
    }

    #[test]
    fn reset_wipes_memory_and_both_files() {
        let dir = scratch();
        let mut learner = Learner::open(&dir, Settings { enabled: true, keep_text: true });
        learner.record(&kept("habit", "Foo.bar"), 10);
        learner.flush().unwrap();
        assert!(dir.join(COUNTERS_FILE).exists() && dir.join(EVENTS_FILE).exists());
        learner.reset().unwrap();
        assert!(!dir.join(COUNTERS_FILE).exists() && !dir.join(EVENTS_FILE).exists());
        assert!(learner.tally("habit", "Foo.bar").is_none());
        learner.reset().unwrap();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_raw_text_is_kept_only_when_asked() {
        let dir = scratch();
        let mut learner = Learner::open(&dir, on());
        learner.record(&kept("habit", "Foo.bar"), 10);
        assert!(!dir.join(EVENTS_FILE).exists());
        learner.set_settings(Settings { enabled: true, keep_text: true });
        learner.record(&kept("habit", "Foo.bar"), 11);
        let log = fs::read_to_string(dir.join(EVENTS_FILE)).unwrap();
        assert_eq!(log.lines().count(), 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_separator_in_a_name_is_refused_not_split() {
        let mut learner = Learner::open(scratch(), on());
        assert!(!learner.record(&kept("ha\tbit", "x"), 1));
    }

    #[test]
    fn a_broken_counters_file_starts_over() {
        let dir = scratch();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(COUNTERS_FILE), "{not json").unwrap();
        let learner = Learner::open(&dir, on());
        assert!(learner.tally("habit", "x").is_none());
        let _ = fs::remove_dir_all(dir);
    }
}
