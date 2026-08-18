//! What the open buffer gets: the gutter marks beside its declarations, and the warning on a pair
//! of systems nothing orders.
//!
//! ## Anchored in the buffer, answered from the model
//!
//! Both of these carry an **offset into the file in front of you**, and the model was built from
//! what was on disk at the last reindex — so an offset taken from it lands a line late as soon as
//! anything above it is typed. Everything anchored here is therefore re-read from `source` on each
//! call (one linear scan of one file, on the editor's debounce), and the model is consulted only
//! for what lives *elsewhere*: which systems touch this type, which system this one contends with.
//! A target in another file may be stale by a line; a squiggle under the caret may not.
//!
//! It is the same division `bennu-jpa` makes for the same reason.

use bennu_ext::prelude::{ExtGutterMark, ExtTarget};
use bennu_proto::prelude::Diagnostic;

use bennu_complete::prelude::line_number;

use crate::conflict::warnable;
use crate::items::scan_file;
use crate::mask::mask;
use crate::model::{access_keys, BevyModel, Role, SystemDecl};

/// A mark beside every ECS declaration in `source`, pointing at the systems that touch it.
pub fn gutter(model: &BevyModel, source: &str) -> Vec<ExtGutterMark> {
    scan_file(&mask(source))
        .types
        .into_iter()
        .map(|t| {
            let touching = model.touching(&access_keys(&t.name, &t.roles));
            let writers = touching.iter().filter(|(_, a)| a.kind.writes()).count();
            // A marker's users are its filters, and a mark that ignored them would be a dot beside
            // a component the whole project queries on saying nobody wants it.
            let filtering = model.filtering(&t.name);
            let role = t.roles.first().copied().unwrap_or(Role::Component);
            ExtGutterMark {
                line: line_number(source, t.offset),
                kind: role.gutter_kind().to_string(),
                tooltip: format!(
                    "{} — {}",
                    role.label(),
                    touch_summary(touching.len(), writers, filtering.len())
                ),
                // Read/written first, then the systems that only filter on it: the same order the
                // tooltip counts them in, and `filter` rather than a made-up read/write.
                targets: touching
                    .iter()
                    .map(|(s, a)| target(s, a.kind.label()))
                    .chain(filtering.iter().map(|(s, _)| target(s, "filter")))
                    .collect(),
            }
        })
        .collect()
}

/// One jump target: the system, and in one word what it does with the declaration.
fn target(s: &SystemDecl, how: &str) -> ExtTarget {
    ExtTarget {
        file: s.file.to_string_lossy().replace('\\', "/"),
        offset: s.offset,
        label: s.name.clone(),
        detail: format!("{how} · {}", s.schedules.first().map_or("unregistered", String::as_str)),
    }
}

fn touch_summary(total: usize, writers: usize, filters: usize) -> String {
    let mut parts = Vec::new();
    match (total, writers) {
        (0, _) => {}
        (n, 0) => parts.push(format!("read by {n}")),
        (n, w) if w == n => parts.push(format!("written by {w}")),
        (n, w) => parts.push(format!("{} read, {w} written", n - w)),
    }
    if filters > 0 {
        parts.push(format!("filtered on by {filters}"));
    }
    if parts.is_empty() {
        return "no system in this project touches it".to_string();
    }
    parts.join(" · ")
}

/// One warning per system in this buffer that contends with another and is ordered against it by
/// nothing.
///
/// **Only the unordered pairs.** A conflict is not a defect — two systems that want the same data
/// and say in which order is exactly how an ECS is written, and squiggling those would put a
/// permanent mark under half the systems in the project. What is worth interrupting for is the
/// pair where the order was never stated: it is decided by the schedule, it can change when an
/// unrelated system is added, and it is invisible until the day it is wrong. The full list, ordered
/// pairs included, stays in the Access conflicts panel.
pub fn diagnostics(model: &BevyModel, source: &str) -> Vec<Diagnostic> {
    let scanned = scan_file(&mask(source));
    let mut out = Vec::new();
    for f in &scanned.fns {
        for c in &model.conflicts {
            if !warnable(c, &model.systems) {
                continue;
            }
            let (a, b) = (&model.systems[c.a], &model.systems[c.b]);
            let other = if a.name == f.name {
                b
            } else if b.name == f.name {
                a
            } else {
                continue;
            };
            let targets: Vec<&str> = c.reasons.iter().map(|r| r.target.as_str()).collect();
            out.push(Diagnostic {
                message: format!(
                    "`{}` contends over {} in {}, and nothing in this project orders the two — \
                     they run in whichever order the schedule picks",
                    other.name,
                    targets.join(", "),
                    c.schedule,
                ),
                severity: "warning".to_string(),
                code: "bevy.unordered-conflict".to_string(),
                start: f.offset,
                end: f.offset + f.name.len(),
            });
        }
    }
    out
}
