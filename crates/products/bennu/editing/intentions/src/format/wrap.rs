//! Line wrapping: where a statement that is too long (or is told to always break) gets its line breaks.
//!
//! The unit of work is a [`Unit`](super::structure::Unit) — one statement or declaration header. Its
//! tokens are scanned into *constructs*: an argument list, a parameter list, a chain of calls, a run
//! of binary operators, a ternary, a `throws` clause. Each construct knows the token positions where a
//! break may go and carries the policy the style gives that kind of construct.
//!
//! Then, in order:
//!
//!  1. the line breaks the author put at a construct's positions are removed (a `Preserve` construct
//!     keeps them) — a policy decides where breaks go, not a mixture of the policy and the past;
//!  2. `Always` constructs break at every position;
//!  3. while some line is wider than the margin, the **outermost** construct that has a position on it
//!     is applied — `Chop` breaks every position, `Fill` breaks at the last position that still lets
//!     the line fit — and the line is measured again. A line nothing can break is left alone.
//!
//! Lines are measured by the same [`Machine`] that will lay them out, so a wrapped line is judged at
//! the indent it will really have. A unit that contains a lambda block or an anonymous class (so its
//! brackets do not balance within it) is not touched: where to break through a block is not something
//! to guess.

use std::collections::HashSet;

use super::classify::{prev_sig, BraceKind, Info, Role, NO_MATCH};
use super::layout::{gap_text, lines_of, text_width, Ctx, Machine};
use super::lex::{Kind, Tok};
use super::structure::Unit;
use super::style::{FormatStyle, WrapPolicy};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CKind {
    Params,
    Args,
    ArrayInit,
    Chain,
    Binary,
    Ternary,
    Clauses,
}

impl CKind {
    /// Which of two constructs at the same depth is broken first.
    fn rank(self) -> u8 {
        match self {
            CKind::Ternary => 0,
            CKind::Binary => 1,
            CKind::Chain => 2,
            CKind::Params | CKind::Args | CKind::ArrayInit => 3,
            CKind::Clauses => 4,
        }
    }
}

#[derive(Clone, Debug)]
struct Construct {
    kind: CKind,
    policy: WrapPolicy,
    /// Token indices a line break may go *before*, ascending.
    positions: Vec<usize>,
    /// Further positions whose existing breaks are removed along with the rest (`(` + 1, `)`).
    flatten: Vec<usize>,
    /// The `)` / `}` that goes on its own line when the construct is fully broken.
    close_break: Option<usize>,
    depth: u32,
    applied: bool,
}

/// Remove the line break before a token (keeping the code on one line).
fn join(t: &mut Tok) {
    t.nl_before = 0;
    t.gap = " ".to_string();
    t.ws_before = true;
}

fn put_break(t: &mut Tok) {
    t.nl_before = t.nl_before.max(1);
}

// ── Scanning ────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum GroupKind {
    Params,
    Args,
    Array,
    Plain,
}

struct Scanner<'a> {
    toks: &'a [Tok],
    info: &'a [Info],
    style: &'a FormatStyle,
    decl: bool,
    seen_paren: bool,
    out: Vec<Construct>,
}

fn tier(op: &str) -> Option<u8> {
    Some(match op {
        "||" => 0,
        "&&" => 1,
        "|" => 2,
        "^" => 3,
        "&" => 4,
        "==" | "!=" => 5,
        "<" | ">" | "<=" | ">=" => 6,
        "<<" => 7,
        "+" | "-" => 8,
        "*" | "/" | "%" => 9,
        _ => return None,
    })
}

impl Scanner<'_> {
    fn group_kind(&mut self, open: usize, close: usize, top: bool) -> GroupKind {
        let toks = self.toks;
        if toks[open].is("{") {
            return GroupKind::Array;
        }
        if toks[open].is("[") || self.info[open].role == Role::CastOpen {
            return GroupKind::Plain;
        }
        // `(a, b) -> …` — lambda parameters.
        if toks.get(close + 1).is_some_and(|n| n.is("->")) {
            return GroupKind::Plain;
        }
        let Some(p) = prev_sig(toks, open) else { return GroupKind::Plain };
        let pt = &toks[p];
        if pt.is_word() {
            if super::classify::is_control_word(&pt.text) {
                return GroupKind::Plain;
            }
            if matches!(pt.text.as_str(), "return" | "throw" | "case" | "else" | "assert" | "yield" | "in") {
                return GroupKind::Plain;
            }
            let first = !self.seen_paren;
            self.seen_paren = true;
            if self.decl && top && first {
                return GroupKind::Params;
            }
            return GroupKind::Args;
        }
        if self.info[p].role == Role::GenericClose {
            return GroupKind::Args;
        }
        GroupKind::Plain
    }

    fn policy(&self, kind: CKind) -> WrapPolicy {
        let s = self.style;
        match kind {
            CKind::Params => s.wrap_parameters,
            CKind::Args => s.wrap_arguments,
            CKind::ArrayInit => s.wrap_array_init,
            CKind::Chain => s.wrap_chain,
            CKind::Binary => s.wrap_binary,
            CKind::Ternary => s.wrap_ternary,
            CKind::Clauses => s.wrap_clauses,
        }
    }

    fn push(&mut self, kind: CKind, positions: Vec<usize>, flatten: Vec<usize>, close_break: Option<usize>, depth: u32) {
        let policy = self.policy(kind);
        if positions.is_empty() {
            return;
        }
        let depth = if kind == CKind::Params { 0 } else if kind == CKind::Clauses { 1 } else { depth };
        self.out.push(Construct { kind, policy, positions, flatten, close_break, depth, applied: false });
    }

    /// Close a chain segment: a run of `.call()`s not interrupted by an operator.
    fn flush_chain(&mut self, seg: &mut Vec<usize>, depth: u32) {
        let dots = std::mem::take(seg);
        if dots.len() < self.style.chain_min_calls.max(1) {
            return;
        }
        let keep_first = self.style.chain_keep_first_call
            && dots.first().is_some_and(|&d| d > 0 && self.toks[d - 1].is_word());
        let positions: Vec<usize> = dots.iter().copied().skip(usize::from(keep_first)).collect();
        self.push(CKind::Chain, positions.clone(), positions, None, depth);
    }

    fn frame(&mut self, lo: usize, hi: usize, depth: u32, group: Option<(GroupKind, usize, usize)>) {
        let toks = self.toks;
        let info = self.info;
        let style = self.style;
        let top = group.is_none();
        let mut commas: Vec<usize> = Vec::new();
        let mut ops: Vec<(u8, usize)> = Vec::new();
        let mut tern: Vec<usize> = Vec::new();
        let mut seg: Vec<usize> = Vec::new();
        let mut clauses: Vec<usize> = Vec::new();
        let mut generic = 0usize;

        let mut i = lo;
        while i < hi {
            let t = &toks[i];
            if t.is_comment() {
                i += 1;
                continue;
            }
            match info[i].role {
                Role::GenericOpen => generic += 1,
                Role::GenericClose => generic = generic.saturating_sub(1),
                _ => {}
            }
            let opens = t.is("(") || t.is("[") || (t.is("{") && info[i].brace == Some(BraceKind::Array));
            if opens {
                let m = info[i].matching;
                if m != NO_MATCH && m < hi {
                    let kind = self.group_kind(i, m, top);
                    self.frame(i + 1, m, depth + 1, Some((kind, i, m)));
                    i = m + 1;
                    continue;
                }
            }
            if t.kind == Kind::Punct && generic == 0 {
                if t.is(",") {
                    commas.push(i);
                } else if t.is(".") {
                    if toks.get(i + 1).is_some_and(Tok::is_word) && toks.get(i + 2).is_some_and(|x| x.is("(")) {
                        seg.push(i);
                    }
                } else {
                    match info[i].role {
                        Role::Binary => {
                            // `>>` and `>>>` are shifts: their halves are not break points.
                            let shift = t.is(">")
                                && (toks.get(i + 1).is_some_and(|n| (n.is(">") || n.is(">=")) && !n.ws_before)
                                    || (i > 0 && toks[i - 1].is(">") && !t.ws_before));
                            if let Some(tr) = tier(&t.text) {
                                if !shift {
                                    ops.push((tr, i));
                                }
                            }
                            self.flush_chain(&mut seg, depth);
                        }
                        Role::Assign | Role::Arrow => self.flush_chain(&mut seg, depth),
                        Role::TernaryQ | Role::TernaryColon => {
                            tern.push(i);
                            self.flush_chain(&mut seg, depth);
                        }
                        _ => {}
                    }
                }
            } else if top && generic == 0 && t.is_word() && matches!(t.text.as_str(), "extends" | "implements" | "throws" | "permits") {
                clauses.push(i);
            }
            i += 1;
        }
        self.flush_chain(&mut seg, depth);

        // The frame's own list, when it is an argument / parameter / initialiser group.
        if let Some((kind, open, close)) = group {
            let ckind = match kind {
                GroupKind::Params => Some(CKind::Params),
                GroupKind::Args => Some(CKind::Args),
                GroupKind::Array => Some(CKind::ArrayInit),
                GroupKind::Plain => None,
            };
            if let Some(ckind) = ckind {
                let mut positions: Vec<usize> = commas.iter().map(|c| c + 1).filter(|&p| p < close).collect();
                let mut flatten = positions.clone();
                if lo < close {
                    flatten.push(open + 1);
                    if self.policy(ckind) == WrapPolicy::Chop || self.policy(ckind) == WrapPolicy::Always {
                        if style.break_after_open_paren {
                            positions.insert(0, open + 1);
                        }
                    }
                }
                flatten.push(close);
                let close_break = style.closing_paren_on_new_line.then_some(close);
                self.push(ckind, positions, flatten, close_break, depth);
            }
        }

        if top && !clauses.is_empty() {
            self.push(CKind::Clauses, clauses.clone(), clauses, None, depth);
        }

        if tern.len() >= 2 {
            let positions: Vec<usize> =
                tern.iter().map(|&o| if style.operator_on_next_line { o } else { o + 1 }).filter(|&p| p < hi).collect();
            self.push(CKind::Ternary, positions.clone(), positions, None, depth);
        }

        if let Some(&(low, _)) = ops.iter().min_by_key(|(t, _)| *t) {
            let positions: Vec<usize> = ops
                .iter()
                .filter(|(t, _)| *t == low)
                .map(|&(_, o)| if style.operator_on_next_line { o } else { o + 1 })
                .filter(|&p| p < hi)
                .collect();
            self.push(CKind::Binary, positions.clone(), positions, None, depth);
        }
    }
}

/// Whether the unit is a declaration header (a method, constructor or record) rather than a statement.
fn is_declaration(toks: &[Tok], info: &[Info], u: &Unit) -> bool {
    if !matches!(info[u.start].scope, None | Some(BraceKind::Type) | Some(BraceKind::Anon)) {
        return false;
    }
    if !(toks[u.end].is("{") || toks[u.end].is(";")) {
        return false;
    }
    let mut i = u.start;
    while i <= u.end && toks[i].is_comment() {
        i += 1;
    }
    // Leading annotations.
    while i < u.end && toks[i].is("@") {
        i += 2;
        while i + 1 <= u.end && toks[i].is(".") {
            i += 2;
        }
        if i <= u.end && toks[i].is("(") && info[i].matching != NO_MATCH {
            i = info[i].matching + 1;
        }
    }
    if i > u.end {
        return false;
    }
    if toks[i].is_word()
        && matches!(
            toks[i].text.as_str(),
            "return" | "throw" | "new" | "this" | "super" | "if" | "for" | "while" | "switch" | "try" | "do" | "else"
                | "case" | "default" | "yield" | "assert" | "break" | "continue"
        )
    {
        return false;
    }
    // No assignment before the first parenthesis.
    for t in toks[i..=u.end].iter().enumerate() {
        let (k, tok) = t;
        if tok.is("(") {
            break;
        }
        if info[i + k].role == Role::Assign {
            return false;
        }
    }
    true
}

/// Every bracket opened in the unit is closed in it, and no line comment cuts it short.
fn is_wrappable(toks: &[Tok], info: &[Info], u: &Unit) -> bool {
    let mut depth = 0i32;
    for i in u.start..=u.end {
        let t = &toks[i];
        if t.kind == Kind::LineComment && i < u.end {
            return false;
        }
        if t.kind != Kind::Punct {
            continue;
        }
        match t.text.as_str() {
            "(" | "[" => depth += 1,
            ")" | "]" => depth -= 1,
            "{" if info[i].brace == Some(BraceKind::Array) => depth += 1,
            "}" if info[i].brace == Some(BraceKind::Array) => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

// ── Measuring ───────────────────────────────────────────────────────────────────────────

struct SimLine {
    first: usize,
    last: usize,
    width: usize,
}

/// Lay out `from..=to` on a copy of the machine and report each line's width — a trailing line
/// comment is not counted, since a comment is never a reason to break code.
fn simulate(toks: &[Tok], ctx: &Ctx, machine: &Machine, from: usize, to: usize) -> Vec<SimLine> {
    let mut m = machine.clone();
    let iw = ctx.style.indent_width.max(1);
    lines_of(toks, from, to)
        .into_iter()
        .map(|(first, last)| {
            let out = m.layout_line(toks, ctx, first, last);
            let mut width = out.width;
            if last > first && toks[last].kind == Kind::LineComment {
                let tail = format!("{}{}", gap_text(toks, ctx, last), toks[last].text);
                width = width.saturating_sub(text_width(&tail, iw));
            }
            SimLine { first, last, width }
        })
        .collect()
}

/// The width the line `first..p` would have if it broke before `p`.
fn width_before(toks: &[Tok], ctx: &Ctx, machine: &Machine, first: usize, p: usize) -> usize {
    let mut m = machine.clone();
    let out = m.layout_line(toks, ctx, first, p - 1);
    out.width
}

// ── The pass ────────────────────────────────────────────────────────────────────────────

/// Wrap one unit. `machine` is the layout state before the line the unit starts on (or in), and
/// `sim_start` that line's first token.
pub(crate) fn wrap_unit(toks: &mut [Tok], ctx: &Ctx, machine: &Machine, sim_start: usize, u: &Unit) {
    let style = ctx.style;
    let all_preserve = [
        style.wrap_parameters,
        style.wrap_arguments,
        style.wrap_chain,
        style.wrap_binary,
        style.wrap_ternary,
        style.wrap_clauses,
        style.wrap_array_init,
    ]
    .iter()
    .all(|p| *p == WrapPolicy::Preserve);
    if all_preserve || u.end <= u.start || !is_wrappable(toks, ctx.info, u) {
        return;
    }

    let mut scanner = Scanner {
        toks,
        info: ctx.info,
        style,
        decl: is_declaration(toks, ctx.info, u),
        seen_paren: false,
        out: Vec::new(),
    };
    scanner.frame(u.start, u.end + 1, 0, None);
    let mut constructs = scanner.out;
    constructs.retain(|c| c.policy != WrapPolicy::Preserve);
    if constructs.is_empty() {
        return;
    }

    // 1. Forget the author's breaks at the positions we now own.
    for c in &constructs {
        for &p in c.positions.iter().chain(c.flatten.iter()) {
            if p > u.start && p <= u.end && toks[p].nl_before > 0 && !toks[p - 1].is_comment() {
                join(&mut toks[p]);
            }
        }
    }

    // 2. Always.
    for c in constructs.iter_mut().filter(|c| c.policy == WrapPolicy::Always) {
        break_all(toks, c);
        c.applied = true;
    }

    // 3. The margin.
    if style.line_width == 0 {
        return;
    }
    let mut skipped: HashSet<usize> = HashSet::new();
    for _ in 0..200 {
        let lines = simulate(toks, ctx, machine, sim_start, u.end);
        let Some(bad) = lines
            .iter()
            .find(|l| l.width > style.line_width && l.last >= u.start && !skipped.contains(&l.first))
        else {
            break;
        };
        let candidate = constructs
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.applied && matches!(c.policy, WrapPolicy::Fill | WrapPolicy::Chop))
            .filter(|(_, c)| {
                c.positions.iter().any(|&p| p > bad.first && p <= bad.last && toks[p].nl_before == 0)
            })
            .min_by_key(|(_, c)| (c.depth, c.kind.rank()))
            .map(|(i, _)| i);
        let Some(ci) = candidate else {
            skipped.insert(bad.first);
            continue;
        };
        if constructs[ci].policy == WrapPolicy::Chop {
            break_all(toks, &mut constructs[ci]);
            constructs[ci].applied = true;
        } else {
            fill_once(toks, ctx, machine, sim_start, bad.first, bad.last, style.line_width, &mut constructs[ci]);
        }
    }
}

fn break_all(toks: &mut [Tok], c: &mut Construct) {
    for &p in &c.positions {
        if toks[p].nl_before == 0 {
            put_break(&mut toks[p]);
        }
    }
    if let Some(close) = c.close_break {
        if toks[close].nl_before == 0 {
            put_break(&mut toks[close]);
        }
    }
}

/// Break the overflowing line `first..=last` at the last position of `c` that lets what precedes it
/// fit (or at its first position, if none does). Marks `c` applied once it has nothing left to break.
#[allow(clippy::too_many_arguments)]
fn fill_once(
    toks: &mut [Tok],
    ctx: &Ctx,
    machine: &Machine,
    sim_start: usize,
    first: usize,
    last: usize,
    width: usize,
    c: &mut Construct,
) {
    let on_line: Vec<usize> =
        c.positions.iter().copied().filter(|&p| p > first && p <= last && toks[p].nl_before == 0).collect();
    if on_line.is_empty() {
        c.applied = true;
        return;
    }
    // Lay out the lines before `first` so the machine is in the right state for it.
    let mut m = machine.clone();
    for (f, l) in lines_of(toks, sim_start, first.max(sim_start + 0)).into_iter().filter(|(f, _)| *f < first) {
        let l = l.min(first - 1);
        m.layout_line(toks, ctx, f, l);
    }
    let mut chosen = on_line[0];
    for &p in &on_line {
        if width_before(toks, ctx, &m, first, p) <= width {
            chosen = p;
        } else {
            break;
        }
    }
    put_break(&mut toks[chosen]);
    if on_line.len() == 1 {
        c.applied = true;
        if let Some(close) = c.close_break {
            if toks[close].nl_before == 0 {
                put_break(&mut toks[close]);
            }
        }
    }
}
