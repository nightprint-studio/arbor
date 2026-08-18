//! Reading a system's access set out of its parameter list.
//!
//! This is the piece the whole feature turns on. Bevy decides whether two systems may run at the
//! same time from their `Access<ComponentId>` — derived, at schedule build time, from exactly the
//! types written in these parameters. So the signature is not a hint about what a system touches:
//! it **is** what it touches, and the engine has no other source either.
//!
//! What this module cannot do is resolve a name to a type. `Health` here is the string `Health`,
//! and two `Health`s in two modules collapse into one. Every consequence of that is a *false
//! positive* in one direction only — a conflict claimed between systems that touch different types
//! of the same name — which is why the catalog row always shows the parameter it came from.
//!
//! ## Under-report rather than guess
//!
//! Several shapes are readable but not worth guessing at: an `EntityMut`, a `QueryData` derive, a
//! `SystemParam` from a dependency. Each produces **no access** rather than an invented one. The
//! resulting model is short of some accesses and contains no invented ones, which is the direction
//! bennu errs in everywhere else.

use std::collections::HashMap;

use crate::model::{Access, AccessKind, Filter};
use crate::wrappers;

/// What one parameter contributes.
#[derive(Debug, Default)]
pub struct ParamOutcome {
    pub accesses: Vec<Access>,
    /// The parameter was `&mut World`: the system runs alone.
    pub exclusive: bool,
}

/// The pseudo-type a `&World` parameter reads. Not a real component — it stands for "the whole
/// world", so a whole-world read pairs with the whole-world write an exclusive system performs and
/// with nothing else.
pub const WORLD: &str = "World";

/// The accesses of a whole parameter list, given the project's `#[derive(SystemParam)]` structs
/// (name → field type texts) so a bundled parameter contributes what it bundles.
pub fn accesses_for(
    params: &[String],
    custom: &HashMap<String, Vec<String>>,
) -> (Vec<Access>, bool) {
    let mut out = Vec::new();
    let mut exclusive = false;
    for param in params {
        let outcome = param_accesses(param, custom, 0);
        exclusive |= outcome.exclusive;
        out.extend(outcome.accesses);
    }
    (out, exclusive)
}

/// One parameter — `name: Type`, or just `Type` for the expanded fields of a `SystemParam`.
fn param_accesses(
    param: &str,
    custom: &HashMap<String, Vec<String>>,
    depth: usize,
) -> ParamOutcome {
    let ty = type_of_binding(param);
    let mut outcome = ParamOutcome::default();
    collect(ty, param.trim(), custom, depth, &mut outcome);
    outcome
}

/// The type half of `name: Type`. A parameter with no binding (a bundled field's type, a tuple
/// element) is already a type.
pub fn type_of_binding(param: &str) -> &str {
    match binding_colon(param) {
        Some(i) => param[i + 1..].trim(),
        None => param.trim(),
    }
}

/// Offset of the colon that separates a parameter's name from its type — the first one at bracket
/// depth zero that is not part of a `::` path, so `q: bevy::prelude::Query<…>` splits once and in
/// the right place.
fn binding_colon(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'-' if b.get(i + 1) == Some(&b'>') => i += 1,
            b'<' | b'(' | b'[' => depth += 1,
            b'>' | b')' | b']' => depth -= 1,
            b':' if depth == 0 => {
                if b.get(i + 1) == Some(&b':') {
                    i += 1;
                } else {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Walk one type expression, adding what it accesses to `out`.
fn collect(
    ty: &str,
    param: &str,
    custom: &HashMap<String, Vec<String>>,
    depth: usize,
    out: &mut ParamOutcome,
) {
    if depth > 4 {
        return;
    }
    let ty = ty.trim();
    // `&mut World` / `&World` — the two whole-world parameters, and the only ones whose reference
    // form changes what they mean.
    if let Some(inner) = ty.strip_prefix('&') {
        let inner = strip_lifetime(inner);
        if let Some(rest) = inner.strip_prefix("mut ") {
            if simple_name(rest) == "World" {
                out.exclusive = true;
                return;
            }
        } else if simple_name(inner) == "World" {
            out.accesses.push(Access {
                target: WORLD.to_string(),
                kind: AccessKind::ComponentRead,
                filters: Vec::new(),
                opaque_filter: false,
                param: param.to_string(),
            });
            return;
        }
    }
    let (head, args) = head_and_args(ty);
    let args: Vec<&str> = args.iter().map(|a| a.trim()).filter(|a| !is_lifetime(a)).collect();
    match head.as_str() {
        // The data / filter pair. `Single` and `Populated` are the same shape with a different
        // emptiness rule, which is a run condition rather than an access.
        "Query" | "Populated" | "Single" => {
            if let Some(data) = args.first() {
                let filters = args.get(1).map(|f| filters_of(f, param, out)).unwrap_or_default();
                let opaque = args.get(1).is_some_and(|f| f.contains("Or<") || f.contains("Or <"));
                query_data(data, param, &filters, opaque, out);
            }
        }
        "Res" | "NonSend" => resource(args.last(), param, AccessKind::ResourceRead, out),
        "ResMut" | "NonSendMut" => resource(args.last(), param, AccessKind::ResourceWrite, out),
        // A reader holds `Res<Messages<T>>`, a writer `ResMut<Messages<T>>` — naming the buffer
        // rather than the payload is what makes a reader and a writer of the same message pair up,
        // and what `crate::model::access_keys` looks the declaration up by. The `Event*` spellings
        // are the same thing under Bevy's older name.
        "EventReader" | "MessageReader" => {
            resource(args.first().copied().map(messages).as_ref(), param, AccessKind::ResourceRead, out);
        }
        "EventWriter" | "MessageWriter" | "EventMutator" | "MessageMutator" => {
            resource(args.first().copied().map(messages).as_ref(), param, AccessKind::ResourceWrite, out);
        }
        // Holds several conflicting parameters and hands out one at a time. The system still needs
        // every access in the set, so the union is what other systems must be checked against.
        "ParamSet" => {
            for inner in args.first().map(|a| tuple_parts(a)).unwrap_or_default() {
                collect(&inner, param, custom, depth + 1, out);
            }
        }
        "Option" => {
            if let Some(inner) = args.first() {
                collect(inner, param, custom, depth + 1, out);
            }
        }
        // An observer's first parameter: the event it was triggered with. A read, and the only
        // reason an `#[derive(Event)]` declaration has anyone to point at — an observer is not
        // registered with `add_systems`, so nothing else in this crate would ever mention it.
        "On" | "Trigger" => {
            if let Some(event) = args.first() {
                out.accesses.push(Access {
                    target: type_key(event),
                    kind: AccessKind::ComponentRead,
                    filters: Vec::new(),
                    opaque_filter: false,
                    param: param.to_string(),
                });
            }
        }
        // Deferred by construction: nothing they touch is touched during the schedule.
        "Commands" | "ParallelCommands" | "Local" | "Deferred" | "Gizmos" | "Entity" => {}
        other => {
            // A parameter whose body this crate cannot read — an engine's own wrapper around one
            // of the above. See `crate::wrappers` for why a table rather than an expansion.
            if let Some(effect) = wrappers::lookup(other) {
                wrapped(effect, &args, param, out);
                return;
            }
            // A project `#[derive(SystemParam)]` — expanded once, from its own fields.
            if let Some(fields) = custom.get(other) {
                for f in fields {
                    collect(f, param, custom, depth + 1, out);
                }
            }
        }
    }
}

/// What a known wrapper's arguments amount to. The table says which of the five it is; turning
/// that into accesses is the same code the unwrapped forms go through.
fn wrapped(effect: wrappers::Effect, args: &[&str], param: &str, out: &mut ParamOutcome) {
    use wrappers::Effect;
    let first = args.first().copied();
    match effect {
        Effect::ResourceRead => resource(first, param, AccessKind::ResourceRead, out),
        Effect::ResourceWrite => resource(first, param, AccessKind::ResourceWrite, out),
        Effect::MessageRead => {
            resource(first.map(messages).as_ref(), param, AccessKind::ResourceRead, out);
        }
        Effect::MessageWrite => {
            resource(first.map(messages).as_ref(), param, AccessKind::ResourceWrite, out);
        }
        Effect::QueryLike => {
            if let Some(data) = first {
                let filters = args.get(1).map(|f| filters_of(f, param, out)).unwrap_or_default();
                let opaque = args.get(1).is_some_and(|f| f.contains("Or<"));
                query_data(data, param, &filters, opaque, out);
            }
        }
    }
}

/// `T` → `Messages<T>`, the buffer a reader and a writer of the same message actually contend
/// over — and the key [`crate::model::access_keys`] looks a message declaration up by.
fn messages(t: &str) -> String {
    format!("Messages<{}>", type_key(t))
}

fn resource(
    arg: Option<impl AsRef<str>>,
    param: &str,
    kind: AccessKind,
    out: &mut ParamOutcome,
) {
    let Some(arg) = arg else { return };
    let name = arg.as_ref().trim();
    if name.is_empty() {
        return;
    }
    out.accesses.push(Access {
        target: if name.starts_with("Messages<") { name.to_string() } else { type_key(name) },
        kind,
        filters: Vec::new(),
        opaque_filter: false,
        param: param.to_string(),
    });
}

/// The `D` of a `Query<D, F>`: every component it reads or writes.
fn query_data(
    data: &str,
    param: &str,
    filters: &[Filter],
    opaque: bool,
    out: &mut ParamOutcome,
) {
    for part in tuple_parts(data) {
        let part = part.trim().to_string();
        let (kind, target) = if let Some(rest) = part.strip_prefix('&') {
            let rest = strip_lifetime(rest);
            match rest.strip_prefix("mut ") {
                Some(t) => (AccessKind::ComponentWrite, type_key(t)),
                None => (AccessKind::ComponentRead, type_key(rest)),
            }
        } else {
            let (head, args) = head_and_args(&part);
            let args: Vec<&str> =
                args.iter().map(|a| a.trim()).filter(|a| !is_lifetime(a)).collect();
            match head.as_str() {
                "Mut" => (AccessKind::ComponentWrite, args.first().map(|a| type_key(a)).unwrap_or_default()),
                "Ref" => (AccessKind::ComponentRead, args.first().map(|a| type_key(a)).unwrap_or_default()),
                // Nested shapes: recurse rather than flatten, so `Option<&mut T>` keeps its `mut`.
                "Option" | "AnyOf" => {
                    for inner in args {
                        query_data(inner, param, filters, opaque, out);
                    }
                    continue;
                }
                // `Entity`, `Has<T>`, an `EntityRef`, a `QueryData` derive: no *data* access this
                // crate is willing to claim.
                _ => continue,
            }
        };
        if target.is_empty() {
            continue;
        }
        out.accesses.push(Access {
            target,
            kind,
            filters: filters.to_vec(),
            opaque_filter: opaque,
            param: param.to_string(),
        });
    }
}

/// The `F` of a `Query<D, F>` — and the reads that hide in it: `Changed<T>` and `Added<T>` are
/// filters that consult `T`'s change ticks, which is a read.
fn filters_of(filter: &str, param: &str, out: &mut ParamOutcome) -> Vec<Filter> {
    let mut filters = Vec::new();
    for part in tuple_parts(filter) {
        let (head, args) = head_and_args(part.trim());
        let arg = args.first().map(|a| type_key(a)).unwrap_or_default();
        if arg.is_empty() {
            continue;
        }
        match head.as_str() {
            "With" => filters.push(Filter::With(arg)),
            "Without" => filters.push(Filter::Without(arg)),
            "Added" | "Changed" => out.accesses.push(Access {
                target: arg,
                kind: AccessKind::ComponentRead,
                filters: Vec::new(),
                opaque_filter: false,
                param: param.to_string(),
            }),
            _ => {}
        }
    }
    filters
}

// ── Type-expression mechanics ────────────────────────────────────────────────

/// Whether a generic argument is a lifetime (`'w`, `'static`) rather than a type.
fn is_lifetime(arg: &str) -> bool {
    arg.trim_start().starts_with('\'')
}

/// Drop a leading lifetime from a reference's target: `'w mut T` → `mut T`.
fn strip_lifetime(ty: &str) -> &str {
    let t = ty.trim_start();
    if !t.starts_with('\'') {
        return t;
    }
    t.split_once(char::is_whitespace).map_or("", |(_, rest)| rest.trim_start())
}

/// The last path segment of a type, without its generic arguments: `bevy::prelude::Res<Score>` →
/// `Res`. What a *shape* is decided by — see the module docs on what dropping the path costs.
pub fn simple_name(ty: &str) -> String {
    let head = head_and_args(ty).0;
    head.rsplit("::").next().unwrap_or(&head).trim().to_string()
}

/// The name an access is **recorded under**: the head's last segment, with its generic arguments
/// kept and reduced the same way. `bevy::prelude::NextState<GameState>` → `NextState<GameState>`.
///
/// The arguments are not decoration. Two systems writing `NextState<GameState>` contend; one
/// writing `NextState<GameState>` and one writing `NextState<MenuPage>` do not, and dropping the
/// argument would have reported them as if they did — a false conflict, on a pair that is fine.
pub fn type_key(ty: &str) -> String {
    let (head, args) = head_and_args(ty);
    let args: Vec<String> =
        args.iter().filter(|a| !is_lifetime(a)).map(|a| type_key(a)).collect();
    match args.is_empty() {
        true => head,
        false => format!("{head}<{}>", args.join(", ")),
    }
}

/// A type split into its head path and its top-level generic arguments.
pub fn head_and_args(ty: &str) -> (String, Vec<String>) {
    let ty = ty.trim().trim_start_matches('&').trim();
    let ty = strip_lifetime(ty);
    let ty = ty.strip_prefix("mut ").unwrap_or(ty).trim();
    let Some(open) = find_top(ty, '<') else {
        return (ty.rsplit("::").next().unwrap_or(ty).trim().to_string(), Vec::new());
    };
    let head = ty[..open].rsplit("::").next().unwrap_or(&ty[..open]).trim().to_string();
    let close = matching(ty, open).unwrap_or(ty.len());
    (head, split_top(&ty[open + 1..close], ','))
}

/// The elements of a tuple type, or the one element that is not a tuple. What lets `&mut A` and
/// `(&mut A, &B)` be walked by the same code.
pub fn tuple_parts(ty: &str) -> Vec<String> {
    let t = ty.trim();
    match t.strip_prefix('(').and_then(|_| matching(t, 0).map(|c| &t[1..c])) {
        Some(inner) => split_top(inner, ','),
        None => vec![t.to_string()],
    }
}

/// Split at `sep`, ignoring anything nested inside brackets.
fn split_top(s: &str, sep: char) -> Vec<String> {
    let b = s.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'<' | b'(' | b'[' => depth += 1,
            b'>' | b')' | b']' => depth -= 1,
            // `->` is not a closing angle bracket.
            b'-' if b.get(i + 1) == Some(&b'>') => i += 1,
            c if depth == 0 && c == sep as u8 => {
                parts.push(s[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let tail = s[start..].trim();
    if !tail.is_empty() || !parts.is_empty() {
        parts.push(tail.to_string());
    }
    parts.into_iter().filter(|p| !p.is_empty()).collect()
}

/// Offset of the first `c` at bracket depth zero.
fn find_top(s: &str, c: char) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 0i32;
    for (i, &ch) in b.iter().enumerate() {
        match ch {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            x if x == c as u8 && depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Offset of the bracket closing the one at `open`.
fn matching(s: &str, open: usize) -> Option<usize> {
    let b = s.as_bytes();
    let (o, c) = match b.get(open)? {
        b'<' => (b'<', b'>'),
        b'(' => (b'(', b')'),
        b'[' => (b'[', b']'),
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
