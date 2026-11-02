//! The arguments this project always passes — `.filter(Boolean.FALSE::equals)`, `.map(it -> it.name())`,
//! `.orElseThrow(NotFoundException::new)` — so the ghost text proposes the usual one and the completion list puts it
//! first when you start to type it.
//!
//! For every call in the project's files: the callee's simple name, the position of the argument, and the argument's
//! text, when it is short, on one line and not a literal or a plain local variable (those are the scope's business, not
//! a habit). Counts of that text per `(callee, position)`, each file's contribution kept apart so a saved file
//! replaces its own.
//!
//! A plain local variable or parameter inside the argument is not part of the habit: `new Factory().create(v1)` and
//! `new Factory().create(v2)` are the same habit. Each such name is counted as a typed slot, `$0:Type` (the declared
//! type's simple name, `?` for a `var`), so the table holds one template, `new Factory().create($0:Order)`, with the
//! sightings of every spelling. When the template is drawn each slot is filled with the nearest variable in scope whose
//! type is that one (or a subtype of it); where no variable fits, nothing is drawn — a name is never invented.
//!
//! A second table keeps which plain local variable is passed at each place (`transfer(from, to)`: `from` in slot 0),
//! so an argument list with two equally good variables takes the one the project always passes there.
//!
//! The tables hold short pieces of source text. They are written to disk only through `flc_profile` — in the user's
//! Arbor data folder, when the user's settings allow it — and never anywhere else.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use bennu_complete::prelude::{MatchCase, Typed};
use bennu_proto::prelude::CompletionItem;

/// The usual argument has to hold this share of the arguments seen for that place...
const MIN_SHARE: f32 = 0.6;
/// ...over at least this many sightings in other files.
const MIN_SIGHTINGS: u32 = 3;
/// An argument longer than this is not a habit, it is a piece of code.
const MAX_LEN: usize = 60;
/// At most this many distinct (callee, position, text) entries; past it a new one is simply not counted.
const MAX_ENTRIES: usize = 30_000;
/// A row's label is the whole argument when it is this short, else only its first word.
const MAX_LABEL: usize = 40;
/// Rows the popup gets at most.
const MAX_ROWS: usize = 4;

type Place = (String, usize);

#[derive(Default)]
pub(crate) struct Habits {
    table: HashMap<Place, HashMap<String, u32>>,
    entries: usize,
    /// What each file contributed, to take it back and to leave it out of its own vote.
    by_file: HashMap<String, Vec<(Place, String)>>,
    /// The plain variable names passed at each place, and what each file contributed to them.
    names: HashMap<Place, HashMap<String, u32>>,
    name_entries: usize,
    name_by_file: HashMap<String, Vec<(Place, String)>>,
    /// Hash of the text each file was counted from: the same text again is not read again.
    hashes: HashMap<String, u64>,
    /// What other projects saw, consulted only where this project has too little (see `flc_profile`).
    prior_texts: HashMap<Place, HashMap<String, u32>>,
    prior_names: HashMap<Place, HashMap<String, u32>>,
}

/// One file's contribution, as plain data: `(callee, position, text)`.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub(crate) struct HabitPart {
    pub texts: Vec<(String, usize, String)>,
    pub names: Vec<(String, usize, String)>,
}

/// A project's counters summed over its files: `(callee, position, text, count)`.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub(crate) struct HabitTotals {
    pub texts: Vec<(String, usize, String, u32)>,
    pub names: Vec<(String, usize, String, u32)>,
}

/// A usual argument: its text and how it stands.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Habit {
    pub text: String,
    pub confidence: f32,
}

/// `filter#0`: where in a call an argument was seen, as a person reads it and as a rule keys it.
fn place_key(place: &Place) -> String {
    format!("{}#{}", place.0, place.1)
}

/// What the project's argument habits hold, for the page that shows them: per place, its texts by how often they were seen.
pub(crate) fn report(names: bool) -> Vec<(String, Vec<(String, u32)>)> {
    let h = lock();
    let table = if names { &h.names } else { &h.table };
    let mut out: Vec<(String, Vec<(String, u32)>)> = table
        .iter()
        .map(|(place, texts)| {
            let mut rows: Vec<(String, u32)> = texts.iter().map(|(t, n)| (t.clone(), *n)).collect();
            rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            (place_key(place), rows)
        })
        .collect();
    out.sort_by_key(|(key, rows)| (std::cmp::Reverse(rows.iter().map(|r| r.1).sum::<u32>()), key.clone()));
    out
}

/// A variable in scope, as a slot sees it: its name, where it was declared, and the simple names of its type and of every
/// type above it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Candidate {
    pub name: String,
    pub decl: usize,
    pub types: Vec<String>,
}

/// A template is literal text and typed slots (`$0:Order`).
#[derive(Debug, PartialEq)]
enum Part<'a> {
    Lit(&'a str),
    Slot(usize, &'a str),
}

fn parts(template: &str) -> Vec<Part<'_>> {
    let mut out = Vec::new();
    let mut lit = 0;
    let mut i = 0;
    let bytes = template.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let digits = template[i + 1..].bytes().take_while(u8::is_ascii_digit).count();
            let at = i + 1 + digits;
            if digits > 0 && bytes.get(at) == Some(&b':') {
                let ty_len = if bytes.get(at + 1) == Some(&b'?') {
                    1
                } else {
                    let word = template[at + 1..].bytes().take_while(|b| b.is_ascii_alphanumeric() || *b == b'_').count();
                    if word == 0 { 0 } else { word + 2 * prefix_brackets(&template[at + 1 + word..]) }
                };
                if ty_len > 0 {
                    if lit < i {
                        out.push(Part::Lit(&template[lit..i]));
                    }
                    out.push(Part::Slot(template[i + 1..at].parse().unwrap_or(0), &template[at + 1..at + 1 + ty_len]));
                    i = at + 1 + ty_len;
                    lit = i;
                    continue;
                }
            }
        }
        i += 1;
    }
    if lit < bytes.len() {
        out.push(Part::Lit(&template[lit..]));
    }
    out
}

/// How many `[]` pairs `rest` starts with.
fn prefix_brackets(rest: &str) -> usize {
    let mut n = 0;
    while rest[n * 2..].starts_with("[]") {
        n += 1;
    }
    n
}

fn has_slots(template: &str) -> bool {
    parts(template).iter().any(|p| matches!(p, Part::Slot(..)))
}

/// Whether `template` can still become `typed` once written out: its literal pieces agree and a slot stands for any word.
fn can_start_with(template: &str, typed: &str) -> bool {
    let mut at = 0;
    for part in parts(template) {
        let rest = &typed[at..];
        match part {
            Part::Lit(l) if rest.starts_with(l) => at += l.len(),
            Part::Lit(l) => return l.starts_with(rest),
            Part::Slot(..) => at += rest.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).unwrap_or(rest.len()),
        }
        if at == typed.len() {
            return true;
        }
    }
    true
}

/// The template with each slot filled by the nearest variable in `scope` of that type; `None` when one has no variable.
/// Two slots never take the same variable unless they are the same slot.
fn instantiate(template: &str, scope: &[Candidate]) -> Option<String> {
    let mut chosen: HashMap<usize, &str> = HashMap::new();
    let mut out = String::with_capacity(template.len());
    for part in parts(template) {
        match part {
            Part::Lit(l) => out.push_str(l),
            Part::Slot(n, ty) => {
                if !chosen.contains_key(&n) {
                    let taken: Vec<&str> = chosen.values().copied().collect();
                    let best = scope
                        .iter()
                        .filter(|c| ty != "?" && !c.name.starts_with('_') && !taken.contains(&c.name.as_str()) && c.types.iter().any(|t| t == ty))
                        .max_by_key(|c| c.decl)?;
                    chosen.insert(n, &best.name);
                }
                out.push_str(chosen[&n]);
            }
        }
    }
    Some(out)
}

fn first_word(text: &str) -> &str {
    let end = text.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).unwrap_or(text.len());
    &text[..end]
}

/// The texts that start with the same word as the most used one, when together they hold the share a habit needs and
/// there is more than one of them: `(what they share up to a `.`, their share, their sightings)`. `None` when the
/// group shares nothing past that word.
fn leading_group(seen: &[(String, u32)], total: u32) -> Option<(String, f32, u32)> {
    let word = first_word(&seen.first()?.0);
    let group: Vec<&(String, u32)> = seen.iter().filter(|(t, _)| first_word(t) == word).collect();
    let count: u32 = group.iter().map(|(_, n)| n).sum();
    let share = count as f32 / total as f32;
    if group.len() < 2 || count < MIN_SIGHTINGS || share < MIN_SHARE {
        return None;
    }
    let mut common = group[0].0.as_str();
    for (t, _) in &group[1..] {
        let same = common.char_indices().zip(t.chars()).take_while(|((_, a), b)| a == b).last().map_or(0, |((i, c), _)| i + c.len_utf8());
        common = &common[..same];
    }
    // only whole pieces: up to and including the last `.`, the arrow of a lambda (`it -> `), or the opening parenthesis of
    // the `new Type(` that all of them start with — what goes inside is a variable, and a different one each time
    let arrow = common.find("-> ").map(|a| a + 3);
    let constructor = common.strip_prefix("new ").and_then(|_| common.find('(')).map(|p| p + 1);
    let cut = common.rfind('.').map(|d| d + 1).max(arrow).max(constructor)?;
    (cut > word.len()).then(|| (common[..cut].to_string(), share, count))
}

/// Whether the argument is the kind worth counting as a habit.
fn habit_worthy(arg: tree_sitter::Node, source: &str) -> bool {
    let Ok(text) = arg.utf8_text(source.as_bytes()).map(str::trim) else { return false };
    if text.is_empty() || text.len() > MAX_LEN || text.contains('\n') {
        return false;
    }
    let capitalised = text.starts_with(|c: char| c.is_ascii_uppercase());
    match arg.kind() {
        // literals say nothing of a habit, `this` and a lowercase local are the scope's
        "string_literal" | "character_literal" | "decimal_integer_literal" | "hex_integer_literal" | "octal_integer_literal"
        | "binary_integer_literal" | "decimal_floating_point_literal" | "hex_floating_point_literal" | "null_literal" | "true"
        | "false" | "text_block" | "this" => false,
        "identifier" => capitalised,
        "lambda_expression" | "method_reference" => true,
        // `Type.CONST`, `Type.method()`, `Foo.class`, `new Foo<>()`: a form somebody typed on purpose
        _ => capitalised || text.starts_with("new "),
    }
}

/// The argument worth counting as a habit, with each plain local variable in it replaced by a typed slot, or `None`.
fn habit_text(arg: tree_sitter::Node, source: &str) -> Option<String> {
    if !habit_worthy(arg, source) {
        return None;
    }
    let text = arg.utf8_text(source.as_bytes()).ok()?;
    let start = arg.start_byte();
    let mut uses = local_uses(arg, source, &declared_locals(arg, source));
    if uses.is_empty() {
        return Some(text.trim().to_string());
    }
    uses.sort_by_key(|u| u.0);
    let mut slots: Vec<&str> = Vec::new();
    let mut out = String::with_capacity(text.len());
    let mut at = start;
    for (from, to, name, ty) in &uses {
        let name = name.as_str();
        let n = slots.iter().position(|s| *s == name).unwrap_or_else(|| {
            slots.push(name);
            slots.len() - 1
        });
        out.push_str(&source[at..*from]);
        out.push_str(&format!("${n}:{ty}"));
        at = *to;
    }
    out.push_str(&source[at..arg.end_byte()]);
    Some(out.trim().to_string())
}

/// The simple name of a declared type, as a slot spells it: no package, no generics, array brackets kept; `?` for a `var`.
fn slot_type(declared: &str) -> String {
    let mut bare = String::new();
    let mut depth = 0usize;
    for c in declared.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            c if depth == 0 && !c.is_whitespace() => bare.push(c),
            _ => {}
        }
    }
    let dims = bare.matches("[]").count();
    let name = bare.trim_end_matches("[]").rsplit('.').next().unwrap_or("");
    if name.is_empty() || name == "var" || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return "?".to_string();
    }
    format!("{name}{}", "[]".repeat(dims))
}

/// The local variables and parameters declared before `arg` in the scopes that enclose it: `name → slot type`.
fn declared_locals(arg: tree_sitter::Node, source: &str) -> HashMap<String, String> {
    use tree_sitter::Node;
    let text = |n: Node| n.utf8_text(source.as_bytes()).unwrap_or("").to_string();
    let start = arg.start_byte();
    let mut out: HashMap<String, String> = HashMap::new();
    let mut put = |name: Option<Node>, ty: Option<Node>| {
        if let Some(name) = name {
            out.entry(text(name)).or_insert_with(|| ty.map_or_else(|| "?".to_string(), |t| slot_type(&text(t))));
        }
    };
    let mut params = |list: Node, put: &mut dyn FnMut(Option<Node>, Option<Node>)| {
        let mut cursor = list.walk();
        for p in list.named_children(&mut cursor) {
            match p.kind() {
                "formal_parameter" | "spread_parameter" => {
                    // a spread parameter keeps its name in a declarator
                    let name = p.child_by_field_name("name").or_else(|| {
                        let mut c = p.walk();
                        let d = p.named_children(&mut c).find(|c| c.kind() == "variable_declarator");
                        d.and_then(|d| d.child_by_field_name("name"))
                    });
                    put(name, p.child_by_field_name("type"));
                }
                "identifier" => put(Some(p), None),
                _ => {}
            }
        }
    };
    let mut cur = arg.parent();
    while let Some(a) = cur {
        match a.kind() {
            "method_declaration" | "constructor_declaration" => {
                if let Some(list) = a.child_by_field_name("parameters") {
                    params(list, &mut put);
                }
            }
            "lambda_expression" => match a.child_by_field_name("parameters") {
                Some(list) if list.kind() == "identifier" => put(Some(list), None),
                Some(list) => params(list, &mut put),
                None => {}
            },
            "enhanced_for_statement" => put(a.child_by_field_name("name"), a.child_by_field_name("type")),
            "catch_clause" => {
                let mut c = a.walk();
                let param = a.named_children(&mut c).find(|c| c.kind() == "catch_formal_parameter");
                if let Some(param) = param {
                    let mut c = param.walk();
                    let types: Vec<Node> = param.named_children(&mut c).filter(|c| c.kind() == "catch_type").collect();
                    // a multi-catch has no one type to state
                    let single = types.first().filter(|t| !text(**t).contains('|'));
                    put(param.child_by_field_name("name"), single.copied());
                }
            }
            _ => {}
        }
        // declarations that sit among the node's own children, before the argument
        let mut cursor = a.walk();
        for child in a.named_children(&mut cursor) {
            let decl = match child.kind() {
                "local_variable_declaration" => child,
                "resource" => {
                    put(child.child_by_field_name("name"), child.child_by_field_name("type"));
                    continue;
                }
                _ => continue,
            };
            let mut dc = decl.walk();
            for d in decl.named_children(&mut dc).filter(|d| d.kind() == "variable_declarator" && d.start_byte() < start) {
                put(d.child_by_field_name("name"), decl.child_by_field_name("type"));
            }
        }
        cur = a.parent();
    }
    out
}

/// Every use of a declared local inside `arg`, as `(from, to, name, slot type)`.
fn local_uses(arg: tree_sitter::Node, source: &str, locals: &HashMap<String, String>) -> Vec<(usize, usize, String, String)> {
    let mut out = Vec::new();
    if locals.is_empty() {
        return out;
    }
    let mut stack = vec![arg];
    while let Some(node) = stack.pop() {
        if node.kind() == "identifier" {
            if let Some(ty) = node.utf8_text(source.as_bytes()).ok().and_then(|n| locals.get(n).map(|t| (n, t))) {
                if is_value_use(node) {
                    out.push((node.start_byte(), node.end_byte(), ty.0.to_string(), ty.1.clone()));
                }
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    out
}

/// Whether an identifier reads a variable, as opposed to naming a method, a field, a method reference's target or
/// something declared inside the argument.
fn is_value_use(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else { return true };
    let is = |field: &str| parent.child_by_field_name(field).is_some_and(|f| f.id() == node.id());
    match parent.kind() {
        "method_invocation" => !is("name"),
        "field_access" => !is("field"),
        "variable_declarator" | "formal_parameter" | "inferred_parameters" | "lambda_expression" | "catch_formal_parameter" => false,
        "method_reference" => node.prev_sibling().is_none_or(|p| p.kind() != "::"),
        _ => true,
    }
}

/// A plain lowercase variable passed as an argument: a name, not a habit of form.
fn variable_name(arg: tree_sitter::Node, source: &str) -> Option<String> {
    if arg.kind() != "identifier" {
        return None;
    }
    let text = arg.utf8_text(source.as_bytes()).ok()?.trim();
    (text.len() <= MAX_LEN && text.starts_with(|c: char| c.is_ascii_lowercase())).then(|| text.to_string())
}

/// Every habit-worthy argument of every call in `source`.
fn arguments(source: &str) -> Vec<(Place, String)> {
    scan(source).0
}

/// The habit-worthy arguments and the plain variable names passed, with their places.
fn scan(source: &str) -> (Vec<(Place, String)>, Vec<(Place, String)>) {
    let Some(tree) = bennu_java::prelude::parse_java(source) else { return (Vec::new(), Vec::new()) };
    let mut out = Vec::new();
    let mut named_out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if matches!(node.kind(), "method_invocation" | "object_creation_expression") {
            let name = match node.kind() {
                "method_invocation" => node.child_by_field_name("name").and_then(|n| n.utf8_text(source.as_bytes()).ok()).map(str::to_string),
                _ => node
                    .child_by_field_name("type")
                    .and_then(|t| t.utf8_text(source.as_bytes()).ok())
                    .map(|t| format!("new {}", t.split('<').next().unwrap_or(t).trim())),
            };
            if let (Some(name), Some(args)) = (name, node.child_by_field_name("arguments")) {
                let mut cursor = args.walk();
                let named: Vec<_> = args.named_children(&mut cursor).filter(|c| !matches!(c.kind(), "line_comment" | "block_comment")).collect();
                for (idx, arg) in named.into_iter().enumerate() {
                    if let Some(text) = habit_text(arg, source) {
                        out.push(((name.clone(), idx), text));
                    } else if let Some(var) = variable_name(arg, source) {
                        named_out.push(((name.clone(), idx), var));
                    }
                }
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    (out, named_out)
}

/// Counts `found` into `table`, within `cap` distinct entries; what was kept.
fn count_into(table: &mut HashMap<Place, HashMap<String, u32>>, entries: &mut usize, cap: usize, found: Vec<(Place, String)>) -> Vec<(Place, String)> {
    let mut kept = Vec::with_capacity(found.len());
    for (place, text) in found {
        let known = table.get(&place).is_some_and(|m| m.contains_key(&text));
        if !known && *entries >= cap {
            continue;
        }
        let count = table.entry(place.clone()).or_default().entry(text.clone()).or_insert(0);
        if *count == 0 {
            *entries += 1;
        }
        *count += 1;
        kept.push((place, text));
    }
    kept
}

/// Takes back `taken` from `table`.
fn uncount(table: &mut HashMap<Place, HashMap<String, u32>>, entries: &mut usize, taken: Vec<(Place, String)>) {
    for (place, text) in taken {
        let Some(texts) = table.get_mut(&place) else { continue };
        if let Some(n) = texts.get_mut(&text) {
            *n -= 1;
            if *n == 0 {
                texts.remove(&text);
                *entries -= 1;
            }
        }
        if texts.is_empty() {
            table.remove(&place);
        }
    }
}

/// `table` plus `prior`, for a place this project has seen too little: the other projects' counts are added.
fn with_prior(mut seen: Vec<(String, u32)>, prior: Option<&HashMap<String, u32>>, keep: &dyn Fn(&str) -> bool) -> Vec<(String, u32)> {
    let local: u32 = seen.iter().map(|(_, n)| n).sum();
    if local >= MIN_SIGHTINGS {
        return seen;
    }
    for (t, n) in prior.into_iter().flatten().filter(|(t, _)| keep(t)) {
        match seen.iter_mut().find(|(s, _)| s == t) {
            Some(slot) => slot.1 += n,
            None => seen.push((t.clone(), *n)),
        }
    }
    seen.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    seen
}

fn group(flat: &[(String, usize, String, u32)]) -> HashMap<Place, HashMap<String, u32>> {
    let mut out: HashMap<Place, HashMap<String, u32>> = HashMap::new();
    for (callee, idx, text, n) in flat {
        *out.entry((callee.clone(), *idx)).or_default().entry(text.clone()).or_insert(0) += n;
    }
    out
}

impl Habits {
    pub(crate) fn add_file(&mut self, key: &str, source: &str) {
        let hash = crate::flc_profile::text_hash(source);
        if self.hashes.get(key) == Some(&hash) {
            return;
        }
        self.remove_file(key);
        let (found, named) = scan(source);
        let kept = count_into(&mut self.table, &mut self.entries, MAX_ENTRIES, found);
        let named = count_into(&mut self.names, &mut self.name_entries, MAX_ENTRIES, named);
        self.by_file.insert(key.to_string(), kept);
        self.name_by_file.insert(key.to_string(), named);
        self.hashes.insert(key.to_string(), hash);
        crate::flc_profile::mark_dirty();
    }

    pub(crate) fn remove_file(&mut self, key: &str) {
        let had = self.hashes.remove(key).is_some();
        uncount(&mut self.table, &mut self.entries, self.by_file.remove(key).unwrap_or_default());
        uncount(&mut self.names, &mut self.name_entries, self.name_by_file.remove(key).unwrap_or_default());
        if had {
            crate::flc_profile::mark_dirty();
        }
    }

    /// Every file counted, with the hash of its text and what it contributed — for the profile on disk.
    pub(crate) fn snapshot(&self, include: &dyn Fn(&str) -> bool) -> Vec<(String, u64, HabitPart)> {
        let flat = |v: Option<&Vec<(Place, String)>>| -> Vec<(String, usize, String)> {
            v.into_iter().flatten().map(|((c, i), t)| (c.clone(), *i, t.clone())).collect()
        };
        self.hashes
            .iter()
            .filter(|(k, _)| include(k))
            .map(|(k, h)| (k.clone(), *h, HabitPart { texts: flat(self.by_file.get(k)), names: flat(self.name_by_file.get(k)) }))
            .collect()
    }

    /// Puts back what a saved profile says `key` contributed, without reading the file.
    pub(crate) fn restore_file(&mut self, key: &str, hash: u64, part: HabitPart) {
        self.remove_file(key);
        let place = |v: Vec<(String, usize, String)>| -> Vec<(Place, String)> { v.into_iter().map(|(c, i, t)| ((c, i), t)).collect() };
        let kept = count_into(&mut self.table, &mut self.entries, MAX_ENTRIES, place(part.texts));
        let named = count_into(&mut self.names, &mut self.name_entries, MAX_ENTRIES, place(part.names));
        self.by_file.insert(key.to_string(), kept);
        self.name_by_file.insert(key.to_string(), named);
        self.hashes.insert(key.to_string(), hash);
    }

    /// The counters of the files `include` accepts, summed.
    pub(crate) fn totals(&self, include: &dyn Fn(&str) -> bool) -> HabitTotals {
        let mut texts: HashMap<(String, usize, String), u32> = HashMap::new();
        let mut names: HashMap<(String, usize, String), u32> = HashMap::new();
        for (key, _) in self.hashes.iter().filter(|(k, _)| include(k)) {
            for ((c, i), t) in self.by_file.get(key).into_iter().flatten() {
                *texts.entry((c.clone(), *i, t.clone())).or_insert(0) += 1;
            }
            for ((c, i), t) in self.name_by_file.get(key).into_iter().flatten() {
                *names.entry((c.clone(), *i, t.clone())).or_insert(0) += 1;
            }
        }
        let flat = |m: HashMap<(String, usize, String), u32>| {
            let mut v: Vec<_> = m.into_iter().map(|((c, i, t), n)| (c, i, t, n)).collect();
            v.sort();
            v
        };
        HabitTotals { texts: flat(texts), names: flat(names) }
    }

    pub(crate) fn set_prior(&mut self, totals: &HabitTotals) {
        self.prior_texts = group(&totals.texts);
        self.prior_names = group(&totals.names);
    }

    pub(crate) fn clear(&mut self) {
        *self = Habits::default();
    }

    /// The keys of the files counted.
    pub(crate) fn files(&self) -> Vec<String> {
        self.hashes.keys().cloned().collect()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// How many times `file` itself wrote `text` at `place`: what it must not count in its own favour.
    fn own(&self, file: &str, place: &Place, text: &str) -> u32 {
        self.by_file.get(file).map_or(0, |v| v.iter().filter(|(p, t)| p == place && t == text).count() as u32)
    }

    /// The arguments seen at `place` that `keep` accepts, most used first, with the other files' counts.
    fn seen(&self, file: &str, place: &Place, keep: impl Fn(&str) -> bool) -> Vec<(String, u32)> {
        let mut out: Vec<(String, u32)> = self
            .table
            .get(place)
            .into_iter()
            .flatten()
            .filter(|(t, _)| keep(t) && !crate::learned_rules::hidden("habit", &place_key(place), t))
            .map(|(t, n)| (t.clone(), n.saturating_sub(self.own(file, place, t))))
            .filter(|(_, n)| *n > 0)
            .collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let key = place_key(place);
        with_prior(out, self.prior_texts.get(place), &|t| keep(t) && !crate::learned_rules::hidden("habit", &key, t))
    }

    /// The variable name usually passed at `place`: it needs enough sightings in other files and a clear majority.
    pub(crate) fn habit_name(&self, file: &str, callee: &str, index: usize) -> Option<(String, f32, u32)> {
        let place = (callee.to_string(), index);
        let own = |t: &str| self.name_by_file.get(file).map_or(0, |v| v.iter().filter(|(p, n)| *p == place && n == t).count() as u32);
        let mut seen: Vec<(String, u32)> = self
            .names
            .get(&place)
            .into_iter()
            .flatten()
            .filter(|(t, _)| !crate::learned_rules::hidden("name", &place_key(&place), t))
            .map(|(t, n)| (t.clone(), n.saturating_sub(own(t))))
            .filter(|(_, n)| *n > 0)
            .collect();
        seen.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let seen = with_prior(seen, self.prior_names.get(&place), &|_| true);
        let total: u32 = seen.iter().map(|(_, n)| n).sum();
        let (name, count) = seen.into_iter().next()?;
        let share = count as f32 / total as f32;
        (count >= MIN_SIGHTINGS && share >= MIN_SHARE).then_some((name, share, count))
    }

    /// The usual argument at `place` among those that start with `typed`: needs enough sightings and a clear majority.
    fn usual(&self, file: &str, place: &Place, typed: &str, fit: &dyn Fn(&str) -> bool) -> Option<(String, f32, u32)> {
        let seen = self.seen(file, place, |t| can_start_with(t, typed) && (has_slots(t) || t.len() > typed.len()) && fit(t));
        let total: u32 = seen.iter().map(|(_, n)| n).sum();
        let (text, count) = seen.first().cloned()?;
        let share = count as f32 / total as f32;
        if count >= MIN_SIGHTINGS && share >= MIN_SHARE {
            return Some((text, share, count));
        }
        // no single text leads, but one first word does (`Boolean.FALSE::equals` and `Boolean.TRUE::equals`): what
        // those texts share is drawn, and the popup takes it from there
        let (prefix, share, count) = leading_group(&seen, total)?;
        (prefix.len() > typed.len()).then_some((prefix, share, count))
    }

    /// The usual argument at the caret, still as a template: what the lock is held for. [`finish`] fills its slots.
    fn lead(&self, file: &str, source: &str, offset: usize, fit: &dyn Fn(&str) -> bool) -> Option<Lead> {
        let (callee, idx, start) = bennu_query::prelude::call_site_name_and_index(source, offset)?;
        let typed = source.get(start..offset)?.trim_start();
        if typed.contains('\n') {
            return None;
        }
        let (template, share, _) = self.usual(file, &(callee, idx), typed, fit)?;
        Some(Lead { typed: typed.to_string(), template, share })
    }

    /// The argument to draw after the caret, when the project has a usual one. `scope` is asked only when the usual
    /// argument has variables in it: the variables in scope at the caret.
    pub(crate) fn suggest_with(&self, file: &str, source: &str, offset: usize, scope: &dyn Fn() -> Option<Vec<Candidate>>) -> Option<Habit> {
        finish(self.lead(file, source, offset, &|_| true)?, scope)
    }

    /// [`Self::suggest_with`] among the texts `fit` accepts (see `flc_generic_slot`).
    #[cfg(test)]
    pub(crate) fn suggest_fit(&self, file: &str, source: &str, offset: usize, fit: &dyn Fn(&str) -> bool) -> Option<Habit> {
        finish(self.lead(file, source, offset, fit)?, &|| None)
    }

    /// [`suggest_with`] for a caret with no variables to offer.
    #[cfg(test)]
    pub(crate) fn suggest(&self, file: &str, source: &str, offset: usize) -> Option<Habit> {
        self.suggest_with(file, source, offset, &|| None)
    }

    /// The usual arguments to put in the completion list, still as templates.
    fn rows_plan(&self, file: &str, source: &str, offset: usize, case: MatchCase, fit: &dyn Fn(&str) -> bool) -> Option<RowsPlan> {
        if self.is_empty() {
            return None;
        }
        let (callee, idx, start) = bennu_query::prelude::call_site_name_and_index(source, offset)?;
        let typed = source.get(start..offset).map(str::trim_start)?;
        // only a word being typed (or nothing yet): after `Boolean.` the member list is the provider's
        if !typed.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$') {
            return None;
        }
        let place = (callee.clone(), idx);
        let matcher = Typed::new(typed, case);
        let seen = self.seen(file, &place, |t| (typed.is_empty() || matcher.matches(first_word(t))) && fit(t));
        let total: u32 = seen.iter().map(|(_, n)| n).sum();
        let (_, top) = seen.first()?;
        let share = *top as f32 / total as f32;
        if (*top < MIN_SIGHTINGS || share < MIN_SHARE) && leading_group(&seen, total).is_none() {
            // no single argument leads, but a few are the usual ones (`Foo.class`, `Bar.class`, `Baz.class` at the same place): they
            // head the list, none of them preselected
            let usual: Vec<(String, u32)> = seen.iter().filter(|(_, n)| *n >= MIN_SIGHTINGS).take(MAX_ROWS).cloned().collect();
            let covered: u32 = usual.iter().map(|(_, n)| n).sum();
            if usual.len() >= 2 && covered as f32 / total as f32 >= MIN_SHARE {
                return Some(RowsPlan { callee, share, seen: usual });
            }
            return None;
        }
        Some(RowsPlan { callee, share, seen: seen.into_iter().take(MAX_ROWS).collect() })
    }

    /// Rows for the completion list: the usual arguments whose first word the typed start matches, most used first.
    #[cfg(test)]
    pub(crate) fn rows(&self, file: &str, source: &str, offset: usize, case: MatchCase, items: &mut Vec<CompletionItem>) {
        self.rows_fit(file, source, offset, case, items, &|_| true, false);
    }

    /// [`Self::rows`] among the texts `fit` accepts; `demoted` puts them after the provider's rows instead of ahead.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn rows_fit(&self, file: &str, source: &str, offset: usize, case: MatchCase, items: &mut Vec<CompletionItem>, fit: &dyn Fn(&str) -> bool, demoted: bool) {
        if let Some(plan) = self.rows_plan(file, source, offset, case, fit) {
            plan.put_in(&|| None, demoted, items);
        }
    }
}

/// What [`Habits::lead`] found: the template and how it stands, with the start already typed.
struct Lead {
    typed: String,
    template: String,
    share: f32,
}

/// Fills the slots of a lead from the variables in scope; the text left to write after what is typed.
fn finish(lead: Lead, scope: &dyn Fn() -> Option<Vec<Candidate>>) -> Option<Habit> {
    let text = if has_slots(&lead.template) { instantiate(&lead.template, &scope()?)? } else { lead.template };
    let rest = text.strip_prefix(lead.typed.as_str()).filter(|r| !r.is_empty())?;
    Some(Habit { text: rest.to_string(), confidence: lead.share })
}

/// The rows to put in the popup, before their slots are filled.
struct RowsPlan {
    callee: String,
    share: f32,
    seen: Vec<(String, u32)>,
}

impl RowsPlan {
    fn put_in(self, scope: &dyn Fn() -> Option<Vec<Candidate>>, demoted: bool, items: &mut Vec<CompletionItem>) {
        let variables = self.seen.iter().any(|(t, _)| has_slots(t)).then(scope).flatten();
        let mut rows = Vec::new();
        for (text, n) in &self.seen {
            // a row with a variable in it is drawn with the one in scope; without one in scope it is not drawn
            let text = if has_slots(text) {
                match variables.as_deref().and_then(|v| instantiate(text, v)) {
                    Some(t) => t,
                    None => continue,
                }
            } else {
                text.clone()
            };
            let word = first_word(&text);
            let label = if text.len() <= MAX_LABEL { text.clone() } else { word.to_string() };
            // a class the file has not imported: the provider already knows its import, so it is borrowed, not invented
            let known = items.iter().find(|r| r.label == word);
            let capitalised = word.starts_with(|c: char| c.is_ascii_uppercase());
            rows.push(CompletionItem {
                label: label.clone(),
                kind: if capitalised { "class" } else { "method" }.to_string(),
                detail: Some(format!("used {n}× in .{}(…)", self.callee.trim_start_matches("new "))),
                auto_import: known.and_then(|r| r.auto_import.clone()),
                preselect: rows.is_empty() && self.share >= MIN_SHARE && !demoted,
                ..Default::default()
            });
        }
        if rows.is_empty() {
            return;
        }
        let labels: Vec<String> = rows.iter().map(|r| r.label.clone()).collect();
        items.retain(|r| !labels.contains(&r.label));
        if rows.first().is_some_and(|r| r.preselect) {
            // two preselected rows tie for first: the habit is the one the list is led by
            for r in items.iter_mut() {
                r.preselect = false;
            }
        }
        if demoted {
            items.append(&mut rows);
        } else {
            rows.append(items);
            *items = rows;
        }
    }
}

fn habits() -> &'static Mutex<Habits> {
    static HABITS: OnceLock<Mutex<Habits>> = OnceLock::new();
    HABITS.get_or_init(|| Mutex::new(Habits::default()))
}

fn lock() -> std::sync::MutexGuard<'static, Habits> {
    habits().lock().unwrap_or_else(|p| p.into_inner())
}

pub(crate) fn add_file(key: &str, source: &str) {
    lock().add_file(key, source);
}

pub(crate) fn remove_file(key: &str) {
    lock().remove_file(key);
}

/// The variable name the project usually passes at argument `index` of `callee`, as `(name, share, sightings)`.
#[allow(dead_code)] // the file-aware `habit_name_in` is what the argument list calls
pub(crate) fn habit_name(callee: &str, index: usize) -> Option<(String, f32, u32)> {
    habit_name_in("", callee, index)
}

/// [`habit_name`] without counting what `file` itself passed: a file does not vouch for itself.
pub(crate) fn habit_name_in(file: &str, callee: &str, index: usize) -> Option<(String, f32, u32)> {
    lock().habit_name(file, callee, index)
}

pub(crate) fn snapshot(include: &dyn Fn(&str) -> bool) -> Vec<(String, u64, HabitPart)> {
    lock().snapshot(include)
}

pub(crate) fn restore_file(key: &str, hash: u64, part: HabitPart) {
    lock().restore_file(key, hash, part);
}

pub(crate) fn totals(include: &dyn Fn(&str) -> bool) -> HabitTotals {
    lock().totals(include)
}

pub(crate) fn set_prior(totals: &HabitTotals) {
    lock().set_prior(totals);
}

pub(crate) fn clear() {
    lock().clear();
}

pub(crate) fn sizes() -> crate::flc_memory::Sizes {
    use crate::flc_memory::{text, two_level, Sizes};
    let h = lock();
    let place = |p: &Place| text(&p.0) + 8;
    let mut s = Sizes::default();
    for (k, v) in h.by_file.iter().chain(h.name_by_file.iter()) {
        s.add_file(k, v.iter().map(|(p, t)| place(p) + text(t)).sum());
    }
    s.shared = two_level(&h.table, place) + two_level(&h.names, place);
    s.shared_entries = h.entries + h.name_entries;
    s.carried = two_level(&h.prior_texts, place) + two_level(&h.prior_names, place);
    s
}

pub(crate) fn files() -> Vec<String> {
    lock().files()
}

/// The usual argument for the call the caret is in, as text to draw after the caret.
pub(crate) fn suggest(file: &str, source: &str, offset: usize) -> Option<Habit> {
    // the lock is let go before the variables are looked up: that asks the index
    let slot = crate::flc_generic_slot::at(file, source, offset);
    let lead = lock().lead(file, source, offset, &|t| slot.as_ref().map_or(true, |s| s.admits(t)))?;
    let habit = finish(lead, &|| crate::flc_member_args::scope_candidates(file, source, offset))?;
    Some(Habit { confidence: slot.as_ref().map_or(habit.confidence, |s| s.confidence(habit.confidence)), ..habit })
}

/// Puts the usual arguments at the top of `items` when the caret is in a call argument.
pub(crate) fn popup_rows(file: &str, source: &str, offset: usize, case: MatchCase, items: &mut Vec<CompletionItem>) {
    let slot = crate::flc_generic_slot::at(file, source, offset);
    let plan = lock().rows_plan(file, source, offset, case, &|t| slot.as_ref().map_or(true, |s| s.admits(t)));
    if let Some(plan) = plan {
        let demoted = slot.as_ref().is_some_and(|s| s.is_free());
        plan.put_in(&|| crate::flc_member_args::scope_candidates(file, source, offset), demoted, items);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(body: &str) -> String {
        format!("class A {{\n    void f() {{\n{body}\n    }}\n}}\n")
    }

    /// Four files that all filter, map and unwrap the way the project always does.
    fn project() -> Habits {
        let mut h = Habits::default();
        for i in 0..4 {
            h.add_file(
                &format!("S{i}.java"),
                &file(
                    "        repo.find(id).map(it -> it.name()).filter(Boolean.FALSE::equals).orElseThrow(NotFoundException::new);\n        other.filter(Objects::nonNull);",
                ),
            );
        }
        h
    }

    #[test]
    fn short_non_literal_arguments_are_counted_and_the_rest_is_not() {
        let found = arguments(&file("        a.m(\"x\", 1, local, this, Boolean.FALSE::equals, it -> it.id(), Foo.BAR, new Bar<>(), local.get(), 1 + 2);"));
        let texts: Vec<&str> = found.iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(texts, ["Boolean.FALSE::equals", "it -> it.id()", "Foo.BAR", "new Bar<>()"]);
        assert_eq!(found[0].0, ("m".to_string(), 4));
        // too long, or on several lines: a piece of code, not a habit
        let long = format!("        a.m(Foo.{}());", "veryLongName".repeat(6));
        assert!(arguments(&file(&long)).is_empty());
    }

    #[test]
    fn the_usual_argument_is_proposed_after_the_open_parenthesis() {
        let h = project();
        let src = file("        list.stream().orElseThrow(");
        let at = src.find("orElseThrow(").unwrap() + "orElseThrow(".len();
        assert_eq!(h.suggest("New.java", &src, at).expect("a habit").text, "NotFoundException::new");
        // `filter(` has two usual arguments here, half and half: neither is the habit
        let src = file("        list.stream().filter(");
        assert!(h.suggest("New.java", &src, src.find("filter(").unwrap() + 7).is_none());
    }

    #[test]
    fn several_usual_arguments_at_one_place_all_head_the_list_without_a_preselection() {
        let mut h = Habits::default();
        for i in 0..9 {
            let class = ["Alpha", "Beta", "Gamma"][i % 3];
            h.add_file(&format!("C{i}.java"), &file(&format!("        reg.add(name, {class}.class);")));
        }
        let src = file("        reg.add(name, ");
        let at = src.find("add(name, ").unwrap() + "add(name, ".len();
        let mut items = Vec::new();
        h.rows("New.java", &src, at, MatchCase::default(), &mut items);
        let labels: Vec<&str> = items.iter().map(|r| r.label.as_str()).collect();
        for expected in ["Alpha.class", "Beta.class", "Gamma.class"] {
            assert!(labels.contains(&expected), "{labels:?}");
        }
        assert!(items.iter().all(|r| !r.preselect), "none of three equal choices is the answer");
    }

    #[test]
    fn a_typed_start_is_completed_from_the_matching_arguments() {
        let h = project();
        let src = file("        list.stream().filter(Boo");
        let at = src.find("filter(Boo").unwrap() + "filter(Boo".len();
        let got = h.suggest("New.java", &src, at).expect("a habit");
        assert_eq!(got.text, "lean.FALSE::equals");
        // a start no habit has gives nothing
        let src = file("        list.stream().filter(Zzz");
        assert!(h.suggest("New.java", &src, src.find("Zzz").unwrap() + 3).is_none());
        // the lambda is proposed as written
        let src = file("        x.map(");
        let at = src.find("map(").unwrap() + 4;
        assert_eq!(h.suggest("New.java", &src, at).unwrap().text, "it -> it.name()");
    }

    #[test]
    fn two_texts_with_one_leading_word_are_drawn_up_to_where_they_part() {
        let mut h = Habits::default();
        for i in 0..2 {
            h.add_file(&format!("T{i}.java"), &file("        x.filter(Boolean.TRUE::equals);"));
            h.add_file(&format!("F{i}.java"), &file("        x.filter(Boolean.FALSE::equals);"));
        }
        let src = file("        list.stream().filter(");
        let at = src.find("filter(").unwrap() + 7;
        assert_eq!(h.suggest("N.java", &src, at).expect("the shared start").text, "Boolean.");
        // a start the group shares is not drawn again, and a start past it leaves the group
        let src = file("        list.stream().filter(Boolean.");
        assert!(h.suggest("N.java", &src, src.find("Boolean.").unwrap() + 8).is_none());
    }

    #[test]
    fn lambdas_with_the_same_parameter_and_different_bodies_are_drawn_up_to_the_arrow() {
        let mut h = Habits::default();
        for i in 0..2 {
            h.add_file(&format!("A{i}.java"), &file("        x.filter(it -> it.ok());"));
            h.add_file(&format!("B{i}.java"), &file("        x.filter(it -> !it.gone());"));
        }
        let src = file("        list.stream().filter(");
        let at = src.find("filter(").unwrap() + 7;
        assert_eq!(h.suggest("N.java", &src, at).expect("the head").text, "it -> ");
    }

    #[test]
    fn constructors_that_differ_only_inside_are_drawn_up_to_the_parenthesis() {
        let mut h = Habits::default();
        for (i, var) in ["bao", "out", "baos", "buffer"].iter().enumerate() {
            h.add_file(&format!("U{i}.java"), &file(&format!("        store.upload(new ByteArrayInputStream({var}.toByteArray()), doc);")));
        }
        let src = file("        store.upload(");
        let at = src.find("upload(").unwrap() + 7;
        assert_eq!(h.suggest("N.java", &src, at).expect("the shared constructor").text, "new ByteArrayInputStream(");
    }

    #[test]
    fn a_second_argument_is_its_own_place() {
        let mut h = Habits::default();
        for i in 0..3 {
            h.add_file(&format!("F{i}.java"), &file("        x.collect(a, Collectors.toList());"));
        }
        let src = file("        x.collect(a, ");
        let at = src.find("a, ").unwrap() + 3;
        assert_eq!(h.suggest("N.java", &src, at).unwrap().text, "Collectors.toList()");
        let src = file("        x.collect(");
        assert!(h.suggest("N.java", &src, src.find("collect(").unwrap() + 8).is_none(), "the first place has no habit");
    }

    #[test]
    fn too_few_sightings_or_no_majority_give_nothing() {
        let mut h = Habits::default();
        h.add_file("A.java", &file("        x.m(Foo.A);"));
        h.add_file("B.java", &file("        x.m(Foo.A);"));
        let src = file("        x.m(");
        let at = src.find("m(").unwrap() + 2;
        assert!(h.suggest("N.java", &src, at).is_none(), "two sightings are not a habit");
        h.add_file("C.java", &file("        x.m(Foo.A);"));
        assert!(h.suggest("N.java", &src, at).is_some());
        for i in 0..3 {
            h.add_file(&format!("D{i}.java"), &file("        x.m(Bar.B);"));
        }
        assert!(h.suggest("N.java", &src, at).is_none(), "3 against 3 is no majority");
    }

    #[test]
    fn a_file_does_not_vouch_for_itself_and_a_resaved_file_replaces_its_counts() {
        let mut h = Habits::default();
        h.add_file("A.java", &file("        x.m(Foo.A);\n        x.m(Foo.A);\n        x.m(Foo.A);"));
        let src = file("        x.m(");
        let at = src.find("m(").unwrap() + 2;
        assert!(h.suggest("A.java", &src, at).is_none(), "its own three do not count for it");
        assert!(h.suggest("B.java", &src, at).is_some());
        h.add_file("A.java", &file("        x.m(Foo.Z);"));
        assert!(h.suggest("B.java", &src, at).is_none());
        h.remove_file("A.java");
        assert!(h.is_empty());
    }

    #[test]
    fn the_variable_usually_passed_at_a_place_is_a_habit_of_names() {
        let mut h = Habits::default();
        for i in 0..4 {
            h.add_file(&format!("P{i}.java"), &file("        bank.pay(payer, payee);\n        bank.audit(payer);"));
        }
        h.add_file("Q.java", &file("        bank.pay(other, payee);"));
        // `payer` is passed first in 4 of 5 calls, `payee` second in all of them; other files vote, this one does not
        let (name, share, n) = h.habit_name("New.java", "pay", 0).expect("a habit");
        assert_eq!((name.as_str(), n), ("payer", 4));
        assert!((share - 0.8).abs() < 1e-6);
        assert_eq!(h.habit_name("New.java", "pay", 1).unwrap().0, "payee");
        // a file's own calls do not count in its favour
        assert!(h.habit_name("P0.java", "pay", 0).is_some_and(|(_, _, n)| n == 3));
        // too few sightings, or an even split, give nothing
        assert!(h.habit_name("New.java", "audit", 1).is_none());
        let mut split = Habits::default();
        for i in 0..3 {
            split.add_file(&format!("A{i}.java"), &file("        bank.pay(a, b);"));
            split.add_file(&format!("B{i}.java"), &file("        bank.pay(c, b);"));
        }
        assert!(split.habit_name("N.java", "pay", 0).is_none(), "3 against 3");
        // the names never leak into the table of forms
        assert!(h.table.is_empty());
        // a resaved file replaces its names; removing them all empties the table
        for i in 0..4 {
            h.remove_file(&format!("P{i}.java"));
        }
        h.remove_file("Q.java");
        assert!(h.names.is_empty() && h.name_entries == 0);
    }

    #[test]
    fn a_snapshot_restores_the_same_counts_without_reading_the_file() {
        let mut h = project();
        h.add_file("N.java", &file("        bank.pay(payer);"));
        let snap = h.snapshot(&|_| true);
        let mut back = Habits::default();
        for (key, hash, part) in snap {
            back.restore_file(&key, hash, part);
        }
        assert_eq!(back.totals(&|_| true), h.totals(&|_| true));
        // the same text again is not read again: the counts stay
        let before = back.totals(&|_| true);
        back.add_file("N.java", &file("        bank.pay(payer);"));
        assert_eq!(back.totals(&|_| true), before);
    }

    #[test]
    fn another_projects_counts_are_added_only_where_this_one_has_too_little() {
        let mut other = Habits::default();
        for i in 0..4 {
            other.add_file(&format!("O{i}.java"), &file("        x.orElseThrow(NotFoundException::new);\n        y.send(sender);"));
        }
        let mut mine = Habits::default();
        mine.set_prior(&other.totals(&|_| true));
        let src = file("        x.orElseThrow(");
        let at = src.find("orElseThrow(").unwrap() + "orElseThrow(".len();
        assert_eq!(mine.suggest("N.java", &src, at).expect("from the prior").text, "NotFoundException::new");
        assert_eq!(mine.habit_name("N.java", "send", 0).unwrap().0, "sender");
        // with three sightings of its own, this project no longer asks
        for i in 0..3 {
            mine.add_file(&format!("M{i}.java"), &file("        x.orElseThrow(Foo::new);"));
        }
        assert_eq!(mine.suggest("N.java", &src, at).unwrap().text, "Foo::new");
    }

    #[test]
    fn the_popup_opens_on_the_habit_that_starts_like_the_typed_word() {
        let h = project();
        let src = file("        list.stream().filter(Boo");
        let at = src.find("filter(Boo").unwrap() + "filter(Boo".len();
        let mut items = vec![
            CompletionItem { label: "Boolean".into(), kind: "class".into(), auto_import: None, preselect: true, ..Default::default() },
            CompletionItem { label: "Book".into(), kind: "class".into(), auto_import: Some("shop.Book".into()), ..Default::default() },
        ];
        h.rows("New.java", &src, at, MatchCase::default(), &mut items);
        assert_eq!(items[0].label, "Boolean.FALSE::equals", "{items:?}");
        assert!(items[0].preselect && items.iter().filter(|i| i.preselect).count() == 1);
        assert!(items[0].detail.as_deref().is_some_and(|d| d.contains("4×") && d.contains(".filter(")));
        // nothing else moved
        assert_eq!(items.iter().skip(1).map(|i| i.label.as_str()).collect::<Vec<_>>(), ["Boolean", "Book"]);
    }

    #[test]
    fn a_class_the_file_has_not_imported_borrows_the_providers_import() {
        let mut h = Habits::default();
        for i in 0..3 {
            h.add_file(&format!("F{i}.java"), &file("        x.orElseThrow(NotFoundException::new);"));
        }
        let src = file("        x.orElseThrow(Not");
        let at = src.find("orElseThrow(Not").unwrap() + "orElseThrow(Not".len();
        let mut items = vec![CompletionItem { label: "NotFoundException".into(), kind: "class".into(), auto_import: Some("shop.NotFoundException".into()), ..Default::default() }];
        h.rows("N.java", &src, at, MatchCase::default(), &mut items);
        assert_eq!(items[0].label, "NotFoundException::new");
        assert_eq!(items[0].auto_import.as_deref(), Some("shop.NotFoundException"));
    }

    #[test]
    fn after_a_dot_the_popup_is_left_alone() {
        let h = project();
        let src = file("        list.filter(Boolean.F");
        let at = src.find("Boolean.F").unwrap() + 9;
        let mut items = vec![CompletionItem { label: "FALSE".into(), ..Default::default() }];
        h.rows("N.java", &src, at, MatchCase::default(), &mut items);
        assert_eq!(items.len(), 1);
    }

    /// A method whose locals are named `names`, each declared with `ty`, passed to `new Factory().create(..)` and `new Stream(..)`.
    fn site(ty: &str, var: &str) -> String {
        file(&format!("        {ty} {var} = make();\n        sink.put(new Factory().create({var}), new Stream({var}.toByteArray()));"))
    }

    fn scope(vars: &[(&str, usize, &[&str])]) -> Vec<Candidate> {
        vars.iter().map(|(n, d, t)| Candidate { name: n.to_string(), decl: *d, types: t.iter().map(|t| t.to_string()).collect() }).collect()
    }

    #[test]
    fn a_local_variable_in_an_argument_becomes_a_typed_slot() {
        let found = arguments(&site("Order", "o1"));
        let texts: Vec<&str> = found.iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(texts, ["new Factory().create($0:Order)", "new Stream($0:Order.toByteArray())"]);
        // the same variable twice is one slot; two variables are two; a field, a method name and a lambda parameter stay text
        let src = file("        Order a = x();\n        Order b = y();\n        m(Pair.of(a, b, a), Pair.of(a.size(), this.total, it -> it.a()), Pair.of(b::size));");
        let texts: Vec<String> = arguments(&src).into_iter().map(|(_, t)| t).collect();
        for want in ["Pair.of($0:Order, $1:Order, $0:Order)", "Pair.of($0:Order.size(), this.total, it -> it.a())", "Pair.of($0:Order::size)"] {
            assert!(texts.iter().any(|t| t == want), "{want}: {texts:?}");
        }
        // an undeclared name (a field) and a `var` of unknown type
        let src = file("        var v = x();\n        m(Pair.of(v, field));");
        assert_eq!(arguments(&src)[0].1, "Pair.of($0:?, field)");
        // declared in a parameter list, a for-each and a catch, with generics and packages dropped
        let src = "class A {\n void f(java.util.List<String> xs, int[] n) {\n  for (Item i : xs) {\n   try { g(); } catch (IOException e) {\n    m(Wrap.of(xs, n, i, e));\n   }\n  }\n }\n}\n";
        assert_eq!(arguments(src)[0].1, "Wrap.of($0:List, $1:int[], $2:Item, $3:IOException)");
    }

    #[test]
    fn sites_with_different_local_names_count_as_one_template() {
        let mut h = Habits::default();
        for (i, var) in ["v1", "v2", "payload", "o", "current"].iter().enumerate() {
            h.add_file(&format!("S{i}.java"), &site("Order", var));
        }
        let puts = &h.table[&("put".to_string(), 0)];
        assert_eq!(puts.len(), 1, "{puts:?}");
        assert_eq!(puts["new Factory().create($0:Order)"], 5);
        // the same for a call on the variable inside a constructor, and the report shows the template: one row
        let streams = &h.table[&("put".to_string(), 1)];
        assert_eq!((streams.len(), streams["new Stream($0:Order.toByteArray())"]), (1, 5));
    }

    #[test]
    fn the_template_is_filled_with_the_nearest_variable_of_the_type() {
        let mut h = Habits::default();
        for (i, var) in ["v1", "v2", "payload", "o"].iter().enumerate() {
            h.add_file(&format!("S{i}.java"), &site("Order", var));
        }
        let src = file("        sink.put(");
        let at = src.find("put(").unwrap() + 4;
        let vars = scope(&[("early", 10, &["Order", "Object"]), ("name", 50, &["String", "Object"]), ("late", 90, &["Order", "Object"]), ("_unused", 95, &["Order"])]);
        let got = h.suggest_with("N.java", &src, at, &|| Some(vars.clone())).expect("a habit");
        assert_eq!(got.text, "new Factory().create(late)");
        // a typed start is completed through the slot
        let src = file("        sink.put(new Factory().cr");
        let at = src.find(".cr").unwrap() + 3;
        assert_eq!(h.suggest_with("N.java", &src, at, &|| Some(vars.clone())).unwrap().text, "eate(late)");
        // no variable of the type, or no scope at all: nothing is drawn, no name is invented
        let wrong = scope(&[("name", 50, &["String"])]);
        let src = file("        sink.put(");
        let at = src.find("put(").unwrap() + 4;
        assert!(h.suggest_with("N.java", &src, at, &|| Some(wrong.clone())).is_none());
        assert!(h.suggest("N.java", &src, at).is_none());
        // the second use of a variable in the template is the same variable
        let mut t = Habits::default();
        for i in 0..4 {
            t.add_file(&format!("T{i}.java"), &file(&format!("        Order a{i} = x();\n        Order b{i} = y();\n        q.run(Pair.of(a{i}, b{i}, a{i}));")));
        }
        let src = file("        q.run(");
        let at = src.find("run(").unwrap() + 4;
        let two = scope(&[("first", 10, &["Order"]), ("second", 20, &["Order"])]);
        assert_eq!(t.suggest_with("N.java", &src, at, &|| Some(two.clone())).unwrap().text, "Pair.of(second, first, second)");
        // two slots need two variables
        let one = scope(&[("only", 10, &["Order"])]);
        assert!(t.suggest_with("N.java", &src, at, &|| Some(one.clone())).is_none());
    }

    #[test]
    fn the_popup_row_shows_the_variable_not_the_slot() {
        let mut h = Habits::default();
        for (i, var) in ["v1", "v2", "payload", "o"].iter().enumerate() {
            h.add_file(&format!("S{i}.java"), &site("Order", var));
        }
        let src = file("        sink.put(ne");
        let at = src.find("put(ne").unwrap() + 6;
        let vars = scope(&[("order", 10, &["Order"])]);
        let mut items = vec![CompletionItem { label: "new".into(), ..Default::default() }];
        h.rows_plan("N.java", &src, at, MatchCase::default(), &|_| true).expect("a plan").put_in(&|| Some(vars.clone()), false, &mut items);
        assert_eq!(items[0].label, "new Factory().create(order)");
        // without a fitting variable the row is not drawn
        let mut items = vec![CompletionItem { label: "new".into(), ..Default::default() }];
        h.rows_plan("N.java", &src, at, MatchCase::default(), &|_| true).unwrap().put_in(&|| Some(Vec::new()), false, &mut items);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn a_hidden_rule_works_on_the_template() {
        let mut h = Habits::default();
        for (i, var) in ["v1", "v2", "payload", "o"].iter().enumerate() {
            h.add_file(&format!("S{i}.java"), &file(&format!("        Order {var} = x();\n        hiddenzz.run(Kit.make({var}));")));
        }
        let src = file("        hiddenzz.run(");
        let at = src.find("run(").unwrap() + 4;
        let vars = scope(&[("order", 10, &["Order"])]);
        assert_eq!(h.suggest_with("N.java", &src, at, &|| Some(vars.clone())).unwrap().text, "Kit.make(order)");
        crate::learned_rules::hide(crate::learned_rules::Rule { kind: "habit".into(), key: "run#0".into(), text: "Kit.make($0:Order)".into() });
        let hidden = h.suggest_with("N.java", &src, at, &|| Some(vars.clone()));
        crate::learned_rules::restore(&crate::learned_rules::Rule { kind: "habit".into(), key: "run#0".into(), text: "Kit.make($0:Order)".into() });
        assert!(hidden.is_none());
    }

    #[test]
    fn templates_are_parsed_and_matched_by_their_literal_pieces() {
        assert!(can_start_with("new F().create($0:Order)", "new F().cr"));
        assert!(can_start_with("new F().create($0:Order)", "new F().create(ab"));
        assert!(!can_start_with("new F().create($0:Order)", "new G"));
        assert_eq!(parts("a($0:int[][], $12:?)"), [Part::Lit("a("), Part::Slot(0, "int[][]"), Part::Lit(", "), Part::Slot(12, "?"), Part::Lit(")")]);
        assert!(!has_slots("price$ + $x"));
    }
}
