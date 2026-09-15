//! Definite assignment of locals (JLS chapter 16).
//!
//! Two questions about a local, asked along every path through the body that declares it:
//!
//! * is it read before it is **definitely assigned** — `int x; if (c) { x = 1; } use(x);` is javac's
//!   `var.might.not.have.been.initialized`;
//! * is a blank `final` assigned where it is not **definitely unassigned** — twice on one path
//!   (`var.might.already.be.assigned`), or inside a loop that can come round again
//!   (`var.might.be.assigned.in.loop`).
//!
//! ## The model
//!
//! The rules are the JLS's, statement by statement: a branch joins its arms, a loop's body adds
//! nothing to what is assigned after it (only its `break`s do), a `catch` starts from what was
//! assigned before its `try`, a `finally` that assigns counts whatever ran before it. A statement that
//! cannot complete normally leaves nothing to follow — every local is vacuously assigned there, which
//! is what makes `if (x == null) { return; }` legal before a read.
//!
//! Soundness rests on one rule. Where the model does not follow a construct — a `switch` expression,
//! a `switch` with patterns, a condition that might be a constant — every local the construct
//! mentions is **abandoned** and never reported again. Abandoning a name can hide an error; it can
//! never invent one. Lambda and class bodies run later, so they are not entered at all: a lambda body
//! is a unit of its own.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;

/// Every definite-assignment finding in the file.
pub fn definite_assignment_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        let body = match n.kind() {
            "method_declaration" | "constructor_declaration" | "compact_constructor_declaration"
            | "lambda_expression" => n.child_by_field_name("body"),
            "static_initializer" => crate::support::nodes::child_of_kind(n, "block"),
            // An instance initializer.
            "block" if n.parent().is_some_and(|p| p.kind() == "class_body") => Some(n),
            _ => None,
        };
        let Some(body) = body.filter(|b| b.kind() == "block" && !b.has_error()) else { continue };
        let mut unit = Unit::new(bytes);
        unit.statement(body, Some(State::default()));
        out.extend(unit.out);
    }
    out.sort_by_key(|d| d.start);
    out
}

/// Whether an assignment is a plain `=` rather than a compound `+=` / `>>=`, which reads its target.
pub(crate) fn is_plain_assignment(e: Node, bytes: &[u8]) -> bool {
    let (Some(left), Some(right)) = (e.child_by_field_name("left"), e.child_by_field_name("right"))
    else {
        return false;
    };
    e.child_by_field_name("operator")
        .and_then(|o| o.utf8_text(bytes).ok())
        .map(|op| op == "=")
        .unwrap_or_else(|| {
            // Some grammar builds have no `operator` field; the text between the two sides is it.
            bytes
                .get(left.end_byte()..right.start_byte())
                .and_then(|s| std::str::from_utf8(s).ok())
                .is_some_and(|s| s.trim() == "=")
        })
}

/// What is known of the locals at a point that can be reached.
#[derive(Debug, Clone, Default)]
struct State {
    /// In-scope locals that are NOT definitely assigned.
    unassigned: BTreeSet<String>,
    /// Blank `final` locals that MAY have been assigned already, with the span of an assignment.
    assigned: BTreeMap<String, (usize, usize)>,
}

/// Two paths meeting: a local is assigned after only if it is assigned on both, and may have been
/// assigned if it may on either. An unreachable path (`None`) adds nothing.
fn join(a: Option<State>, b: Option<State>) -> Option<State> {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(mut a), Some(b)) => {
            a.unassigned.extend(b.unassigned);
            for (name, span) in b.assigned {
                a.assigned.entry(name).or_insert(span);
            }
            Some(a)
        }
    }
}

/// Where a `break` or `continue` lands, and the states that arrive there by one.
struct Target {
    label: Option<String>,
    /// An unlabeled `break` stops here (a loop or a `switch`).
    breakable: bool,
    /// An unlabeled `continue` stops here.
    is_loop: bool,
    breaks: Option<State>,
    continues: Option<State>,
}

/// One body being followed.
struct Unit<'b> {
    bytes: &'b [u8],
    /// The names declared blank `final`, most recent declaration wins.
    blank_finals: HashSet<String>,
    abandoned: HashSet<String>,
    /// Names already reported as read unassigned — one report per local, as javac gives.
    reported: HashSet<String>,
    /// The locals each open block declares, innermost last.
    scopes: Vec<Vec<String>>,
    targets: Vec<Target>,
    /// A label written on the next statement, for the loop or `switch` that takes it.
    pending_label: Option<String>,
    /// Assignment sites already reported for a loop, so an outer loop does not repeat an inner one.
    loop_reported: HashSet<usize>,
    out: Vec<Diagnostic>,
}

impl<'b> Unit<'b> {
    fn new(bytes: &'b [u8]) -> Self {
        Unit {
            bytes,
            blank_finals: HashSet::new(),
            abandoned: HashSet::new(),
            reported: HashSet::new(),
            scopes: vec![Vec::new()],
            targets: Vec::new(),
            pending_label: None,
            loop_reported: HashSet::new(),
            out: Vec::new(),
        }
    }

    fn text(&self, n: Node) -> Option<String> {
        n.utf8_text(self.bytes).ok().map(str::to_string)
    }

    fn in_scope(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.iter().any(|n| n == name))
    }

    // ── scopes ───────────────────────────────────────────────────────────────

    /// Bring a local into the innermost scope, `assigned` or not.
    fn declare(&mut self, name: String, state: &mut State, assigned: bool, blank_final: bool) {
        self.abandoned.remove(&name);
        self.reported.remove(&name);
        state.assigned.remove(&name);
        if blank_final {
            self.blank_finals.insert(name.clone());
        } else {
            self.blank_finals.remove(&name);
        }
        if assigned {
            state.unassigned.remove(&name);
        } else {
            state.unassigned.insert(name.clone());
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(name);
        }
    }

    fn open_scope(&mut self) {
        self.scopes.push(Vec::new());
    }

    /// Leave the innermost scope: its locals are no longer anyone's business.
    fn close_scope(&mut self, state: Option<State>) -> Option<State> {
        let names = self.scopes.pop().unwrap_or_default();
        state.map(|mut s| {
            for name in &names {
                s.unassigned.remove(name);
                s.assigned.remove(name);
            }
            s
        })
    }

    /// A state that arrived by a jump, cut down to the locals in scope where it lands.
    fn trim(&self, state: Option<State>) -> Option<State> {
        state.map(|mut s| {
            s.unassigned.retain(|n| self.in_scope(n));
            s.assigned.retain(|n, _| self.in_scope(n));
            s
        })
    }

    /// Stop judging every in-scope local `node` mentions.
    fn abandon(&mut self, node: Node) {
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if n.kind() == "identifier" {
                if let Some(name) = self.text(n).filter(|name| self.in_scope(name)) {
                    self.abandoned.insert(name);
                }
            }
            let mut c = n.walk();
            stack.extend(n.named_children(&mut c));
        }
    }

    // ── statements ───────────────────────────────────────────────────────────

    /// The state after `stmt` completes normally, or `None` when it cannot.
    fn statement(&mut self, stmt: Node, state: Option<State>) -> Option<State> {
        let label = self.pending_label.take();
        let state = state?;
        match stmt.kind() {
            "block" => self.block(stmt, state),
            "local_variable_declaration" => Some(self.declaration(stmt, state)),
            "expression_statement" => match stmt.named_child(0) {
                Some(inner) if inner.kind() == "switch_expression" => self.switch_statement(inner, state, label),
                _ => Some(self.children(stmt, state)),
            },
            "if_statement" => self.if_statement(stmt, state),
            "while_statement" => self.while_statement(stmt, state, label),
            "do_statement" => self.do_statement(stmt, state, label),
            "for_statement" => self.for_statement(stmt, state, label),
            "enhanced_for_statement" => self.enhanced_for(stmt, state, label),
            "labeled_statement" => self.labeled(stmt, state),
            "switch_expression" => self.switch_statement(stmt, state, label),
            "try_statement" | "try_with_resources_statement" => self.try_statement(stmt, state),
            "return_statement" | "throw_statement" | "yield_statement" => {
                self.children(stmt, state);
                None
            }
            "break_statement" => {
                self.jump(stmt, state, false);
                None
            }
            "continue_statement" => {
                self.jump(stmt, state, true);
                None
            }
            "synchronized_statement" => {
                let mut state = state;
                let mut c = stmt.walk();
                let parts: Vec<Node> = stmt.named_children(&mut c).collect();
                for part in parts {
                    match part.kind() {
                        "block" => return self.statement(part, Some(state)),
                        _ => state = self.expr(part, state),
                    }
                }
                Some(state)
            }
            // An assertion may be disabled: what it assigns is never counted, what it reads is.
            "assert_statement" => {
                self.children(stmt, state.clone());
                Some(state)
            }
            "explicit_constructor_invocation" => Some(self.children(stmt, state)),
            "line_comment" | "block_comment" | ";" | "empty_statement" | "class_declaration"
            | "record_declaration" | "interface_declaration" | "enum_declaration" => Some(state),
            _ => {
                self.abandon(stmt);
                Some(state)
            }
        }
    }

    fn block(&mut self, block: Node, state: State) -> Option<State> {
        self.open_scope();
        let mut state = Some(state);
        let mut c = block.walk();
        let statements: Vec<Node> = block.named_children(&mut c).collect();
        for stmt in statements {
            if state.is_none() {
                break; // unreachable code is its own error, and nothing is unassigned there
            }
            state = self.statement(stmt, state);
        }
        self.close_scope(state)
    }

    fn declaration(&mut self, decl: Node, mut state: State) -> State {
        let is_final = crate::support::nodes::modifier_keywords(decl, self.bytes).contains(&"final");
        let mut c = decl.walk();
        let declarators: Vec<Node> =
            decl.named_children(&mut c).filter(|d| d.kind() == "variable_declarator").collect();
        for d in declarators {
            let Some(name) = d.child_by_field_name("name").and_then(|n| self.text(n)) else { continue };
            match d.child_by_field_name("value") {
                Some(value) => {
                    state = self.expr(value, state);
                    self.declare(name, &mut state, true, false);
                }
                None => self.declare(name, &mut state, false, is_final),
            }
        }
        state
    }

    fn if_statement(&mut self, stmt: Node, state: State) -> Option<State> {
        let Some(cond) = stmt.child_by_field_name("condition") else {
            self.abandon(stmt);
            return Some(state);
        };
        let (when_true, when_false) = self.branch_condition(stmt, cond, state);
        let then = match stmt.child_by_field_name("consequence") {
            Some(c) => self.statement(c, when_true),
            None => when_true,
        };
        let other = match stmt.child_by_field_name("alternative") {
            Some(a) => self.statement(a, when_false),
            None => when_false,
        };
        join(then, other)
    }

    fn while_statement(&mut self, stmt: Node, state: State, label: Option<String>) -> Option<State> {
        let entry = state.assigned.clone();
        let (when_true, when_false) = match stmt.child_by_field_name("condition") {
            Some(c) => self.branch_condition(stmt, c, state),
            None => (Some(state), None),
        };
        self.targets.push(Target { label, breakable: true, is_loop: true, breaks: None, continues: None });
        let end = match stmt.child_by_field_name("body") {
            Some(b) => self.statement(b, when_true),
            None => when_true,
        };
        let target = self.targets.pop().expect("pushed above");
        self.loop_reassignment(&entry, &join(end, target.continues));
        let breaks = self.trim(target.breaks);
        join(when_false, breaks)
    }

    fn do_statement(&mut self, stmt: Node, state: State, label: Option<String>) -> Option<State> {
        let entry = state.assigned.clone();
        self.targets.push(Target { label, breakable: true, is_loop: true, breaks: None, continues: None });
        let end = match stmt.child_by_field_name("body") {
            Some(b) => self.statement(b, Some(state)),
            None => Some(state),
        };
        let target = self.targets.pop().expect("pushed above");
        let before_condition = join(end, target.continues);
        self.loop_reassignment(&entry, &before_condition);
        let when_false = match (stmt.child_by_field_name("condition"), before_condition) {
            (Some(c), Some(s)) => self.branch_condition(stmt, c, s).1,
            _ => None,
        };
        let breaks = self.trim(target.breaks);
        join(when_false, breaks)
    }

    fn for_statement(&mut self, stmt: Node, state: State, label: Option<String>) -> Option<State> {
        self.open_scope();
        let mut state = state;
        let mut c = stmt.walk();
        let inits: Vec<Node> = stmt.children_by_field_name("init", &mut c).collect();
        for init in inits {
            state = match init.kind() {
                "local_variable_declaration" => self.declaration(init, state),
                _ => self.expr(init, state),
            };
        }
        let entry = state.assigned.clone();
        let (when_true, when_false) = match stmt.child_by_field_name("condition") {
            Some(cond) => self.branch_condition(stmt, cond, state),
            None => (Some(state), None),
        };
        self.targets.push(Target { label, breakable: true, is_loop: true, breaks: None, continues: None });
        let end = match stmt.child_by_field_name("body") {
            Some(b) => self.statement(b, when_true),
            None => when_true,
        };
        let target = self.targets.pop().expect("pushed above");
        let mut back = join(end, target.continues);
        let mut c = stmt.walk();
        let updates: Vec<Node> = stmt.children_by_field_name("update", &mut c).collect();
        for update in updates {
            back = back.map(|s| self.expr(update, s));
        }
        self.loop_reassignment(&entry, &back);
        let breaks = self.trim(target.breaks);
        let result = join(when_false, breaks);
        self.close_scope(result)
    }

    fn enhanced_for(&mut self, stmt: Node, state: State, label: Option<String>) -> Option<State> {
        let mut state = match stmt.child_by_field_name("value") {
            Some(v) => self.expr(v, state),
            None => state,
        };
        self.open_scope();
        if let Some(name) = stmt.child_by_field_name("name").and_then(|n| self.text(n)) {
            self.declare(name, &mut state, true, false);
        }
        let entry = state.assigned.clone();
        self.targets.push(Target { label, breakable: true, is_loop: true, breaks: None, continues: None });
        let end = match stmt.child_by_field_name("body") {
            Some(b) => self.statement(b, Some(state.clone())),
            None => Some(state.clone()),
        };
        let target = self.targets.pop().expect("pushed above");
        self.loop_reassignment(&entry, &join(end, target.continues));
        let breaks = self.trim(target.breaks);
        let result = join(Some(state), breaks);
        self.close_scope(result)
    }

    fn labeled(&mut self, stmt: Node, state: State) -> Option<State> {
        let mut c = stmt.walk();
        let parts: Vec<Node> = stmt
            .named_children(&mut c)
            .filter(|n| !matches!(n.kind(), "line_comment" | "block_comment"))
            .collect();
        let label = parts.first().filter(|l| l.kind() == "identifier").and_then(|l| self.text(*l));
        let Some(&body) = parts.get(1) else { return Some(state) };
        match body.kind() {
            "while_statement" | "do_statement" | "for_statement" | "enhanced_for_statement" | "switch_expression" => {
                self.pending_label = label;
                self.statement(body, Some(state))
            }
            _ => {
                self.targets.push(Target { label, breakable: false, is_loop: false, breaks: None, continues: None });
                let end = self.statement(body, Some(state));
                let target = self.targets.pop().expect("pushed above");
                let breaks = self.trim(target.breaks);
                join(end, breaks)
            }
        }
    }

    /// A `switch` statement, in either block shape. One with pattern or `null` labels must be
    /// exhaustive, and what that makes assigned after it is not followed here.
    fn switch_statement(&mut self, stmt: Node, state: State, label: Option<String>) -> Option<State> {
        let state = match stmt.child_by_field_name("condition") {
            Some(c) => self.expr(c, state),
            None => state,
        };
        let Some(body) = stmt.child_by_field_name("body") else { return Some(state) };
        let mut c = body.walk();
        let items: Vec<Node> = body
            .named_children(&mut c)
            .filter(|n| !matches!(n.kind(), "line_comment" | "block_comment"))
            .collect();
        let mut has_default = false;
        for item in &items {
            let mut ic = item.walk();
            for l in item.named_children(&mut ic).filter(|p| p.kind() == "switch_label") {
                let (is_default, enhanced) = crate::flow::returns::classify_label(l, self.bytes);
                if enhanced {
                    self.abandon(stmt);
                    return Some(state);
                }
                has_default |= is_default;
            }
        }
        self.open_scope();
        self.targets.push(Target { label, breakable: true, is_loop: false, breaks: None, continues: None });
        let mut group_end: Option<State> = None;
        let mut rule_ends: Option<State> = None;
        for item in items {
            let mut ic = item.walk();
            let parts: Vec<Node> = item
                .named_children(&mut ic)
                .filter(|p| p.kind() != "switch_label" && !matches!(p.kind(), "line_comment" | "block_comment"))
                .collect();
            match item.kind() {
                // Before a group: assigned after the selector AND after the group falling into it.
                "switch_block_statement_group" => {
                    let mut s = join(Some(state.clone()), group_end.take());
                    for p in parts {
                        s = self.statement(p, s);
                    }
                    group_end = s;
                }
                "switch_rule" => {
                    let end = match parts.first() {
                        Some(action) if action.kind() == "expression_statement" => {
                            Some(self.children(*action, state.clone()))
                        }
                        Some(action) => self.statement(*action, Some(state.clone())),
                        None => Some(state.clone()),
                    };
                    rule_ends = join(rule_ends, end);
                }
                _ => {}
            }
        }
        let target = self.targets.pop().expect("pushed above");
        let breaks = self.trim(target.breaks);
        let mut result = join(join(group_end, rule_ends), breaks);
        if !has_default {
            result = join(result, Some(state));
        }
        self.close_scope(result)
    }

    fn try_statement(&mut self, stmt: Node, state: State) -> Option<State> {
        let entry = state.clone();
        self.open_scope();
        let mut inner = state;
        if let Some(resources) = stmt.child_by_field_name("resources") {
            let mut c = resources.walk();
            let list: Vec<Node> = resources.named_children(&mut c).collect();
            for r in list {
                if r.kind() != "resource" {
                    inner = self.expr(r, inner);
                    continue;
                }
                if let Some(value) = r.child_by_field_name("value") {
                    inner = self.expr(value, inner);
                }
                if let Some(name) = r.child_by_field_name("name").and_then(|n| self.text(n)) {
                    self.declare(name, &mut inner, true, false);
                }
            }
        }
        let body = stmt.child_by_field_name("body");
        let try_end = match body {
            Some(b) => self.statement(b, Some(inner)),
            None => Some(inner),
        };
        let try_end = self.close_scope(try_end);

        // A `catch` can be entered from anywhere in the `try`: what was assigned before it is all
        // that is definite, and anything the block assigns may have been.
        let mut catch_entry = entry.clone();
        if let Some(b) = body {
            self.note_final_assignments(b, &mut catch_entry);
        }
        let mut joined = try_end;
        let mut finally = None;
        let mut c = stmt.walk();
        let clauses: Vec<Node> = stmt.named_children(&mut c).collect();
        let mut finally_entry = catch_entry.clone();
        for clause in clauses {
            match clause.kind() {
                "catch_clause" => {
                    self.open_scope();
                    let mut s = catch_entry.clone();
                    let param = crate::support::nodes::child_of_kind(clause, "catch_formal_parameter");
                    if let Some(name) = param.and_then(|p| p.child_by_field_name("name")).and_then(|n| self.text(n)) {
                        self.declare(name, &mut s, true, false);
                    }
                    let end = match clause.child_by_field_name("body") {
                        Some(b) => {
                            self.note_final_assignments(b, &mut finally_entry);
                            self.statement(b, Some(s))
                        }
                        None => Some(s),
                    };
                    let end = self.close_scope(end);
                    joined = join(joined, end);
                }
                "finally_clause" => finally = crate::support::nodes::child_of_kind(clause, "block"),
                _ => {}
            }
        }
        let Some(block) = finally else { return joined };
        let finally_end = self.statement(block, Some(finally_entry))?;
        let joined = joined?;
        // Assigned after the whole statement: by the `try` and every `catch`, OR by the `finally`.
        let unassigned = joined.unassigned.intersection(&finally_end.unassigned).cloned().collect();
        let mut assigned = joined.assigned;
        for (name, span) in finally_end.assigned {
            assigned.entry(name).or_insert(span);
        }
        Some(State { unassigned, assigned })
    }

    /// Mark every blank `final` `node` assigns as possibly assigned in `state`.
    fn note_final_assignments(&self, node: Node, state: &mut State) {
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if matches!(n.kind(), "lambda_expression" | "class_body") {
                continue;
            }
            let target = match n.kind() {
                "assignment_expression" => n.child_by_field_name("left"),
                "update_expression" => n.named_child(0),
                _ => None,
            };
            if let Some(name) = target.filter(|t| t.kind() == "identifier").and_then(|t| self.text(t)) {
                if self.blank_finals.contains(&name) && self.in_scope(&name) {
                    state.assigned.entry(name).or_insert((n.start_byte(), n.end_byte()));
                }
            }
            let mut c = n.walk();
            stack.extend(n.named_children(&mut c));
        }
    }

    fn jump(&mut self, stmt: Node, state: State, is_continue: bool) {
        let label = crate::support::nodes::child_of_kind(stmt, "identifier").and_then(|i| self.text(i));
        let index = self.targets.iter().rposition(|t| match &label {
            Some(l) => t.label.as_deref() == Some(l.as_str()),
            None if is_continue => t.is_loop,
            None => t.breakable,
        });
        let Some(i) = index else { return };
        let slot = match is_continue {
            true => &mut self.targets[i].continues,
            false => &mut self.targets[i].breaks,
        };
        *slot = join(slot.take(), Some(state));
    }

    /// A blank `final` the loop's body may assign, when it may not have been assigned on entering
    /// the loop: the next iteration assigns it again.
    fn loop_reassignment(&mut self, entry: &BTreeMap<String, (usize, usize)>, back: &Option<State>) {
        let Some(back) = back else { return };
        for (name, &(start, end)) in &back.assigned {
            if entry.contains_key(name)
                || !self.blank_finals.contains(name)
                || self.abandoned.contains(name)
                || !self.loop_reported.insert(start)
            {
                continue;
            }
            self.out.push(CheckId::FinalAssignment.span(
                start,
                end,
                format!("Variable `{name}` might be assigned in a loop"),
            ));
        }
    }

    // ── expressions ──────────────────────────────────────────────────────────

    /// A branching condition. A condition that may be a constant expression (`if (DEBUG)` over a
    /// `static final boolean`) changes which arm the JLS considers reachable, so its locals are
    /// abandoned before it is followed as an ordinary one.
    fn branch_condition(&mut self, stmt: Node, cond: Node, state: State) -> (Option<State>, Option<State>) {
        let inner = crate::flow::returns::unwrap_parens(cond);
        if !matches!(inner.kind(), "true" | "false") && !crate::flow::returns::never_constant_true(cond, self.bytes) {
            self.abandon(stmt);
        }
        self.condition(cond, state)
    }

    /// The states when a boolean expression is true and when it is false (JLS §16.1).
    fn condition(&mut self, n: Node, state: State) -> (Option<State>, Option<State>) {
        let operator = n.child_by_field_name("operator").and_then(|o| o.utf8_text(self.bytes).ok());
        match (n.kind(), operator) {
            ("parenthesized_expression", _) => match n.named_child(0) {
                Some(inner) => self.condition(inner, state),
                None => (Some(state.clone()), Some(state)),
            },
            ("true", _) => (Some(state), None),
            ("false", _) => (None, Some(state)),
            ("unary_expression", Some("!")) => match n.child_by_field_name("operand") {
                Some(operand) => {
                    let (t, f) = self.condition(operand, state);
                    (f, t)
                }
                None => (Some(state.clone()), Some(state)),
            },
            ("binary_expression", Some("&&")) => {
                let (Some(left), Some(right)) = (n.child_by_field_name("left"), n.child_by_field_name("right")) else {
                    return (Some(state.clone()), Some(state));
                };
                let (lt, lf) = self.condition(left, state);
                let (rt, rf) = match lt {
                    Some(lt) => self.condition(right, lt),
                    None => (None, None),
                };
                (rt, join(lf, rf))
            }
            ("binary_expression", Some("||")) => {
                let (Some(left), Some(right)) = (n.child_by_field_name("left"), n.child_by_field_name("right")) else {
                    return (Some(state.clone()), Some(state));
                };
                let (lt, lf) = self.condition(left, state);
                let (rt, rf) = match lf {
                    Some(lf) => self.condition(right, lf),
                    None => (None, None),
                };
                (join(lt, rt), rf)
            }
            _ => {
                let s = self.expr(n, state);
                (Some(s.clone()), Some(s))
            }
        }
    }

    fn expr(&mut self, n: Node, state: State) -> State {
        let operator = n.child_by_field_name("operator").and_then(|o| o.utf8_text(self.bytes).ok());
        match (n.kind(), operator) {
            ("identifier", _) => {
                let mut state = state;
                self.read(n, &mut state);
                state
            }
            ("assignment_expression", _) => self.assignment(n, state),
            ("update_expression", _) => match n.named_child(0) {
                Some(id) if id.kind() == "identifier" => {
                    let mut state = state;
                    self.read(id, &mut state);
                    self.write(id, &mut state);
                    state
                }
                _ => self.children(n, state),
            },
            ("binary_expression", Some("&&" | "||")) | ("unary_expression", Some("!")) => {
                let (t, f) = self.condition(n, state);
                join(t, f).unwrap_or_default()
            }
            ("ternary_expression", _) => {
                let (Some(cond), Some(yes), Some(no)) = (
                    n.child_by_field_name("condition"),
                    n.child_by_field_name("consequence"),
                    n.child_by_field_name("alternative"),
                ) else {
                    return self.children(n, state);
                };
                let (t, f) = self.condition(cond, state);
                let yes = t.map(|t| self.expr(yes, t));
                let no = f.map(|f| self.expr(no, f));
                join(yes, no).unwrap_or_default()
            }
            // Bodies that run later, whenever someone calls them.
            ("lambda_expression" | "method_reference" | "class_body", _) => state,
            ("switch_expression", _) => {
                self.abandon(n);
                state
            }
            _ => self.children(n, state),
        }
    }

    /// Every named child of `n`, in order.
    fn children(&mut self, n: Node, mut state: State) -> State {
        let mut c = n.walk();
        let children: Vec<Node> = n.named_children(&mut c).collect();
        for child in children {
            state = self.expr(child, state);
        }
        state
    }

    fn assignment(&mut self, n: Node, state: State) -> State {
        let (Some(left), Some(right)) = (n.child_by_field_name("left"), n.child_by_field_name("right")) else {
            return self.children(n, state);
        };
        if left.kind() != "identifier" {
            // `a[i] = …` reads `a` and `i`; `this.x = …` names a field.
            let state = self.expr(left, state);
            return self.expr(right, state);
        }
        let mut state = state;
        if !is_plain_assignment(n, self.bytes) {
            self.read(left, &mut state);
        }
        let mut state = self.expr(right, state);
        self.write(left, &mut state);
        state
    }

    fn read(&mut self, id: Node, state: &mut State) {
        let Some(name) = self.text(id) else { return };
        if !state.unassigned.contains(&name) || self.abandoned.contains(&name) || !self.in_scope(&name) {
            return;
        }
        if !is_read_position(id) {
            return;
        }
        if self.reported.insert(name.clone()) {
            self.out.push(CheckId::DefiniteAssignment.at(
                id,
                format!("Variable `{name}` might not have been initialized"),
            ));
        }
        state.unassigned.remove(&name);
    }

    fn write(&mut self, id: Node, state: &mut State) {
        let Some(name) = self.text(id) else { return };
        if !self.in_scope(&name) {
            return; // a field, not one of ours
        }
        if self.blank_finals.contains(&name) && !self.abandoned.contains(&name) {
            if state.assigned.contains_key(&name) {
                self.out.push(CheckId::FinalAssignment.at(
                    id,
                    format!("Variable `{name}` might already have been assigned"),
                ));
            } else {
                state.assigned.insert(name.clone(), (id.start_byte(), id.end_byte()));
            }
        }
        state.unassigned.remove(&name);
    }
}

/// Whether an identifier naming an in-scope local READS it: any value slot, and the head of a
/// member access — a local obscures a type of the same name, so `x.foo()` reads `x`.
fn is_read_position(id: Node) -> bool {
    if crate::support::scopes::is_value_position(id) {
        return true;
    }
    let Some(parent) = id.parent() else { return false };
    matches!(parent.kind(), "field_access" | "method_invocation")
        && parent.child_by_field_name("object").is_some_and(|o| o.id() == id.id())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diags(body: &str) -> Vec<String> {
        let src = format!("class C {{\n    int count;\n    void m(boolean c, int n) {{\n{body}\n    }}\n}}\n");
        let tree = bennu_java::prelude::parse_java(&src).expect("parse");
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        definite_assignment_errors_nodes(&nodes, &src)
            .into_iter()
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect()
    }

    fn flagged(body: &str, name: &str) {
        let d = diags(body);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains(&format!("`{name}`")), "{d:?}");
    }

    fn clean(body: &str) {
        let d = diags(body);
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn reading_an_uninitialized_local_is_flagged() {
        flagged("int x; int y = x + 1;", "x");
        clean("int x; x = 1; int y = x + 1;");
        clean("int x = 0; int y = x + 1;");
    }

    #[test]
    fn an_assignment_inside_the_reading_expression_is_ok() {
        // Commons Collections' `DefaultedMap.get`: the write happens before the read in one statement.
        clean("String v; String r = (v = f()) != null ? v : \"\";");
    }

    #[test]
    fn one_report_per_local_however_many_reads() {
        assert_eq!(diags("int x; int y = x; int z = x;").len(), 1);
    }

    #[test]
    fn a_local_assigned_on_one_branch_only_is_flagged() {
        flagged("int x; if (c) { x = 1; } use(x);", "x");
        clean("int x; if (c) { x = 1; } else { x = 2; } use(x);");
        clean("int x; if (c) { x = 1; } else { return; } use(x);");
        clean("int x; if (!c) { throw new IllegalStateException(); } else { x = 1; } use(x);");
    }

    #[test]
    fn a_condition_that_may_be_a_constant_is_not_followed() {
        // `DEBUG` may be a `static final boolean true`, and then the else arm does not count.
        clean("int x; if (DEBUG) { x = 1; } use(x);");
    }

    #[test]
    fn short_circuit_operators_follow_the_jls() {
        clean("int x; if (c && (x = n) > 0) { use(x); }");
        flagged("int x; if (c || (x = n) > 0) { use(x); }", "x");
        clean("int x; if (!c || (x = n) > 0) { return; } use(x);");
    }

    #[test]
    fn a_local_assigned_only_inside_the_try_is_flagged_after_a_catch_that_completes() {
        flagged("int x; try { x = f(); } catch (RuntimeException e) { g(); } use(x);", "x");
        clean("int x; try { x = f(); } catch (RuntimeException e) { x = 0; } use(x);");
        clean("int x; try { x = f(); } catch (RuntimeException e) { return; } use(x);");
        clean("int x; try { f(); } finally { x = 1; } use(x);");
    }

    #[test]
    fn a_loop_body_assigns_nothing_after_the_loop() {
        flagged("int x; while (n > 0) { x = 1; n--; } use(x);", "x");
        clean("int x; while (true) { x = 1; break; } use(x);");
        clean("int x; for (;;) { if (c) { x = 1; break; } } use(x);");
        clean("int x; do { x = 1; } while (c); use(x);");
    }

    #[test]
    fn a_switch_without_default_may_assign_nothing() {
        flagged("int x; switch (n) { case 1: x = 1; break; case 2: x = 2; break; } use(x);", "x");
        clean("int x; switch (n) { case 1: x = 1; break; default: x = 2; } use(x);");
        clean("int x; switch (n) { case 1 -> x = 1; default -> x = 2; } use(x);");
    }

    #[test]
    fn a_field_of_the_same_name_is_not_the_local() {
        clean("{ int count; } count++;");
    }

    #[test]
    fn a_lambda_body_is_not_entered() {
        clean("int x; Runnable r = () -> use(x); x = 1;");
    }

    #[test]
    fn a_blank_final_assigned_twice_is_flagged() {
        let d = diags("final int v; v = 1; v = 2;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].starts_with("final-assignment") && d[0].contains("already"), "{d:?}");
        assert!(diags("final int v; if (c) { v = 1; } else { v = 2; } use(v);").is_empty());
    }

    #[test]
    fn a_blank_final_assigned_in_a_loop_is_flagged() {
        let d = diags("final int v; while (c) { v = 1; }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("in a loop"), "{d:?}");
        assert!(diags("final int v; while (c) { v = 1; break; }").is_empty());
    }

    #[test]
    fn a_blank_final_assigned_in_try_and_catch_is_flagged() {
        let d = diags("final int v; try { v = f(); } catch (RuntimeException e) { v = 0; }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("already"), "{d:?}");
    }

    #[test]
    fn a_labeled_break_carries_its_state_to_the_label() {
        clean("int x; outer: for (int i = 0; i < n; i++) { for (;;) { x = 1; break outer; } } ");
        flagged("int x; outer: for (int i = 0; i < n; i++) { for (;;) { x = 1; break outer; } } use(x);", "x");
    }
}
