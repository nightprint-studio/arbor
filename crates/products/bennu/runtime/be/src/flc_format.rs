//! How the project lays out its code, summed over its files, for the ghost text to follow.
//!
//! One small set of counters per file (`bennu-learn`'s `FormatCounts`), fed by the same background scan that feeds the
//! other profiles and kept current a file at a time. The style is their sum, read once per change and remembered: a
//! suggestion asks for it on every keystroke and must not add up thousands of files to answer.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use bennu_learn::prelude::{conform, FormatCounts, FormatStyle};

#[derive(Default)]
struct Held {
    files: HashMap<String, FormatCounts>,
    /// The sum's style, `None` after any change until it is read again.
    style: Option<FormatStyle>,
}

fn lock() -> MutexGuard<'static, Held> {
    static HELD: OnceLock<Mutex<Held>> = OnceLock::new();
    HELD.get_or_init(Mutex::default).lock().unwrap_or_else(|p| p.into_inner())
}

pub(crate) fn add_file(key: &str, source: &str) {
    let counts = FormatCounts::of(source);
    let mut h = lock();
    h.files.insert(key.to_string(), counts);
    h.style = None;
}

pub(crate) fn sizes() -> crate::flc_memory::Sizes {
    use crate::flc_memory::Sizes;
    let h = lock();
    let mut s = Sizes::default();
    for k in h.files.keys() {
        s.add_file(k, std::mem::size_of::<FormatCounts>());
    }
    s
}

/// Forgets everything counted, for *Forget what Bennu learned*.
pub(crate) fn clear() {
    *lock() = Held::default();
}

/// Files whose layout was counted, for the memory breakdown.
pub(crate) fn tracked() -> usize {
    lock().files.len()
}

pub(crate) fn remove_file(key: &str) {
    let mut h = lock();
    if h.files.remove(key).is_some() {
        h.style = None;
    }
}

/// What the project does, where it does it nearly always.
pub(crate) fn style() -> FormatStyle {
    let mut h = lock();
    if let Some(style) = h.style {
        return style;
    }
    let mut sum = FormatCounts::default();
    h.files.values().for_each(|c| sum.merge(c));
    let style = FormatStyle::from(&sum);
    h.style = Some(style);
    style
}

/// What the project does, in words, for the page that shows it; one line per thing it is consistent about.
pub(crate) fn describe() -> Vec<String> {
    use bennu_learn::prelude::Indent;
    let s = style();
    let mut out = Vec::new();
    match s.indent {
        Some(Indent::Tabs) => out.push("Indentation: tabs".to_string()),
        Some(Indent::Spaces(n)) => out.push(format!("Indentation: {n} spaces")),
        None => {}
    }
    let yes = |v: Option<bool>, on: &str, off: &str| v.map(|v| if v { on } else { off }.to_string());
    out.extend(yes(s.brace_on_same_line, "Braces: on the same line", "Braces: on their own line"));
    out.extend(yes(s.space_after_keyword, "A space after if, for, while: yes", "A space after if, for, while: no"));
    out.extend(yes(s.space_before_brace, "A space before the brace: yes", "A space before the brace: no"));
    out.extend(yes(s.space_after_comma, "A space after a comma: yes", "A space after a comma: no"));
    out.extend(yes(s.dot_leads_line, "A broken chain starts the line with the dot", "A broken chain ends the line with the dot"));
    out
}

/// `text` laid out the way the project does, wherever the project is consistent about it.
pub(crate) fn conformed(text: &str) -> String {
    conform(text, &style())
}
