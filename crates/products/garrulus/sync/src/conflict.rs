//! What happens when two machines edited the same note: the note-level merge,
//! the daily-note special case, and the side file the remote version is parked
//! in when nothing else works.
//!
//! The rule the whole product rests on (`docs/garrulus-design.md` §4.4.3): what
//! does not auto-merge **never goes into the file**. The note keeps the local
//! text, the remote text is written beside it under a name that says what it is
//! and where it came from, and the pair is reported so the UI can offer *keep
//! mine* / *take theirs* / *merge by hand*. Nothing is lost, nothing is
//! corrupted, and the vault still opens in Obsidian mid-conflict.

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::change::RelPath;
use crate::frontmatter::{join, merge_frontmatter, split_front};
use crate::merge::merge_text3;

/// The word that makes a conflict side file recognisable, in a file listing and
/// to [`is_side_file`].
pub const CONFLICT_MARKER: &str = "(conflitto";

/// One note two machines disagree about.
///
/// The three sides travel as *text*. The UI renders them in the `DiffViewer`;
/// it never sees a `<<<<<<<`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    /// The note, vault-relative. It still holds the local text.
    pub path: RelPath,
    /// The common ancestor, when the remote has one (git yes, folder no).
    pub base: Option<String>,
    /// What this machine has — and what stayed in the file.
    pub local: String,
    /// What the other machine has.
    pub remote: String,
    /// Where the remote text was parked, when it was parked.
    pub side_file: Option<RelPath>,
}

/// Day, month and time of day, in the local reading of a wall clock.
///
/// A struct rather than a formatted string so the naming function stays pure
/// and testable: the clock is read once, at the edge, by [`ConflictStamp::now`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictStamp {
    /// 1-12.
    pub month: u32,
    /// 1-31.
    pub day: u32,
    /// 0-23.
    pub hour: u32,
    /// 0-59.
    pub minute: u32,
}

impl ConflictStamp {
    /// Break a Unix timestamp into UTC calendar fields.
    ///
    /// UTC, not local time: this crate has no timezone database and inventing
    /// one for a file name is not worth a dependency. The stamp exists to
    /// disambiguate two conflicts of the same note, which it does either way.
    pub fn from_unix_utc(secs: i64) -> Self {
        let days = secs.div_euclid(86_400);
        let rem = secs.rem_euclid(86_400);
        let (_, month, day) = civil_from_days(days);
        Self {
            month,
            day,
            hour: (rem / 3_600) as u32,
            minute: ((rem % 3_600) / 60) as u32,
        }
    }

    /// Read the clock. The only impure thing in this module.
    pub fn now() -> Self {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Self::from_unix_utc(secs)
    }

    /// `dd-MM HH:mm`, the form that goes in a side file name.
    pub fn label(&self) -> String {
        format!("{:02}-{:02} {:02}:{:02}", self.day, self.month, self.hour, self.minute)
    }
}

/// Days since the epoch to `(year, month, day)` — Howard Hinnant's civil
/// calendar algorithm, shifted to a March-based year so leap days fall last.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11], March = 0
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

/// The name the remote side of a conflict is parked under:
/// `<nota> (conflitto — <device>, <dd-MM HH:mm>).md`, in the note's own folder.
///
/// Beside the note on purpose — the user finds it where they are already
/// looking, and Obsidian shows it as an ordinary note they can read.
pub fn side_file_name(path: &RelPath, device: &str, at: ConflictStamp) -> RelPath {
    let ext = path.extension().unwrap_or_else(|| "md".to_string());
    let name = format!(
        "{} {} — {}, {}).{}",
        path.file_stem(),
        CONFLICT_MARKER,
        sanitize_device(device),
        at.label(),
        ext,
    );
    path.with_file_name(&name)
}

/// Is this note a conflict side file rather than a note the user wrote?
///
/// Used to count outstanding conflicts and to keep side files out of the ones
/// the engine tries to merge again.
pub fn is_side_file(path: &RelPath) -> bool {
    path.file_stem().contains(CONFLICT_MARKER)
}

/// Strip what a filesystem refuses (Windows is the strict one) from a device
/// name before it goes in a file name.
fn sanitize_device(device: &str) -> String {
    let cleaned: String = device
        .trim()
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            c if (c as u32) < 0x20 => '-',
            c => c,
        })
        .collect();
    if cleaned.is_empty() {
        "altro PC".to_string()
    } else {
        cleaned
    }
}

/// Merge one note three-way: frontmatter field-wise, body line-wise.
///
/// `None` means the user has to arbitrate — the caller then keeps the local
/// text and parks the remote one with [`side_file_name`].
pub fn merge_note(base: Option<&str>, local: &str, remote: &str) -> Option<String> {
    if local == remote {
        return Some(local.to_string());
    }
    let (base_front, base_body) = match base {
        Some(b) => {
            let (f, b) = split_front(b);
            (f, Some(b))
        }
        None => (None, None),
    };
    let (local_front, local_body) = split_front(local);
    let (remote_front, remote_body) = split_front(remote);

    let front = merge_frontmatter(base_front, local_front, remote_front)?;
    let body = merge_text3(base_body, local_body, remote_body)?;
    Some(join(front.as_deref(), &body))
}

/// Merge a daily note by taking the **union of both days' entries in time
/// order**, which is the correct answer for the one file two machines are
/// guaranteed to both append to (`docs/garrulus-design.md` §4.4.5).
///
/// A three-way merge is not just risky here, it is *wrong*: two machines
/// appending different lines at the same position is not a conflict, it is two
/// entries. Never fails — that is the point.
///
/// Ordering rule: local order is the spine, remote-only entries are appended,
/// then the whole list is stably sorted by the `HH:MM` an entry starts with. An
/// entry with no time inherits the time of the entry above it, so a note's
/// heading stays at the top and a continuation line stays with its bullet.
pub fn append_merge_daily(base: Option<&str>, local: &str, remote: &str) -> String {
    if local == remote {
        return local.to_string();
    }
    let base_front = base.map(|b| split_front(b).0).unwrap_or(None);
    let (local_front, local_body) = split_front(local);
    let (remote_front, remote_body) = split_front(remote);

    // Frontmatter still merges by field; when even that disagrees the local
    // block wins, because a daily note's frontmatter is bookkeeping and losing
    // an entry to a bookkeeping conflict would be absurd.
    let front = merge_frontmatter(base_front, local_front, remote_front)
        .unwrap_or_else(|| local_front.map(str::to_string));

    let body = union_entries(local_body, remote_body);
    join(front.as_deref(), &body)
}

/// Union of two bodies, local order first, sorted by the time each entry
/// carries.
fn union_entries(local: &str, remote: &str) -> String {
    let mut entries: Vec<String> = local.lines().map(str::to_string).collect();
    let seen: HashSet<&str> =
        local.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    for line in remote.lines() {
        let key = line.trim();
        if key.is_empty() || seen.contains(key) {
            continue;
        }
        entries.push(line.to_string());
    }

    // Sort key: the last time seen at or above this entry, then the original
    // position. Stable, so entries with the same key keep their order.
    let mut keyed: Vec<(u32, usize, String)> = Vec::with_capacity(entries.len());
    let mut current = 0u32;
    for (i, e) in entries.into_iter().enumerate() {
        if let Some(t) = leading_time(&e) {
            current = t;
        }
        keyed.push((current, i, e));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut out = keyed.into_iter().map(|(_, _, e)| e).collect::<Vec<_>>().join("\n");
    if local.ends_with('\n') || remote.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Minutes since midnight of the `HH:MM` an entry starts with, once list
/// markers, task checkboxes, heading hashes and `**` emphasis are stripped.
fn leading_time(line: &str) -> Option<u32> {
    let mut s = line.trim_start();
    loop {
        let before = s;
        for prefix in ["- [ ] ", "- [x] ", "- [X] ", "- ", "* ", "+ ", "> ", "**"] {
            if let Some(rest) = s.strip_prefix(prefix) {
                s = rest.trim_start();
            }
        }
        s = s.trim_start_matches('#').trim_start();
        if s == before {
            break;
        }
    }
    let bytes = s.as_bytes();
    if bytes.len() < 5 || bytes[2] != b':' {
        return None;
    }
    let h: u32 = s.get(0..2)?.parse().ok()?;
    let m: u32 = s.get(3..5)?.parse().ok()?;
    if h > 23 || m > 59 {
        return None;
    }
    Some(h * 60 + m)
}

/// Is this note the day's daily note?
///
/// The daily-note folder is a vault setting; without one configured nothing is
/// special-cased, because guessing would apply append-merge semantics to an
/// ordinary note and silently duplicate its lines.
pub fn is_daily_note(path: &RelPath, daily_folder: Option<&str>) -> bool {
    match daily_folder {
        Some(f) if !f.trim().is_empty() => path.is_in_folder(f.trim()),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_and_a_known_leap_day() {
        let s = ConflictStamp::from_unix_utc(0);
        assert_eq!((s.day, s.month, s.hour, s.minute), (1, 1, 0, 0));
        // 2000-03-01T00:00:00Z, the day after a leap day.
        let s = ConflictStamp::from_unix_utc(951_868_800);
        assert_eq!((s.day, s.month), (1, 3));
        // 2000-02-29T23:59:00Z.
        let s = ConflictStamp::from_unix_utc(951_868_800 - 60);
        assert_eq!((s.day, s.month, s.hour, s.minute), (29, 2, 23, 59));
        assert_eq!(s.label(), "29-02 23:59");
    }

    #[test]
    fn side_file_sits_next_to_the_note() {
        let stamp = ConflictStamp { month: 7, day: 31, hour: 14, minute: 22 };
        let side = side_file_name(&RelPath::new("bugs/crash.md"), "casa", stamp);
        assert_eq!(side.as_str(), "bugs/crash (conflitto — casa, 31-07 14:22).md");
        assert!(is_side_file(&side));
        assert!(!is_side_file(&RelPath::new("bugs/crash.md")));
    }

    #[test]
    fn side_file_name_survives_a_hostile_device_name() {
        let stamp = ConflictStamp { month: 1, day: 2, hour: 3, minute: 4 };
        let side = side_file_name(&RelPath::new("nota.md"), "C:/lavoro?", stamp);
        assert_eq!(side.as_str(), "nota (conflitto — C--lavoro-, 02-01 03:04).md");
    }

    #[test]
    fn note_merge_combines_frontmatter_and_body() {
        let base = "---\nstatus: aperto\n---\n# Bug\n\npasso 1\n";
        let local = "---\nstatus: in corso\n---\n# Bug\n\npasso 1\n";
        let remote = "---\nstatus: aperto\n---\n# Bug\n\npasso 1\npasso 2\n";
        assert_eq!(
            merge_note(Some(base), local, remote).as_deref(),
            Some("---\nstatus: in corso\n---\n# Bug\n\npasso 1\npasso 2\n")
        );
    }

    #[test]
    fn note_merge_reports_a_body_conflict() {
        let base = "# Bug\n\ndescrizione\n";
        let local = "# Bug\n\ndescrizione mia\n";
        let remote = "# Bug\n\ndescrizione sua\n";
        assert_eq!(merge_note(Some(base), local, remote), None);
    }

    #[test]
    fn note_merge_without_a_base_only_accepts_identity() {
        assert_eq!(merge_note(None, "uguale\n", "uguale\n").as_deref(), Some("uguale\n"));
        assert_eq!(merge_note(None, "mio\n", "suo\n"), None);
    }

    #[test]
    fn daily_note_unions_both_days_in_time_order() {
        let base = "# 31-07\n\n- 09:00 standup\n";
        let local = "# 31-07\n\n- 09:00 standup\n- 14:30 fix del crash\n";
        let remote = "# 31-07\n\n- 09:00 standup\n- 11:15 chiamata\n";
        let merged = append_merge_daily(Some(base), local, remote);
        assert_eq!(
            merged,
            "# 31-07\n\n- 09:00 standup\n- 11:15 chiamata\n- 14:30 fix del crash\n"
        );
    }

    #[test]
    fn daily_note_never_drops_an_entry_even_untimed() {
        let local = "# Diario\n- pensiero locale\n";
        let remote = "# Diario\n- pensiero remoto\n";
        let merged = append_merge_daily(None, local, remote);
        assert!(merged.contains("pensiero locale"));
        assert!(merged.contains("pensiero remoto"));
        assert!(merged.starts_with("# Diario\n"));
    }

    #[test]
    fn daily_note_does_not_duplicate_shared_entries() {
        let local = "- 09:00 standup\n- 10:00 mio\n";
        let remote = "- 09:00 standup\n";
        assert_eq!(append_merge_daily(None, local, remote), "- 09:00 standup\n- 10:00 mio\n");
    }

    #[test]
    fn time_prefixes_are_recognised_through_list_syntax() {
        assert_eq!(leading_time("- [ ] 07:05 fare la spesa"), Some(7 * 60 + 5));
        assert_eq!(leading_time("**14:22** riunione"), Some(14 * 60 + 22));
        assert_eq!(leading_time("## 23:59"), Some(23 * 60 + 59));
        assert_eq!(leading_time("- niente ora"), None);
        assert_eq!(leading_time("- 99:99 non è un orario"), None);
    }

    #[test]
    fn daily_detection_needs_a_configured_folder() {
        let p = RelPath::new("diario/2026-07-31.md");
        assert!(is_daily_note(&p, Some("diario")));
        assert!(!is_daily_note(&p, Some("bugs")));
        assert!(!is_daily_note(&p, None));
    }
}
