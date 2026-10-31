//! The annotations this project always writes — `@Operation(summary = "", description = "", operationId = "")` and the
//! `@PostMapping(value = "", produces = MediaType.APPLICATION_JSON_VALUE)` under it on every endpoint — completed from
//! what the project's other members carry.
//!
//! Two caret shapes at class-member level, never inside a method signature (a parameter's `@RequestParam(` is another
//! question):
//!
//! * **inside an annotation** — `@PostMapping(|` or `@PostMapping|`: the arguments the project writes for that
//!   annotation, when one form holds most of its uses, minus the elements the line already has;
//! * **on the empty line under an annotation** — the annotation that usually comes next (`@Operation` → `@PostMapping`),
//!   written the way the project lays it out, multi-line included.
//!
//! The index is per process and in memory only. It holds what a project's annotations look like **with the text of every
//! string blanked** (`summary = ""`), counted per annotation name; each file's contribution is kept apart so a saved
//! file replaces its own, and nothing is ever written to disk. Marker annotations without arguments (`@Override`,
//! `@Autowired`, `@Test`) are not counted: they complete from the model and the other layers.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

/// An annotation name has to be written with arguments at least this many times...
const MIN_SIGHTINGS: u32 = 3;
/// ...and one form has to hold this share of them (or, for what comes next, of the successors).
const MIN_SHARE: f32 = 0.6;
/// The index never holds more distinct forms than this.
const MAX_FORMS: usize = 5_000;
/// Larger files are not read: the parse would cost more than the suggestion is worth.
const MAX_FILE_BYTES: usize = 400_000;
/// Stands in for the annotation being written, so an unfinished buffer parses.
const PLACEHOLDER: &str = "__FLC__";

/// Declarations an annotation can sit on.
const MEMBERS: &[&str] = &[
    "method_declaration",
    "constructor_declaration",
    "field_declaration",
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "annotation_type_declaration",
];

/// What the layer suggests; `text` goes in at the caret. When the editor has already closed the `(` the caller removes
/// one trailing `)` from it, as it does for the argument-list layer.
#[derive(Debug, PartialEq)]
pub(crate) struct Annot {
    pub(crate) text: String,
    pub(crate) confidence: f32,
}

// ---- reading an annotation -------------------------------------------------------------------------------------------

/// `text` with the content of every string literal (and text block) emptied: `"a\"b"` → `""`.
fn blank_strings(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'"' {
            if b[i..].starts_with(b"\"\"\"") {
                i += 3;
                while i < b.len() && !b[i..].starts_with(b"\"\"\"") {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i += 3;
            } else {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
            }
            out.push_str("\"\"");
        } else {
            let ch = text[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// The key of a form: strings blanked, whitespace collapsed, no space inside brackets or around commas, so
/// `summary = "a"\n , description = "b"` and `summary = "x", description = "y"` are the same form.
fn normalise(text: &str) -> String {
    let mut s = blank_strings(text).split_whitespace().collect::<Vec<_>>().join(" ");
    for (from, to) in [(" ,", ","), ("( ", "("), (" )", ")")] {
        while s.contains(from) {
            s = s.replace(from, to);
        }
    }
    s
}

/// The leading whitespace of the line `at` is on.
fn line_indent(source: &str, at: usize) -> &str {
    let start = source[..at].rfind('\n').map_or(0, |p| p + 1);
    let end = source[start..].find(|c: char| c != ' ' && c != '\t').map_or(source.len(), |p| start + p);
    &source[start..end]
}

/// How the annotation is written, with the strings blanked: the lines after the first lose the indentation of the line
/// the annotation starts on, so a multi-line form is relative to column 0 and can be re-indented anywhere.
fn layout(source: &str, start: usize, end: usize) -> String {
    let indent = line_indent(source, start).len();
    let blanked = blank_strings(&source[start..end]);
    let mut lines = blanked.lines();
    let mut out = String::from(lines.next().unwrap_or(""));
    for line in lines {
        let strip = line.len() - line.trim_start_matches([' ', '\t']).len();
        out.push('\n');
        out.push_str(&line[strip.min(indent)..]);
    }
    out.trim_end().to_string()
}

/// The part of a layout after its first `(`, closing parenthesis included; `None` for a form without arguments.
fn args_of(layout: &str) -> Option<&str> {
    let open = layout.find('(')?;
    let args = &layout[open + 1..];
    (!args.trim().trim_end_matches(')').trim().is_empty()).then_some(args)
}

/// The top-level `name = value` elements of normalised arguments (`a = 1, b = f(2))` → `a = 1`, `b = f(2)`).
fn elements(args: &str) -> Vec<String> {
    let body = args.trim_end().strip_suffix(')').unwrap_or(args);
    let (mut out, mut depth, mut start) = (Vec::new(), 0i32, 0usize);
    let b = body.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                out.push(body[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let last = body[start..].trim();
    if !last.is_empty() {
        out.push(last.to_string());
    }
    out
}

/// The element names an annotation's text already writes (`name =`, not `==`).
fn written_names(text: &str) -> HashSet<String> {
    let b = text.as_bytes();
    let mut out = HashSet::new();
    for (i, _) in text.match_indices('=') {
        let prev = if i > 0 { b[i - 1] } else { b' ' };
        if b.get(i + 1) == Some(&b'=') || matches!(prev, b'=' | b'!' | b'<' | b'>') {
            continue;
        }
        let head = text[..i].trim_end();
        let name: String = head.chars().rev().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect::<Vec<_>>().into_iter().rev().collect();
        if !name.is_empty() {
            out.insert(name);
        }
    }
    out
}

fn element_name(element: &str) -> Option<&str> {
    let eq = element.find('=')?;
    let name = element[..eq].trim();
    (!name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$')).then_some(name)
}

/// Parentheses opened and not closed in `s`, strings aside; `None` when a string is left open.
fn unclosed_parens(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let (mut depth, mut i) = (0i32, 0usize);
    while i < b.len() {
        match b[i] {
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                if i >= b.len() {
                    return None;
                }
            }
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    Some(depth.max(0) as usize)
}

struct Written {
    name: String,
    norm: String,
    layout: String,
    has_args: bool,
}

/// The annotations written on each member of `source`, in order, one list per member.
fn members_annotations(source: &str) -> Vec<Vec<Written>> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        if MEMBERS.contains(&node.kind()) {
            if let Some(mods) = children.iter().find(|c| c.kind() == "modifiers") {
                let mut c2 = mods.walk();
                let list: Vec<Written> = mods
                    .children(&mut c2)
                    .filter(|a| matches!(a.kind(), "annotation" | "marker_annotation"))
                    .filter_map(|a| {
                        let name = a.child_by_field_name("name")?.utf8_text(source.as_bytes()).ok()?.rsplit('.').next()?.to_string();
                        let lay = layout(source, a.start_byte(), a.end_byte());
                        Some(Written { name, norm: normalise(&source[a.start_byte()..a.end_byte()]), has_args: args_of(&lay).is_some(), layout: lay })
                    })
                    .collect();
                if !list.is_empty() {
                    out.push(list);
                }
            }
        }
        stack.extend(children);
    }
    out
}

// ---- the index -------------------------------------------------------------------------------------------------------

#[derive(Default, Clone)]
struct Form {
    count: u32,
    layouts: HashMap<String, u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) enum Part {
    Form { name: String, norm: String, layout: String },
    Next { name: String, next: String },
}

/// A project's counters summed over its files: forms `(name, normalised, count, layouts)` and successors
/// `(name, next, count)`.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub(crate) struct AnnTotals {
    pub forms: Vec<(String, String, u32, Vec<(String, u32)>)>,
    pub next: Vec<(String, String, u32)>,
}

#[derive(Default)]
pub(crate) struct Annotations {
    /// annotation name → normalised form → how often and how laid out
    forms: HashMap<String, HashMap<String, Form>>,
    /// annotation name → the name that follows it on the same member → how often
    next: HashMap<String, HashMap<String, u32>>,
    by_file: HashMap<String, Vec<Part>>,
    form_count: usize,
    /// hash of the text each file was counted from: the same text again is not parsed again
    hashes: HashMap<String, u64>,
    /// what other projects saw, consulted only for an annotation this project has too few uses of
    prior_forms: HashMap<String, HashMap<String, Form>>,
    prior_next: HashMap<String, HashMap<String, u32>>,
}

impl Annotations {
    pub(crate) fn add_file(&mut self, key: &str, source: &str) {
        let hash = crate::flc_profile::text_hash(source);
        if self.hashes.get(key) == Some(&hash) {
            return;
        }
        self.remove_file(key);
        if source.len() > MAX_FILE_BYTES {
            return;
        }
        self.hashes.insert(key.to_string(), hash);
        crate::flc_profile::mark_dirty();
        let mut parts = Vec::new();
        for member in members_annotations(source) {
            for (i, a) in member.iter().enumerate() {
                if a.has_args {
                    let known = self.forms.get(&a.name).is_some_and(|f| f.contains_key(&a.norm));
                    if known || self.form_count < MAX_FORMS {
                        let form = self.forms.entry(a.name.clone()).or_default().entry(a.norm.clone()).or_insert_with(|| {
                            self.form_count += 1;
                            Form::default()
                        });
                        form.count += 1;
                        *form.layouts.entry(a.layout.clone()).or_insert(0) += 1;
                        parts.push(Part::Form { name: a.name.clone(), norm: a.norm.clone(), layout: a.layout.clone() });
                    }
                }
                if let Some(after) = member.get(i + 1) {
                    *self.next.entry(a.name.clone()).or_default().entry(after.name.clone()).or_insert(0) += 1;
                    parts.push(Part::Next { name: a.name.clone(), next: after.name.clone() });
                }
            }
        }
        self.by_file.insert(key.to_string(), parts);
    }

    /// Every file counted, with the hash of its text and its parts — for the profile on disk.
    pub(crate) fn snapshot(&self, include: &dyn Fn(&str) -> bool) -> Vec<(String, u64, Vec<Part>)> {
        self.hashes.iter().filter(|(k, _)| include(k)).map(|(k, h)| (k.clone(), *h, self.by_file.get(k).cloned().unwrap_or_default())).collect()
    }

    /// Puts back what a saved profile says `key` contributed, without reading the file.
    pub(crate) fn restore_file(&mut self, key: &str, hash: u64, parts: Vec<Part>) {
        self.remove_file(key);
        let mut kept = Vec::with_capacity(parts.len());
        for part in parts {
            match &part {
                Part::Form { name, norm, layout } => {
                    let known = self.forms.get(name).is_some_and(|f| f.contains_key(norm));
                    if !known && self.form_count >= MAX_FORMS {
                        continue;
                    }
                    let form = self.forms.entry(name.clone()).or_default().entry(norm.clone()).or_insert_with(|| {
                        self.form_count += 1;
                        Form::default()
                    });
                    form.count += 1;
                    *form.layouts.entry(layout.clone()).or_insert(0) += 1;
                }
                Part::Next { name, next } => {
                    *self.next.entry(name.clone()).or_default().entry(next.clone()).or_insert(0) += 1;
                }
            }
            kept.push(part);
        }
        self.by_file.insert(key.to_string(), kept);
        self.hashes.insert(key.to_string(), hash);
    }

    pub(crate) fn totals(&self, include: &dyn Fn(&str) -> bool) -> AnnTotals {
        let mut forms: HashMap<(String, String), (u32, HashMap<String, u32>)> = HashMap::new();
        let mut next: HashMap<(String, String), u32> = HashMap::new();
        for key in self.hashes.keys().filter(|k| include(k)) {
            for part in self.by_file.get(key).into_iter().flatten() {
                match part {
                    Part::Form { name, norm, layout } => {
                        let e = forms.entry((name.clone(), norm.clone())).or_default();
                        e.0 += 1;
                        *e.1.entry(layout.clone()).or_insert(0) += 1;
                    }
                    Part::Next { name, next: n } => *next.entry((name.clone(), n.clone())).or_insert(0) += 1,
                }
            }
        }
        let mut forms: Vec<_> = forms
            .into_iter()
            .map(|((n, f), (c, l))| {
                let mut l: Vec<_> = l.into_iter().collect();
                l.sort();
                (n, f, c, l)
            })
            .collect();
        forms.sort();
        let mut next: Vec<_> = next.into_iter().map(|((a, b), c)| (a, b, c)).collect();
        next.sort();
        AnnTotals { forms, next }
    }

    pub(crate) fn set_prior(&mut self, totals: &AnnTotals) {
        self.prior_forms.clear();
        self.prior_next.clear();
        for (name, norm, count, layouts) in &totals.forms {
            let form = self.prior_forms.entry(name.clone()).or_default().entry(norm.clone()).or_default();
            form.count += count;
            for (l, n) in layouts {
                *form.layouts.entry(l.clone()).or_insert(0) += n;
            }
        }
        for (a, b, n) in &totals.next {
            *self.prior_next.entry(a.clone()).or_default().entry(b.clone()).or_insert(0) += n;
        }
    }

    pub(crate) fn clear(&mut self) {
        *self = Annotations::default();
    }

    pub(crate) fn files(&self) -> Vec<String> {
        self.hashes.keys().cloned().collect()
    }

    pub(crate) fn remove_file(&mut self, key: &str) {
        if self.hashes.remove(key).is_some() {
            crate::flc_profile::mark_dirty();
        }
        for part in self.by_file.remove(key).unwrap_or_default() {
            match part {
                Part::Form { name, norm, layout } => {
                    let Some(forms) = self.forms.get_mut(&name) else { continue };
                    let gone = forms.get_mut(&norm).is_some_and(|f| {
                        f.count = f.count.saturating_sub(1);
                        if let Some(n) = f.layouts.get_mut(&layout) {
                            *n -= 1;
                            if *n == 0 {
                                f.layouts.remove(&layout);
                            }
                        }
                        f.count == 0
                    });
                    if gone {
                        forms.remove(&norm);
                        self.form_count = self.form_count.saturating_sub(1);
                    }
                    if forms.is_empty() {
                        self.forms.remove(&name);
                    }
                }
                Part::Next { name, next } => {
                    let Some(after) = self.next.get_mut(&name) else { continue };
                    if let Some(n) = after.get_mut(&next) {
                        *n -= 1;
                        if *n == 0 {
                            after.remove(&next);
                        }
                    }
                    if after.is_empty() {
                        self.next.remove(&name);
                    }
                }
            }
        }
    }

    /// The form of `name` that holds most of its uses — `(normalised, its commonest layout, share)` — when there are
    /// enough of them and one form clearly leads.
    fn dominant_form(&self, name: &str) -> Option<(String, String, f32)> {
        let mut forms: HashMap<&String, Form> = self.forms.get(name).into_iter().flatten().map(|(n, f)| (n, f.clone())).collect();
        // too few uses here: the other projects' are added
        if forms.values().map(|f| f.count).sum::<u32>() < MIN_SIGHTINGS {
            for (norm, other) in self.prior_forms.get(name).into_iter().flatten() {
                let f = forms.entry(norm).or_default();
                f.count += other.count;
                for (l, n) in &other.layouts {
                    *f.layouts.entry(l.clone()).or_insert(0) += n;
                }
            }
        }
        let total: u32 = forms.values().map(|f| f.count).sum();
        let (norm, form) = forms.iter().max_by(|a, b| a.1.count.cmp(&b.1.count).then_with(|| b.0.cmp(a.0)))?;
        let share = form.count as f32 / total as f32;
        let (layout, _) = form.layouts.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))?;
        (total >= MIN_SIGHTINGS && share >= MIN_SHARE).then(|| ((*norm).clone(), layout.clone(), share))
    }

    /// The annotation that most often comes right after `name`, with its share of what follows.
    fn successor(&self, name: &str) -> Option<(String, f32)> {
        let mut after: HashMap<&String, u32> = self.next.get(name).into_iter().flatten().map(|(n, c)| (n, *c)).collect();
        if after.values().sum::<u32>() < MIN_SIGHTINGS {
            for (n, c) in self.prior_next.get(name).into_iter().flatten() {
                *after.entry(n).or_insert(0) += c;
            }
        }
        let total: u32 = after.values().sum();
        let (next, count) = after.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))?;
        let share = *count as f32 / total as f32;
        (total >= MIN_SIGHTINGS && share >= MIN_SHARE).then(|| ((*next).clone(), share))
    }

    pub(crate) fn suggest(&self, source: &str, offset: usize) -> Option<Annot> {
        if source.len() > MAX_FILE_BYTES || (self.forms.is_empty() && self.prior_forms.is_empty()) || offset > source.len() || !source.is_char_boundary(offset) {
            return None;
        }
        let line_start = source[..offset].rfind('\n').map_or(0, |p| p + 1);
        let eol = source[offset..].find('\n').map_or(source.len(), |p| offset + p);
        // only the editor's own closer may stand after the caret
        if !matches!(source[offset..eol].trim(), "" | ")") {
            return None;
        }
        let prefix = &source[line_start..offset];
        if prefix.trim().is_empty() {
            self.next_annotation(source, offset, eol, prefix)
        } else if prefix.trim_start().starts_with('@') {
            self.inside_annotation(source, offset, eol, prefix)
        } else {
            None
        }
    }

    /// `@Name|` and `@Name(|`, `@Name(a = 1, |`: the arguments the project writes for `Name`.
    fn inside_annotation(&self, source: &str, offset: usize, eol: usize, prefix: &str) -> Option<Annot> {
        let typed = prefix.trim_start();
        let name: String = typed[1..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        let after = &typed[1 + name.len()..];
        if name.is_empty() || !(after.is_empty() || after.starts_with('(')) {
            return None;
        }
        let (norm, lay, share) = self.dominant_form(&name)?;
        let args_prefix = after.strip_prefix('(');
        // the buffer closed the way the probe needs it: the annotation ends here and a declaration follows
        let closers = ")".repeat(unclosed_parens(typed)?);
        if !on_a_member(&format!("{}{closers} void __flc__() {{}}{}", &source[..offset], &source[eol..]), offset - 1) {
            return None;
        }
        let indent = &prefix[..prefix.len() - typed.len()];
        let text = match args_prefix {
            // `@Name` alone becomes `@Name(…)`: the opening parenthesis comes with the text
            None => format!("({}", reindent(args_of(&lay)?, indent)),
            Some(p) if p.trim().is_empty() => reindent(args_of(&lay)?, indent),
            // `@PostMapping(value = |`: the value the project gives that element, then the elements that follow it
            Some(p) if p.trim_end().ends_with('=') => {
                let head = p.trim_end().trim_end_matches('=').trim_end();
                let name: String = head.chars().rev().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect::<Vec<_>>().into_iter().rev().collect();
                let all = elements(args_of(&norm)?);
                let at = all.iter().position(|e| element_name(e) == Some(name.as_str()))?;
                let value = all[at].split_once('=')?.1.trim();
                let have = written_names(head);
                let mut out = format!("{}{value}", if p.ends_with(' ') { "" } else { " " });
                for e in all[at + 1..].iter().filter(|e| element_name(e).is_some_and(|n| !have.contains(n))) {
                    out.push_str(", ");
                    out.push_str(e);
                }
                out.push(')');
                out
            }
            Some(p) => {
                // elements already written are not written again; what is left goes on the same line
                if !p.trim_end().ends_with(',') {
                    return None;
                }
                let have = written_names(p);
                let left: Vec<String> = elements(args_of(&norm)?).into_iter().filter(|e| element_name(e).is_some_and(|n| !have.contains(n))).collect();
                if left.is_empty() {
                    return None;
                }
                format!("{}{})", if p.ends_with(' ') { "" } else { " " }, left.join(", "))
            }
        };
        Some(Annot { text, confidence: (0.6 + 0.3 * share).min(0.9) })
    }

    /// The empty line under an annotation: the annotation that usually comes next.
    fn next_annotation(&self, source: &str, offset: usize, eol: usize, prefix: &str) -> Option<Annot> {
        if !source[offset..eol].trim().is_empty() {
            return None;
        }
        let probe = format!("{}@{PLACEHOLDER} void __flc__() {{}}{}", &source[..offset], &source[eol..]);
        let tree = bennu_java::prelude::parse_java(&probe)?;
        let node = tree.root_node().descendant_for_byte_range(offset, offset + 1)?;
        let mut ann = node;
        while !matches!(ann.kind(), "annotation" | "marker_annotation") {
            ann = ann.parent()?;
        }
        let mods = ann.parent().filter(|m| m.kind() == "modifiers")?;
        let member = mods.parent().filter(|m| MEMBERS.contains(&m.kind()))?;
        if !member.parent().is_some_and(|p| p.kind().ends_with("body") || p.kind().ends_with("body_declarations")) {
            return None;
        }
        let mut cursor = mods.walk();
        let before: Vec<_> = mods.children(&mut cursor).filter(|c| matches!(c.kind(), "annotation" | "marker_annotation") && c.end_byte() <= offset).collect();
        let prev = before.last()?;
        // the line right above is the end of that annotation
        if !probe[prev.end_byte()..offset].trim().is_empty() || probe[prev.end_byte()..offset].matches('\n').count() != 1 {
            return None;
        }
        let prev_name = prev.child_by_field_name("name")?.utf8_text(probe.as_bytes()).ok()?.rsplit('.').next()?.to_string();
        let (next, next_share) = self.successor(&prev_name)?;
        // a member that has it already does not need it again
        let have: HashSet<String> = before.iter().filter_map(|a| a.child_by_field_name("name")?.utf8_text(probe.as_bytes()).ok().map(|s| s.rsplit('.').next().unwrap_or(s).to_string())).collect();
        if have.contains(&next) {
            return None;
        }
        let (_, lay, form_share) = self.dominant_form(&next)?;
        let member_indent = line_indent(&probe, prev.start_byte());
        let missing = member_indent.strip_prefix(prefix).unwrap_or("");
        Some(Annot { text: format!("{missing}{}", reindent(&lay, member_indent)), confidence: (0.6 + 0.3 * next_share.min(form_share)).min(0.9) })
    }
}

/// Whether the annotation that ends at `at` in `probe` sits on a member of a class body.
fn on_a_member(probe: &str, at: usize) -> bool {
    let Some(tree) = bennu_java::prelude::parse_java(probe) else { return false };
    let Some(mut node) = tree.root_node().descendant_for_byte_range(at, at + 1) else { return false };
    while !matches!(node.kind(), "annotation" | "marker_annotation") {
        match node.parent() {
            Some(p) => node = p,
            None => return false,
        }
    }
    let Some(mods) = node.parent().filter(|m| m.kind() == "modifiers") else { return false };
    let Some(member) = mods.parent().filter(|m| MEMBERS.contains(&m.kind())) else { return false };
    member.parent().is_some_and(|p| p.kind().ends_with("body") || p.kind().ends_with("body_declarations"))
}

/// Gives every line after the first the indentation `indent`, on top of the layout's own.
fn reindent(text: &str, indent: &str) -> String {
    let mut out = String::new();
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
            if !line.is_empty() {
                out.push_str(indent);
            }
        }
        out.push_str(line);
    }
    out
}

// ---- the process-wide index ------------------------------------------------------------------------------------------

fn global() -> &'static Mutex<Annotations> {
    static STATE: OnceLock<Mutex<Annotations>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(Annotations::default()))
}

/// Counts the annotations of `source` under `key`, replacing what the same key held before.
pub(crate) fn add_file(key: &str, source: &str) {
    global().lock().unwrap_or_else(|p| p.into_inner()).add_file(key, source);
}

/// Takes back what `key` held (a deleted file).
pub(crate) fn remove_file(key: &str) {
    global().lock().unwrap_or_else(|p| p.into_inner()).remove_file(key);
}

fn lock() -> std::sync::MutexGuard<'static, Annotations> {
    global().lock().unwrap_or_else(|p| p.into_inner())
}

pub(crate) fn snapshot(include: &dyn Fn(&str) -> bool) -> Vec<(String, u64, Vec<Part>)> {
    lock().snapshot(include)
}

pub(crate) fn restore_file(key: &str, hash: u64, parts: Vec<Part>) {
    lock().restore_file(key, hash, parts);
}

pub(crate) fn totals(include: &dyn Fn(&str) -> bool) -> AnnTotals {
    lock().totals(include)
}

pub(crate) fn set_prior(totals: &AnnTotals) {
    lock().set_prior(totals);
}

pub(crate) fn clear() {
    lock().clear();
}

pub(crate) fn sizes() -> crate::flc_memory::Sizes {
    use crate::flc_memory::{text, two_level, Sizes};
    let h = lock();
    let forms = |t: &HashMap<String, HashMap<String, Form>>| -> usize {
        t.iter()
            .map(|(name, row)| {
                text(name) + row.iter().map(|(norm, f)| text(norm) + 4 + f.layouts.keys().map(|l| text(l) + 4 + 48).sum::<usize>() + 48).sum::<usize>()
            })
            .sum()
    };
    let mut s = Sizes::default();
    for (k, parts) in &h.by_file {
        let bytes = parts
            .iter()
            .map(|p| match p {
                Part::Form { name, norm, layout } => text(name) + text(norm) + text(layout),
                Part::Next { name, next } => text(name) + text(next),
            })
            .sum();
        s.add_file(k, bytes);
    }
    s.shared = forms(&h.forms) + two_level(&h.next, |k| text(k));
    s.shared_entries = h.form_count;
    s.carried = forms(&h.prior_forms) + two_level(&h.prior_next, |k| text(k));
    s
}

pub(crate) fn files() -> Vec<String> {
    lock().files()
}

/// The annotation arguments, or the next annotation, the project writes here; see the module doc for the two shapes.
pub(crate) fn suggest(_file: &str, source: &str, offset: usize) -> Option<Annot> {
    global().lock().unwrap_or_else(|p| p.into_inner()).suggest(source, offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn controller(name: &str, strings: [&str; 4]) -> String {
        format!(
            "package app;\n\n@RestController\npublic class {name}Controller {{\n\n    @Operation(\n        summary = \"{}\",\n        description = \"{}\",\n        operationId = \"{}\"\n    )\n    @PostMapping(value = \"{}\", produces = MediaType.APPLICATION_JSON_VALUE)\n    public ResponseEntity<{name}> create(@RequestBody {name} body) {{\n        return ok(body);\n    }}\n\n    @Override\n    public String toString() {{\n        return \"x\";\n    }}\n}}\n",
            strings[0], strings[1], strings[2], strings[3]
        )
    }

    fn index() -> Annotations {
        let mut ix = Annotations::default();
        for (n, name) in ["Order", "Customer", "Invoice", "Offer"].iter().enumerate() {
            ix.add_file(&format!("{name}.java"), &controller(name, [&format!("s{n}"), &format!("d{n}"), &format!("o{n}"), &format!("/p{n}")]));
        }
        ix
    }

    const HEAD: &str = "package app;\n\npublic class SupplierController {\n\n";

    /// The buffer being written: `typed` on the member line, the editor's `closer` after the caret, a method below.
    fn buffer(typed: &str, closer: &str) -> (String, usize) {
        let src = format!("{HEAD}{typed}{closer}\n    public ResponseEntity<Supplier> create() {{ return null; }}\n}}\n");
        (src, HEAD.len() + typed.len())
    }

    #[test]
    fn strings_are_blanked_and_whitespace_collapsed_in_the_form() {
        assert_eq!(normalise("@Operation(summary = \"a b\"\n    , description = \"c\\\"d\")"), "@Operation(summary = \"\", description = \"\")");
        assert_eq!(normalise("@Max(10)"), "@Max(10)");
        assert_eq!(blank_strings("x = \"\"\"\nline\n\"\"\"; y = 'q'"), "x = \"\"; y = 'q'");
    }

    #[test]
    fn a_multi_line_layout_is_relative_to_the_annotation() {
        let src = "class A {\n    @Operation(\n        summary = \"x\",\n        description = \"y\"\n    )\n    void f() {}\n}\n";
        let start = src.find('@').unwrap();
        let end = src.find(")\n    void").unwrap() + 1;
        assert_eq!(layout(src, start, end), "@Operation(\n    summary = \"\",\n    description = \"\"\n)");
    }

    #[test]
    fn the_arguments_of_a_dominant_form_complete_after_the_parenthesis() {
        let ix = index();
        let (src, at) = buffer("    @PostMapping(", ")");
        let got = ix.suggest(&src, at).expect("a form that leads");
        assert_eq!(got.text, "value = \"\", produces = MediaType.APPLICATION_JSON_VALUE)");
        assert!((got.confidence - 0.9).abs() < 1e-6, "{}", got.confidence);
    }

    #[test]
    fn a_multi_line_form_is_laid_out_and_indented_at_the_caret() {
        let ix = index();
        let (src, at) = buffer("    @Operation(", ")");
        let got = ix.suggest(&src, at).expect("a form that leads");
        assert_eq!(got.text, "\n        summary = \"\",\n        description = \"\",\n        operationId = \"\"\n    )");
    }

    #[test]
    fn a_name_alone_gets_its_parenthesis_and_arguments() {
        let ix = index();
        let (src, at) = buffer("    @PostMapping", "");
        assert_eq!(ix.suggest(&src, at).unwrap().text, "(value = \"\", produces = MediaType.APPLICATION_JSON_VALUE)");
    }

    #[test]
    fn elements_already_written_are_not_written_again() {
        let ix = index();
        let (src, at) = buffer("    @PostMapping(value = \"/x\", ", ")");
        assert_eq!(ix.suggest(&src, at).unwrap().text, "produces = MediaType.APPLICATION_JSON_VALUE)");
        let (src, at) = buffer("    @PostMapping(value = \"/x\", produces = A, ", ")");
        assert_eq!(ix.suggest(&src, at), None);
        // the value of the element being written is the project's, and the elements after it follow
        let (src, at) = buffer("    @PostMapping(value = ", ")");
        assert_eq!(ix.suggest(&src, at).unwrap().text, "\"\", produces = MediaType.APPLICATION_JSON_VALUE)");
        let (src, at) = buffer("    @PostMapping(produces =", ")");
        assert_eq!(ix.suggest(&src, at).unwrap().text, " MediaType.APPLICATION_JSON_VALUE)");
        // a half-written element is not completed
        let (src, at) = buffer("    @PostMapping(value = \"/x\", pro", ")");
        assert_eq!(ix.suggest(&src, at), None);
    }

    #[test]
    fn the_next_annotation_comes_under_the_previous_one() {
        let ix = index();
        let (src, at) = buffer("    @Operation(summary = \"s\", description = \"d\", operationId = \"o\")\n    ", "");
        let got = ix.suggest(&src, at).expect("a successor that leads");
        assert_eq!(got.text, "@PostMapping(value = \"\", produces = MediaType.APPLICATION_JSON_VALUE)");
        // not when the member has it already
        let (src, at) = buffer("    @PostMapping(\"/a\")\n    @Operation(summary = \"s\", description = \"d\", operationId = \"o\")\n    ", "");
        assert_eq!(ix.suggest(&src, at), None);
    }

    #[test]
    fn nothing_is_said_inside_a_method_signature_or_under_no_annotation() {
        let ix = index();
        let src = format!("{HEAD}    public void f(@PostMapping(\n    ) {{}}\n}}\n");
        let at_paren = HEAD.len() + "    public void f(@PostMapping(".len();
        assert_eq!(ix.suggest(&src, at_paren), None);
        let (src, at) = buffer("    private int x;\n    ", "");
        assert_eq!(ix.suggest(&src, at), None);
    }

    #[test]
    fn a_name_seen_too_few_times_or_with_no_leading_form_gives_nothing() {
        let mut ix = Annotations::default();
        ix.add_file("A.java", &controller("A", ["a", "b", "c", "/a"]));
        ix.add_file("B.java", &controller("B", ["a", "b", "c", "/b"]));
        let (src, at) = buffer("    @PostMapping(", ")");
        assert_eq!(ix.suggest(&src, at), None, "two sightings are not enough");
        // three sightings, three different forms: none leads
        let mut ix = Annotations::default();
        for (n, args) in ["value = \"\"", "path = \"\"", "name = \"\""].iter().enumerate() {
            ix.add_file(&format!("{n}.java"), &format!("class C{n} {{\n    @PostMapping({args})\n    void f() {{}}\n}}\n"));
        }
        assert_eq!(ix.suggest(&src, at), None);
    }

    #[test]
    fn marker_annotations_are_not_counted() {
        let ix = index();
        assert!(ix.forms.get("Override").is_none() && ix.forms.get("RestController").is_none());
        let (src, at) = buffer("    @Override\n    ", "");
        assert_eq!(ix.suggest(&src, at), None);
    }

    #[test]
    fn a_resaved_file_replaces_its_own_contribution() {
        let mut ix = index();
        assert!(ix.dominant_form("PostMapping").is_some());
        for name in ["Order", "Customer", "Invoice"] {
            ix.add_file(&format!("{name}.java"), "class Gone { }\n");
        }
        // one sighting left: not enough
        assert!(ix.dominant_form("PostMapping").is_none());
        ix.remove_file("Offer.java");
        assert!(ix.forms.is_empty() && ix.next.is_empty() && ix.form_count == 0);
    }
}
