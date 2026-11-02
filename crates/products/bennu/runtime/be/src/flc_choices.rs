//! What you pick from the completion list, per context, kept on disk — IntelliJ's "statistics" weigher.
//!
//! The session memory in `bennu-query`'s `picked` module already learns "reaching into this type, you pick `stream`"
//! while the editor is open. This is the part that survives a restart and knows two more places: the **receiver** of
//! a member access (`m:java/util/List`) and the **argument slot** of a call (`a:filter#0`). Counters only — a context
//! and the label that was picked there, with how many times. No source, no file name, no project: the contexts of
//! library types are shared by every project, so the table is global.
//!
//! **What it does.** The next list in the same place gets the labels picked at least twice put first, in order of how
//! often (at most three), and the leader is preselected only when it was picked at least three times and holds half
//! of what was ever picked there. Everything else keeps the provider's order.
//!
//! **Switch and forget.** It counts only while *Count how often suggestions are taken* (`flc_feedback`) is on, and
//! *Forget what Bennu learned* deletes the file and empties the table. Writes are atomic, at most every
//! [`WRITE_EVERY`], on a background thread, and never on the keystroke path.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use bennu_proto::prelude::CompletionItem;
use serde::{Deserialize, Serialize};

use crate::index_service::IndexService;

const VERSION: u32 = 1;
/// Contexts kept; past this the one with the fewest picks goes.
const MAX_CONTEXTS: usize = 20_000;
/// Labels kept per context; past this the least picked goes.
const MAX_PER_CONTEXT: usize = 50;
/// A label is lifted once it was picked this often in the context...
const MIN_PICKS: u32 = 2;
/// ...at most this many are lifted...
const MAX_LIFTED: usize = 3;
/// ...and the first is preselected at this many picks and this share of the context's picks.
const PRESELECT_PICKS: u32 = 3;
const PRESELECT_SHARE: f32 = 0.5;
/// The file is rewritten at most this often.
const WRITE_EVERY: Duration = Duration::from_secs(30);
/// How long the setting is trusted before the config is read again.
const SETTING_TTL: Duration = Duration::from_secs(5);

/// The counters: context → label → picks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Choices {
    contexts: BTreeMap<String, BTreeMap<String, u32>>,
}

impl Choices {
    pub(crate) fn record(&mut self, context: &str, label: &str) {
        let row = self.contexts.entry(context.to_string()).or_default();
        let n = row.entry(label.to_string()).or_insert(0);
        *n = n.saturating_add(1);
        if row.len() > MAX_PER_CONTEXT {
            if let Some(least) = row.iter().filter(|(l, _)| *l != label).min_by_key(|(_, n)| **n).map(|(l, _)| l.clone()) {
                row.remove(&least);
            }
        }
        if self.contexts.len() > MAX_CONTEXTS {
            if let Some(least) = self.contexts.iter().filter(|(c, _)| *c != context).min_by_key(|(_, r)| r.values().sum::<u32>()).map(|(c, _)| c.clone()) {
                self.contexts.remove(&least);
            }
        }
    }

    pub(crate) fn picks(&self, context: &str) -> Option<&BTreeMap<String, u32>> {
        self.contexts.get(context)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.contexts.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.contexts.clear();
    }
}

/// The file's shape: versioned, so a later layout just means starting empty.
#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    contexts: BTreeMap<String, BTreeMap<String, u32>>,
}

// ---- the context --------------------------------------------------------------------------------------------------

fn ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Where the word the caret is in (or ends) starts, and whether a `.` stands right before it.
pub(crate) fn word_start(source: &str, offset: usize) -> Option<(usize, bool)> {
    let b = source.as_bytes();
    if offset > b.len() || !source.is_char_boundary(offset) {
        return None;
    }
    let mut i = offset;
    while i > 0 && ident_byte(b[i - 1]) {
        i -= 1;
    }
    Some((i, i > 0 && b[i - 1] == b'.'))
}

/// The context a pick at `offset` belongs to: `m:<receiver type>` after a `.`, `a:<callee>#<index>` in a call's
/// argument, `None` for a bare word anywhere else.
pub(crate) fn context_key(file: &str, source: &str, offset: usize) -> Option<String> {
    let (ws, after_dot) = word_start(source, offset)?;
    if after_dot {
        // the receiver is read with the typed prefix taken out — the way completion itself reads it: the parser wants
        // the caret right after the dot, and a half-typed name there mis-parses the receiver
        let b = source.as_bytes();
        let mut end = ws;
        while end < b.len() && ident_byte(b[end]) {
            end += 1;
        }
        let stub = if source[end..].starts_with('(') { "__x" } else { "" };
        let repaired = format!("{}{stub}{}", &source[..ws], &source[end..]);
        let receiver = IndexService::global().receiver_type_for(file, &repaired, ws)?;
        return Some(format!("m:{receiver}"));
    }
    let (callee, index, _) = bennu_query::prelude::call_site_name_and_index(source, ws)?;
    Some(format!("a:{callee}#{index}"))
}

// ---- the table ----------------------------------------------------------------------------------------------------

struct Held {
    choices: Choices,
    dirty: bool,
    last_write: Instant,
}

/// Tests never touch the user's data folder.
#[cfg(test)]
const DISK: bool = false;
#[cfg(not(test))]
const DISK: bool = true;

fn path() -> PathBuf {
    arbor_core::prelude::bennu_data_dir().join("flc").join("choices.json")
}

fn load() -> Choices {
    if !DISK {
        return Choices::default();
    }
    let text = std::fs::read_to_string(path()).ok();
    match text.and_then(|t| serde_json::from_str::<File>(&t).ok()) {
        Some(f) if f.version == VERSION => Choices { contexts: f.contexts },
        _ => Choices::default(),
    }
}

fn held() -> &'static Mutex<Held> {
    static HELD: OnceLock<Mutex<Held>> = OnceLock::new();
    HELD.get_or_init(|| {
        spawn_timer();
        Mutex::new(Held { choices: load(), dirty: false, last_write: Instant::now() })
    })
}

fn lock() -> std::sync::MutexGuard<'static, Held> {
    held().lock().unwrap_or_else(|p| p.into_inner())
}

fn write(h: &mut Held) {
    h.dirty = false;
    h.last_write = Instant::now();
    if !DISK {
        return;
    }
    let file = File { version: VERSION, contexts: h.choices.contexts.clone() };
    let Ok(bytes) = serde_json::to_vec(&file) else { return };
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // atomic: a counter file that cannot be written costs the counters, never the editor
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// A thread that writes what changed, every [`WRITE_EVERY`]: the keystroke path only marks the table dirty.
fn spawn_timer() {
    let _ = std::thread::Builder::new().name("flc-choices-timer".into()).spawn(|| loop {
        std::thread::sleep(WRITE_EVERY);
        let mut h = lock();
        if h.dirty {
            write(&mut h);
        }
    });
}

/// Writes pending counters now (the settings page asked, or the app is closing).
pub(crate) fn flush() {
    let mut h = lock();
    if h.dirty {
        write(&mut h);
    }
}

/// What the picks take: they belong to no project, so it is all "shared".
pub(crate) fn sizes() -> crate::flc_memory::Sizes {
    use crate::flc_memory::{text, Sizes};
    let h = lock();
    let mut s = Sizes::default();
    for (context, row) in &h.choices.contexts {
        s.shared_entries += row.len();
        s.shared += text(context) + 48 + row.keys().map(|l| text(l) + 4 + 48).sum::<usize>();
    }
    s
}

/// Forget every pick: the table and the file.
pub(crate) fn clear() {
    let mut h = lock();
    h.choices.clear();
    h.dirty = false;
    if DISK {
        let _ = std::fs::remove_file(path());
    }
}

// ---- the switch ---------------------------------------------------------------------------------------------------

static FORCED: AtomicBool = AtomicBool::new(false);
static FORCED_VALUE: AtomicBool = AtomicBool::new(true);

/// Pins the setting for a test, so it does not depend on the machine's config file.
#[cfg(test)]
pub(crate) fn force_enabled(on: Option<bool>) {
    FORCED.store(on.is_some(), Ordering::Relaxed);
    FORCED_VALUE.store(on.unwrap_or(true), Ordering::Relaxed);
}

/// `flc_feedback`, read from the config at most every [`SETTING_TTL`]: this runs on every completion.
pub(crate) fn enabled() -> bool {
    if FORCED.load(Ordering::Relaxed) {
        return FORCED_VALUE.load(Ordering::Relaxed);
    }
    static CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    let mut c = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((at, on)) = *c {
        if at.elapsed() < SETTING_TTL {
            return on;
        }
    }
    let on = bennu_core::config::load().flc_feedback;
    *c = Some((Instant::now(), on));
    on
}

// ---- counting and lifting -----------------------------------------------------------------------------------------

/// Counts that `label` was picked at `offset` of `source` (the word's start). A no-op while the setting is off, and
/// for a pick that has no context.
pub(crate) fn record_pick(file: &str, source: &str, offset: usize, label: &str) {
    if !enabled() || label.is_empty() {
        return;
    }
    let Some(context) = context_key(file, source, offset) else { return };
    let mut h = lock();
    h.choices.record(&context, label);
    h.dirty = true;
}

/// Puts what was picked most often in this context first. See the module doc for the rules.
pub(crate) fn lift(file: &str, source: &str, offset: usize, items: &mut Vec<CompletionItem>) {
    if items.is_empty() || !enabled() {
        return;
    }
    // nothing learned yet: no reason to work out where the caret is
    if lock().choices.is_empty() {
        return;
    }
    let Some(context) = context_key(file, source, offset) else { return };
    let picks = lock().choices.picks(&context).cloned();
    if let Some(picks) = picks {
        lift_with(&picks, items);
    }
}

/// A row a habit put first (`used N× in .callee(…)`): picks never go above it.
fn is_habit_row(item: &CompletionItem) -> bool {
    item.detail.as_deref().is_some_and(|d| d.starts_with("used ") && d.contains("× in ."))
}

/// The pure half of [`lift`]: `picks` is what was chosen in this context.
pub(crate) fn lift_with(picks: &BTreeMap<String, u32>, items: &mut Vec<CompletionItem>) {
    let total: u32 = picks.values().sum();
    let mut lifted: Vec<(u32, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, it)| picks.get(&it.label).copied().filter(|n| *n >= MIN_PICKS).map(|n| (n, i)))
        .collect();
    // most picked first; the provider's order breaks a tie
    lifted.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    lifted.truncate(MAX_LIFTED);
    if lifted.is_empty() {
        return;
    }
    // a habit that leads the list keeps its place
    let led_by_habit = items.first().is_some_and(|i| is_habit_row(i) && i.preselect);
    let below = if led_by_habit { items.iter().take_while(|i| is_habit_row(i)).count() } else { 0 };
    let (top_picks, _) = lifted[0];
    let preselect = !led_by_habit && top_picks >= PRESELECT_PICKS && top_picks as f32 / total as f32 >= PRESELECT_SHARE;

    let mut rows: Vec<CompletionItem> = Vec::new();
    let mut taken: Vec<usize> = lifted.iter().map(|(_, i)| *i).collect();
    for &(_, i) in &lifted {
        rows.push(items[i].clone());
    }
    taken.sort_unstable_by(|a, b| b.cmp(a));
    for i in taken {
        items.remove(i);
    }
    if preselect {
        for it in items.iter_mut() {
            it.preselect = false;
        }
        rows[0].preselect = true;
    }
    for (k, row) in rows.into_iter().enumerate() {
        items.insert(below + k, row);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn item(label: &str) -> CompletionItem {
        CompletionItem { label: label.to_string(), kind: "method".to_string(), ..Default::default() }
    }

    fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }

    fn picks(pairs: &[(&str, u32)]) -> BTreeMap<String, u32> {
        pairs.iter().map(|(l, n)| (l.to_string(), *n)).collect()
    }

    #[test]
    fn a_pick_is_counted_under_its_context() {
        let mut c = Choices::default();
        c.record("m:java/util/List", "stream");
        c.record("m:java/util/List", "stream");
        c.record("a:filter#0", "Objects");
        assert_eq!(c.picks("m:java/util/List").unwrap()["stream"], 2);
        assert_eq!(c.picks("a:filter#0").unwrap().len(), 1);
        assert!(c.picks("m:other").is_none());
    }

    #[test]
    fn the_least_picked_label_and_the_quietest_context_go_when_over_the_cap() {
        let mut c = Choices::default();
        for i in 0..MAX_PER_CONTEXT {
            for _ in 0..3 {
                c.record("ctx", &format!("l{i}"));
            }
        }
        c.record("ctx", "newcomer");
        let row = c.picks("ctx").unwrap();
        assert_eq!(row.len(), MAX_PER_CONTEXT);
        assert!(row.contains_key("newcomer"), "the label just picked stays");
        let mut many = Choices::default();
        for i in 0..MAX_CONTEXTS {
            many.record(&format!("c{i}"), "x");
        }
        many.record("busy", "x");
        many.record("busy", "x");
        assert_eq!(many.contexts.len(), MAX_CONTEXTS);
        assert!(many.picks("busy").is_some());
    }

    #[test]
    fn the_table_round_trips_through_json_and_holds_no_text_but_the_names() {
        let mut c = Choices::default();
        c.record("m:java/util/List", "stream");
        let json = serde_json::to_string(&File { version: VERSION, contexts: c.contexts.clone() }).unwrap();
        let back: File = serde_json::from_str(&json).unwrap();
        assert_eq!(back.version, VERSION);
        assert_eq!(back.contexts, c.contexts);
        assert_eq!(json, r#"{"version":1,"contexts":{"m:java/util/List":{"stream":1}}}"#);
    }

    #[test]
    fn clearing_forgets_everything() {
        let mut c = Choices::default();
        c.record("a", "b");
        c.clear();
        assert!(c.is_empty());
    }

    #[test]
    fn the_word_start_is_found_with_and_without_a_typed_prefix() {
        let src = "x = list.str";
        assert_eq!(word_start(src, src.len()), Some((src.len() - 3, true)));
        let src = "x = list.";
        assert_eq!(word_start(src, src.len()), Some((src.len(), true)));
        // a word that follows no dot
        let src = "foo(bar";
        assert_eq!(word_start(src, src.len()), Some((4, false)));
        // the start itself answers the same as the end of the word
        let src = "a.b.method";
        assert_eq!(word_start(src, 4), word_start(src, src.len()));
        assert_eq!(word_start(src, 99), None);
    }

    #[test]
    fn an_argument_slot_is_a_context_and_a_bare_word_is_not() {
        // no index is needed for an argument slot
        assert_eq!(context_key("A.java", "class A { void f() { list.filter(Boo", 36).as_deref(), Some("a:filter#0"));
        assert_eq!(context_key("A.java", "class A { void f() { foo(a, Bo", 30).as_deref(), Some("a:foo#1"));
        assert_eq!(context_key("A.java", "class A { void f() { Bo", 23), None);
    }

    #[test]
    fn what_was_picked_twice_goes_first_in_order_of_picks() {
        let mut items = vec![item("a"), item("b"), item("c"), item("d"), item("e")];
        lift_with(&picks(&[("d", 2), ("b", 5), ("e", 1), ("zzz", 9)]), &mut items);
        assert_eq!(labels(&items), ["b", "d", "a", "c", "e"], "e was picked once; zzz is not in the list");
    }

    #[test]
    fn at_most_three_rows_are_lifted_and_a_tie_keeps_the_providers_order() {
        let mut items = vec![item("a"), item("b"), item("c"), item("d"), item("e")];
        lift_with(&picks(&[("e", 2), ("d", 2), ("c", 2), ("b", 2)]), &mut items);
        assert_eq!(labels(&items), ["b", "c", "d", "a", "e"]);
    }

    #[test]
    fn the_leader_is_preselected_only_with_three_picks_and_half_the_share() {
        let mut items = vec![item("a"), item("b"), item("c")];
        items[0].preselect = true;
        lift_with(&picks(&[("b", 3), ("c", 1)]), &mut items);
        assert_eq!(labels(&items), ["b", "a", "c"]);
        assert!(items[0].preselect && !items[1].preselect, "the old preselect is cleared when one is set");

        // three picks but under half of what was ever picked there
        let mut items = vec![item("a"), item("b")];
        items[0].preselect = true;
        lift_with(&picks(&[("b", 3), ("x", 4)]), &mut items);
        assert_eq!(labels(&items), ["b", "a"]);
        assert!(items[1].preselect && !items[0].preselect, "the provider's preselect stays");

        // two picks: lifted, never preselected
        let mut items = vec![item("a"), item("b")];
        lift_with(&picks(&[("b", 2)]), &mut items);
        assert_eq!(labels(&items), ["b", "a"]);
        assert!(!items[0].preselect);
    }

    #[test]
    fn a_habit_that_leads_keeps_its_place_and_its_preselect() {
        let mut habit = item("Boolean.FALSE::equals");
        habit.detail = Some("used 5× in .filter(…)".to_string());
        habit.preselect = true;
        let mut items = vec![habit, item("a"), item("b")];
        lift_with(&picks(&[("b", 6)]), &mut items);
        assert_eq!(labels(&items), ["Boolean.FALSE::equals", "b", "a"]);
        assert!(items[0].preselect && !items[1].preselect);
    }

    #[test]
    fn nothing_picked_enough_changes_nothing() {
        let mut items = vec![item("a"), item("b")];
        lift_with(&picks(&[("b", 1)]), &mut items);
        assert_eq!(labels(&items), ["a", "b"]);
        lift_with(&BTreeMap::new(), &mut items);
        assert_eq!(labels(&items), ["a", "b"]);
    }

    /// The table and the switch are process-wide: the tests that touch them take turns.
    static TURN: Mutex<()> = Mutex::new(());

    /// Opens a throw-away project with one class and returns its file name, its text and the caret at the `|`.
    fn project(body: &str) -> (String, String, usize) {
        use std::sync::Arc;
        let dir = std::env::temp_dir().join(format!("arbor-choices-{}", std::process::id()));
        let java = dir.join("src/main/java/z");
        std::fs::create_dir_all(&java).unwrap();
        let bag = "package z;\npublic class Bag {\n    public int size() { return 0; }\n    public int stream() { return 0; }\n    public int sort() { return 0; }\n}\n";
        let text = format!("package z;\npublic class Use {{\n    void f(Bag bag) {{\n{body}\n    }}\n}}\n");
        let caret = text.find('|').unwrap();
        let text = text.replace('|', "");
        std::fs::write(java.join("Bag.java"), bag).unwrap();
        std::fs::write(java.join("Use.java"), &text).unwrap();
        let root = dir.to_string_lossy().into_owned();
        let svc = IndexService::global();
        if !svc.has_resolver(&root) {
            struct Quiet;
            impl arbor_ipc::prelude::EventSink for Quiet {
                fn emit(&self, _topic: &str, _payload: serde_json::Value) {}
            }
            svc.open(&root, "21", "UTF-8", Arc::new(Quiet));
            let started = Instant::now();
            while !svc.has_resolver(&root) {
                assert!(started.elapsed() < Duration::from_secs(120), "the index did not come up");
                std::thread::sleep(Duration::from_millis(200));
            }
        }
        (java.join("Use.java").to_string_lossy().into_owned(), text, caret)
    }

    #[test]
    fn a_member_pick_is_counted_per_receiver_type_and_lifts_it_next_time() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        force_enabled(Some(true));
        lock().choices.clear();
        let (file, text, caret) = project("        bag.so|");
        // the context is the receiver's type, with or without the typed prefix
        assert_eq!(context_key(&file, &text, caret).as_deref(), Some("m:z/Bag"));
        assert_eq!(context_key(&file, &text, caret - 2).as_deref(), Some("m:z/Bag"));
        // picked three times where the word starts (the editor reports the start, not the caret)
        for _ in 0..3 {
            record_pick(&file, &text, caret - 2, "stream");
        }
        let mut items = vec![item("size"), item("sort"), item("stream")];
        lift(&file, &text, caret, &mut items);
        assert_eq!(labels(&items), ["stream", "size", "sort"]);
        assert!(items[0].preselect, "three picks, all of them in this context");
        // another receiver does not borrow it
        let (file, other, at) = project("        new StringBuilder().ap|");
        let mut items = vec![item("append"), item("stream")];
        lift(&file, &other, at, &mut items);
        assert_eq!(labels(&items), ["append", "stream"]);
        lock().choices.clear();
        force_enabled(None);
    }

    #[test]
    fn forgetting_empties_the_table() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        lock().choices.record("a:filter#0", "Objects");
        assert!(!lock().choices.is_empty());
        clear();
        assert!(lock().choices.is_empty());
    }

    #[test]
    fn with_the_switch_off_nothing_is_counted_and_nothing_is_lifted() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        force_enabled(Some(false));
        let mut items = vec![item("a"), item("b")];
        lift("A.java", "class A { void f() { list.filter(b", 33, &mut items);
        assert_eq!(labels(&items), ["a", "b"]);
        force_enabled(None);
    }
}
