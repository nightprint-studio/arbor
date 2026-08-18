//! Reading declarations and registrations out of a masked Rust source.
//!
//! A linear token walk rather than a parse. What is being looked for is a small, regular subset of
//! the grammar — an attribute run before an item, a parameter list, the arguments of one call — and
//! each of those is recognisable without knowing what the rest of the file means. The cost is that
//! a `fn` inside a function body looks exactly like one at module level, which is a difference this
//! crate has no use for anyway.
//!
//! What it produces is *raw*: names, offsets and the verbatim text of parameters. Turning a
//! parameter into an access needs the whole project (a `#[derive(SystemParam)]` may live in another
//! file), so that happens once, in [`crate::build`], after every file has been read.

use crate::model::Role;

/// A function, kept whether or not it turns out to be a system: which functions are systems is
/// decided in [`crate::build`], where the project's own `SystemParam` types are known.
#[derive(Debug, Clone)]
pub struct RawFn {
    pub name: String,
    /// Byte offset of the name.
    pub offset: usize,
    /// Verbatim parameters, `self` dropped.
    pub params: Vec<String>,
}

/// A type declaration and the traits its attributes claim for it.
#[derive(Debug, Clone)]
pub struct RawType {
    pub name: String,
    pub offset: usize,
    pub roles: Vec<Role>,
    /// Field types, **verbatim** — a struct's fields, a tuple struct's elements, nothing for an
    /// enum. Kept whole rather than reduced to a name because the two readers want different
    /// halves: a `Bundle` wants the component's name, a `SystemParam` wants the parameter it
    /// bundles (`Query<&mut Transform>`), and only the whole text can answer both.
    pub fields: Vec<String>,
}

/// One `add_systems(schedule, systems)` call.
#[derive(Debug, Clone)]
pub struct Registration {
    pub schedule: String,
    /// The systems named directly in the tuple — not the ones named inside `.after(…)`.
    pub systems: Vec<String>,
    /// Systems named in an ordering combinator (`.before` / `.after`), whichever direction: the
    /// pair is ordered either way, and which way round is a question this crate does not ask.
    pub ordered_with: Vec<String>,
    /// Sets the call put its systems in.
    pub sets: Vec<String>,
    /// The tuple was `.chain()`ed — every system in it is ordered against every other.
    pub chained: bool,
}

/// Everything one file said.
#[derive(Debug, Clone, Default)]
pub struct FileScan {
    pub types: Vec<RawType>,
    pub fns: Vec<RawFn>,
    pub registrations: Vec<Registration>,
    /// `impl Component for Health` — the hand-written half of what a derive usually says.
    pub trait_impls: Vec<(String, Role)>,
}

/// Keywords that may sit between an attribute and the item it belongs to.
const MODIFIERS: &[&str] = &["pub", "async", "unsafe", "const", "extern", "default", "static"];

/// Keywords that take the attributes with them: whatever follows is not a type or a function.
const CONSUMERS: &[&str] = &["mod", "use", "trait", "let", "type", "macro_rules", "match", "if"];

/// Walk `masked` — the output of [`crate::mask::mask`], never the raw source.
pub fn scan_file(masked: &str) -> FileScan {
    let b = masked.as_bytes();
    let mut out = FileScan::default();
    let mut attrs: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // An attribute run — `#[…]` and `#![…]` alike.
        if c == b'#' {
            let open = if b.get(i + 1) == Some(&b'!') { i + 2 } else { i + 1 };
            if b.get(open) == Some(&b'[') {
                let close = matching(b, open).unwrap_or(b.len().saturating_sub(1));
                attrs.push(masked[open + 1..close.min(masked.len())].to_string());
                i = close + 1;
                continue;
            }
        }
        if !is_ident_start(c) {
            i += 1;
            continue;
        }
        let (word, after) = read_ident(masked, i);
        match word.as_str() {
            "struct" | "enum" | "union" => {
                let is_struct = word != "enum";
                i = read_type_decl(masked, after, &attrs, is_struct, &mut out);
                attrs.clear();
            }
            "fn" => {
                i = read_fn(masked, after, &mut out);
                attrs.clear();
            }
            "impl" => {
                i = read_impl(masked, after, &mut out);
                attrs.clear();
            }
            "add_systems" => {
                i = read_add_systems(masked, after, &mut out);
            }
            w if MODIFIERS.contains(&w) => {
                // `pub(crate)` — the restriction is part of the modifier, not a new item.
                let j = skip_ws(b, after);
                i = if b.get(j) == Some(&b'(') {
                    matching(b, j).map_or(b.len(), |c| c + 1)
                } else {
                    after
                };
            }
            w if CONSUMERS.contains(&w) => {
                attrs.clear();
                i = after;
            }
            _ => i = after,
        }
    }
    out
}

/// After `struct` / `enum`: the name, then the body if it carries field types worth keeping.
fn read_type_decl(
    src: &str,
    at: usize,
    attrs: &[String],
    is_struct: bool,
    out: &mut FileScan,
) -> usize {
    let b = src.as_bytes();
    let start = skip_ws(b, at);
    let Some((name, after)) = ident_at(src, start) else { return start.max(at) };
    let roles = roles_of(attrs);
    // Generics between the name and the body.
    let mut j = skip_ws(b, after);
    if b.get(j) == Some(&b'<') {
        j = matching(b, j).map_or(j, |c| skip_ws(b, c + 1));
    }
    // A `where` clause sits between the generics and the body; skipping to the body is enough.
    let fields = if !is_struct {
        Vec::new()
    } else {
        match b.get(j) {
            Some(&b'{') => matching(b, j)
                .map(|c| field_types(&src[j + 1..c], true))
                .unwrap_or_default(),
            Some(&b'(') => matching(b, j)
                .map(|c| field_types(&src[j + 1..c], false))
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    };
    // A type with no ECS role is not this crate's business — but one with fields might still be a
    // bundle named by a derive this scan could not see, so the filter is the role, checked once.
    if !roles.is_empty() {
        out.types.push(RawType { name, offset: start, roles, fields });
    }
    j
}

/// The field types of a struct body: `a: A, b: B` (named) or `A, B` (tuple).
fn field_types(body: &str, named: bool) -> Vec<String> {
    crate::params::tuple_parts(&format!("({body})"))
        .into_iter()
        .filter_map(|f| {
            let f = f.trim();
            // A named field without a colon is not a field — an attribute on its own line, a
            // `where` fragment the body split badly. Dropped rather than recorded as a type.
            let ty = if named {
                f.find(':').map(|_| crate::params::type_of_binding(f))?
            } else {
                strip_pub(f)
            };
            let ty = strip_pub(ty);
            (!ty.is_empty()).then(|| ty.to_string())
        })
        .collect()
}

/// A tuple-struct field's visibility, dropped — and only when it is one: a type named `public`
/// keeps its name.
fn strip_pub(ty: &str) -> &str {
    let Some(rest) = ty.strip_prefix("pub") else { return ty };
    match rest.chars().next() {
        Some('(') => rest.split_once(')').map_or(ty, |(_, t)| t.trim_start()),
        Some(c) if c.is_whitespace() => rest.trim_start(),
        _ => ty,
    }
}

/// After `fn`: the name and the verbatim parameter list.
fn read_fn(src: &str, at: usize, out: &mut FileScan) -> usize {
    let b = src.as_bytes();
    let start = skip_ws(b, at);
    let Some((name, after)) = ident_at(src, start) else { return start.max(at) };
    let mut j = skip_ws(b, after);
    if b.get(j) == Some(&b'<') {
        j = matching(b, j).map_or(j, |c| skip_ws(b, c + 1));
    }
    let Some(close) = (if b.get(j) == Some(&b'(') { matching(b, j) } else { None }) else {
        return j;
    };
    let params: Vec<String> = split_top(&src[j + 1..close], b',')
        .into_iter()
        .filter(|p| {
            let t = p.trim_start_matches('&').trim();
            let t = t.strip_prefix("mut ").unwrap_or(t).trim();
            !t.starts_with("self") && !t.starts_with('\'')
        })
        .collect();
    if !params.is_empty() {
        out.fns.push(RawFn { name, offset: start, params });
    }
    close + 1
}

/// After `impl`: `Component for Health` — the hand-written form of a derive.
///
/// Both halves are read positionally rather than parsed: with generics and a `where` clause in
/// play, the trait is the **last** token before ` for ` (`impl<T> Component for …`) and the type is
/// the **first** one after it (`… for Health<T> where T: Send`).
fn read_impl(src: &str, at: usize, out: &mut FileScan) -> usize {
    let b = src.as_bytes();
    let end = b[at..]
        .iter()
        .position(|&c| c == b'{' || c == b';')
        .map_or(b.len(), |n| at + n);
    let header = &src[at..end];
    if let Some((tr, ty)) = header.split_once(" for ") {
        let trait_name = bare_name(tr.split_whitespace().next_back().unwrap_or(""));
        let type_name = bare_name(ty.split_whitespace().next().unwrap_or(""));
        if let Some(role) = Role::from_trait(&trait_name) {
            if !type_name.is_empty() {
                out.trait_impls.push((type_name, role));
            }
        }
    }
    end
}

/// A path with its generic arguments dropped, reduced to its last segment: `bevy::prelude::Res<T>`
/// → `Res`.
fn bare_name(token: &str) -> String {
    let t = token.trim().trim_start_matches('&').trim();
    let t = t.split('<').next().unwrap_or(t);
    last_segment(t)
}

/// After the `add_systems` identifier: its arguments.
fn read_add_systems(src: &str, at: usize, out: &mut FileScan) -> usize {
    let b = src.as_bytes();
    let open = skip_ws(b, at);
    if b.get(open) != Some(&b'(') {
        return open;
    }
    let Some(close) = matching(b, open) else { return open + 1 };
    let args = split_top(&src[open + 1..close], b',');
    let Some((schedule, rest)) = args.split_first() else { return close + 1 };
    if !rest.is_empty() {
        let mut reg = systems_in(&rest.join(", "));
        reg.schedule = schedule_label(schedule);
        out.registrations.push(reg);
    }
    close + 1
}

/// A schedule expression as a label: module prefixes off, the rest verbatim — `OnEnter(Playing)`
/// says which state, and dropping that would merge every state transition into one column.
fn schedule_label(expr: &str) -> String {
    let flat = expr.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.find('(') {
        Some(p) => format!("{}{}", last_segment(&flat[..p]), &flat[p..]),
        None => last_segment(&flat),
    }
}

fn last_segment(path: &str) -> String {
    path.trim().rsplit("::").next().unwrap_or(path).trim().to_string()
}

/// The systems a registration expression names, and what it says about their order.
///
/// The rule that keeps a combinator's argument from being read as a registration: a `.method(…)`
/// has its argument list **consumed by the method**, so `after(sync)` contributes an ordering and
/// never a registration, and `run_if(in_state(Playing))` contributes nothing at all. What is left
/// at the top level is the tuple of systems — which is exactly what `add_systems` registers.
fn systems_in(expr: &str) -> Registration {
    let b = expr.as_bytes();
    let mut reg = Registration {
        schedule: String::new(),
        systems: Vec::new(),
        ordered_with: Vec::new(),
        sets: Vec::new(),
        chained: false,
    };
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'.' {
            let j = skip_ws(b, i + 1);
            let Some((method, after)) = ident_at(expr, j) else {
                i += 1;
                continue;
            };
            let k = skip_ws(b, after);
            let (inner, next) = match (b.get(k), matching(b, k)) {
                (Some(&b'('), Some(close)) => (&expr[k + 1..close], close + 1),
                _ => ("", after),
            };
            match method.as_str() {
                "chain" | "chain_ignore_deferred" => reg.chained = true,
                "before" | "after" | "before_ignore_deferred" | "after_ignore_deferred"
                | "ambiguous_with" => reg.ordered_with.extend(plain_paths(inner)),
                "in_set" => reg.sets.extend(plain_paths(inner)),
                _ => {}
            }
            i = next;
            continue;
        }
        if !is_ident_start(b[i]) {
            i += 1;
            continue;
        }
        let (name, next) = read_path(expr, i);
        if let Some(n) = name {
            reg.systems.push(n);
        }
        i = next;
    }
    reg
}

/// The identifiers of a combinator's argument, as system / set names.
fn plain_paths(expr: &str) -> Vec<String> {
    let b = expr.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        if !is_ident_start(b[i]) {
            i += 1;
            continue;
        }
        let (name, next) = read_path(expr, i);
        if let Some(n) = name {
            out.push(n);
        }
        i = next;
    }
    out
}

/// Read one path (`combat::apply_damage`, `spawn::<Enemy>`) and decide whether it names a value or
/// calls one. A call is not a system: `my_plugin(app)` builds systems, it is not one.
fn read_path(src: &str, at: usize) -> (Option<String>, usize) {
    let b = src.as_bytes();
    let mut i = at;
    let mut last = String::new();
    loop {
        let Some((seg, after)) = ident_at(src, i) else { break };
        last = seg;
        i = after;
        let j = skip_ws(b, i);
        if b.get(j) == Some(&b':') && b.get(j + 1) == Some(&b':') {
            let k = skip_ws(b, j + 2);
            // A turbofish ends the path: `spawn::<Enemy>` still names `spawn`.
            if b.get(k) == Some(&b'<') {
                i = matching(b, k).map_or(k + 1, |c| c + 1);
                break;
            }
            i = k;
            continue;
        }
        break;
    }
    let j = skip_ws(b, i);
    match b.get(j) {
        // A call, or a macro: neither is a system reference.
        Some(&b'(') => (None, matching(b, j).map_or(j + 1, |c| c + 1)),
        Some(&b'!') => (None, j + 1),
        _ => ((!last.is_empty()).then_some(last), i),
    }
}

/// The ECS roles an attribute run claims — `#[derive(Component, Debug)]` and nothing else. A role
/// conferred by hand (`impl Component for Health`) is picked up by [`read_impl`] instead.
fn roles_of(attrs: &[String]) -> Vec<Role> {
    let mut roles: Vec<Role> = Vec::new();
    for attr in attrs {
        let a = attr.trim();
        let Some(open) = a.find('(') else { continue };
        if last_segment(&a[..open]) != "derive" {
            continue;
        }
        let Some(close) = a.rfind(')') else { continue };
        if close <= open {
            continue;
        }
        for item in split_top(&a[open + 1..close], b',') {
            if let Some(role) = Role::from_trait(&last_segment(&item)) {
                if !roles.contains(&role) {
                    roles.push(role);
                }
            }
        }
    }
    roles.sort();
    roles
}

// ── Byte mechanics ───────────────────────────────────────────────────────────

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// The identifier at `at`, and the offset after it.
fn ident_at(src: &str, at: usize) -> Option<(String, usize)> {
    let b = src.as_bytes();
    if at >= b.len() || !is_ident_start(b[at]) {
        return None;
    }
    Some(read_ident(src, at))
}

fn read_ident(src: &str, at: usize) -> (String, usize) {
    let b = src.as_bytes();
    let mut end = at;
    while end < b.len() && is_ident_byte(b[end]) {
        end += 1;
    }
    (src[at..end].to_string(), end)
}

/// Offset of the bracket closing the one at `open`, honouring every bracket kind in between so a
/// `(` inside `<…>` cannot end the group early.
fn matching(b: &[u8], open: usize) -> Option<usize> {
    let (o, c) = match b.get(open)? {
        b'(' => (b'(', b')'),
        b'[' => (b'[', b']'),
        b'{' => (b'{', b'}'),
        b'<' => (b'<', b'>'),
        _ => return None,
    };
    let mut depth = 0i32;
    let mut i = open;
    while i < b.len() {
        if b[i] == b'-' && b.get(i + 1) == Some(&b'>') {
            i += 2;
            continue;
        }
        if b[i] == o {
            depth += 1;
        } else if b[i] == c {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Split at `sep`, ignoring anything nested inside brackets of any kind.
fn split_top(s: &str, sep: u8) -> Vec<String> {
    let b = s.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'-' if b.get(i + 1) == Some(&b'>') => i += 1,
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' | b'>' => depth -= 1,
            c if c == sep && depth == 0 => {
                parts.push(s[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(s[start..].trim().to_string());
    parts.into_iter().filter(|p| !p.is_empty()).collect()
}
