//! Watches on a native session: what gets walked, what gets asked, and what gets refused.
//!
//! One text box, three different things behind it, and the whole point of this module is that the
//! user can tell which one answered.
//!
//! ## 1. A path is walked here, not evaluated anywhere
//!
//! `order.customer.name`, `items[2]`, `*head` — [`crate::debug_path`] recognises those, and they are
//! resolved by reading the **variables tree** the panel is already showing: the frame's scopes, then
//! one `variables` request per segment. No expression evaluator is involved on either side of the
//! seam. Three reasons that is worth doing rather than forwarding the string:
//!
//! * **It means the same thing on all three adapters.** CodeLLDB's simple-expression language, LLDB's
//!   C++ parser and GDB's Rust parser disagree about `v[0]` on a `Vec`; the variables tree does not.
//! * **It reads runtime structure.** The tree knows which *variant* an enum is actually holding,
//!   which no static type can tell you.
//! * **It agrees with what is on screen.** A path is walked through the same synthetic children the
//!   formatters produce, so `v[0]` in the watch box and `[0]` under `v` in the tree are the same row.
//!   An evaluator that reached past the formatters would show a different value for the same words.
//!
//! Same semantics as the Java watch, which is the consistency the panel implicitly promises.
//!
//! ## 2. Anything else goes to the adapter, in a named dialect
//!
//! CodeLLDB has three evaluators and a prefix to pick one (`/se`, `/nat`, `/py` — see
//! [`bennu_dap::prelude::Evaluator`]); the other two adapters have one. A prefix the adapter does not
//! have is refused **by name** rather than sent, because sending it produces a syntax error about a
//! stray `/` that tells the user nothing.
//!
//! ## 3. What Rust cannot evaluate at all is said, not forwarded
//!
//! Ask LLDB about `v.len()` and it answers *no member named 'len' in 'alloc::vec::Vec<i32>'* — C++
//! prose about a Rust type, which reads as a broken debugger rather than as an unsupported operation.
//! The real reason is worth one sentence and is not obvious: the debugger can only call a function
//! that is **in the binary**, and monomorphisation means a generic function nobody called was never
//! compiled. No debugger fixes that; only a compiler in the loop would. So the shapes that can never
//! work — method calls, macros, turbofish, `?`, `.await`, closures — are recognised and explained,
//! and the adapter's own complaint is kept only for expressions where it might actually be the
//! answer.

use bennu_dap::prelude::{evaluators, split_dialect, AdapterSpec, Evaluator, Variable};
use bennu_proto::prelude::DebugValue;

use crate::debug_dap::{value_dto, DapSession};
use crate::debug_path::{self, Step, RUST};

/// How many children to fetch when following one step of a path.
///
/// Smaller than the panel's page: a path step needs the *one* child it names, and the fetch is only
/// wide because an adapter has no "give me the child called `x`" request.
const STEP_WIDTH: u32 = 500;

/// How many sibling names to list back when a segment does not resolve. Enough to spot a typo,
/// short enough to read in a one-line error.
const MAX_SUGGESTIONS: usize = 8;

// ── routing ─────────────────────────────────────────────────────────────────────

/// What to do with what the user typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Route {
    /// A path: walk the variables tree.
    Walk(Vec<Step>),
    /// Not a path: hand this text to the adapter's evaluator.
    Ask {
        /// Exactly what to send — including the dialect prefix when the adapter is the one that
        /// parses it, and without it when the prefix was only meant for us.
        text: String,
        dialect: Evaluator,
    },
}

/// Decide, without talking to anything.
///
/// Pure on purpose: this is the part with the branches, so it is the part worth pinning in tests. The
/// walk that follows needs a stopped program and cannot be.
pub(crate) fn route(expression: &str, spec: &AdapterSpec) -> Result<Route, String> {
    let (dialect, body) = split_dialect(expression);

    if let Some(dialect) = dialect {
        if !evaluators(spec).contains(&dialect) {
            return Err(format!(
                "`{}` is CodeLLDB's dialect — {} has only its own expression parser. Drop the \
                 prefix to use it, or install CodeLLDB.",
                dialect.prefix(),
                spec.label
            ));
        }
        if body.is_empty() {
            return Err(format!("`{}` and then what?", dialect.prefix()));
        }
        // CodeLLDB parses the prefix itself, so it is left on. The other adapters have never heard of
        // it, and `/nat` in front of an expression they would otherwise understand is a syntax error
        // about a slash.
        let text = if spec.rust_formatters {
            format!("{} {body}", dialect.prefix())
        } else {
            body.to_string()
        };
        return Ok(Route::Ask { text, dialect });
    }

    if body.is_empty() {
        return Err("an empty watch".to_string());
    }
    if let Ok(steps) = debug_path::parse(body, RUST) {
        return Ok(Route::Walk(steps));
    }
    // Not a path. The adapter's default dialect: CodeLLDB's own reader, or the only one the others
    // have.
    let dialect = *evaluators(spec).first().unwrap_or(&Evaluator::Native);
    Ok(Route::Ask { text: body.to_string(), dialect })
}

// ── the entry point ─────────────────────────────────────────────────────────────

/// Evaluate a watch against a stopped frame.
pub(crate) fn watch(
    session: &DapSession,
    frame: usize,
    expression: &str,
) -> Result<DebugValue, String> {
    let spec = session.spec();
    match route(expression, spec)? {
        Route::Walk(steps) => {
            let value = walk(session, frame, &steps)?;
            Ok(DebugValue { name: expression.to_string(), ..value_dto(session, &value, "watch") })
        }
        Route::Ask { text, dialect } => {
            let frame_id = session.frame_id(frame).ok();
            // `watch` and not `repl`: an adapter is entitled to allow side effects in a repl
            // evaluation, and a watch is re-evaluated on every stop — it must not be able to move the
            // program it is describing.
            let body = session
                .with(|s| s.evaluate(&text, frame_id, "watch"))
                .map_err(|e| explain(&text, &e, spec, dialect))?;
            Ok(DebugValue {
                name: expression.to_string(),
                kind: "watch".to_string(),
                type_name: body.type_name.clone().unwrap_or_default(),
                value: body.result.clone(),
                object: session.handle(body.variables_reference),
            })
        }
    }
}

// ── the walk ────────────────────────────────────────────────────────────────────

fn walk(session: &DapSession, frame: usize, steps: &[Step]) -> Result<Variable, String> {
    let frame_id = session.frame_id(frame)?;
    let Some(Step::Field(name)) = steps.first() else {
        return Err("a watch starts with a variable name".to_string());
    };
    let mut current = root(session, frame_id, name)?;
    // What has been resolved so far, so an error names the segment that failed rather than the whole
    // expression: on `a.b.c.d` it matters a great deal which of the four is wrong.
    let mut so_far = name.clone();

    for step in &steps[1..] {
        current = follow(session, &current, step, &so_far)?;
        so_far = match step {
            Step::Field(name) => format!("{so_far}.{name}"),
            Step::Index(at) => format!("{so_far}[{at}]"),
            Step::Deref => format!("*{so_far}"),
        };
    }
    Ok(current)
}

/// The first segment: a variable of the frame.
///
/// No fallback to a field of `self`, deliberately — unlike the Java watch, where a bare field name is
/// how you refer to a field inside an instance method. Rust has no implicit receiver, so `count` and
/// `self.count` are different things in the source file and a watch that conflated them would be
/// lying about which one it read.
fn root(session: &DapSession, frame_id: i64, name: &str) -> Result<Variable, String> {
    let scopes = session.with(|s| s.scopes(frame_id))?;
    let mut seen: Vec<String> = Vec::new();
    for scope in &scopes {
        // Registers, usually. Nothing a Rust watch is written against, and fetching it costs a round
        // trip per scope on every re-evaluation.
        if scope.expensive {
            continue;
        }
        let variables = session.with(|s| s.variables(scope.variables_reference, Some(STEP_WIDTH)))?;
        if let Some(hit) = variables.iter().find(|v| v.name == name) {
            return Ok(hit.clone());
        }
        seen.extend(variables.iter().map(|v| v.name.clone()));
    }
    Err(format!("no variable named `{name}` in this frame{}", suggestions(&seen)))
}

/// One hop.
fn follow(
    session: &DapSession,
    value: &Variable,
    step: &Step,
    so_far: &str,
) -> Result<Variable, String> {
    if value.variables_reference == 0 {
        let what = value.type_name.clone().unwrap_or_else(|| "a value".to_string());
        return Err(format!("`{so_far}` is {what} — there is nothing inside it"));
    }

    match step {
        Step::Field(name) => {
            let children =
                session.with(|s| s.variables(value.variables_reference, Some(STEP_WIDTH)))?;
            if let Some(hit) = children.iter().find(|c| &c.name == name) {
                return Ok(hit.clone());
            }
            let names: Vec<String> = children.iter().map(|c| c.name.clone()).collect();
            Err(format!("`{so_far}` has no field named `{name}`{}", suggestions(&names)))
        }
        Step::Index(at) => {
            if *at < 0 {
                return Err(format!("`{at}` is not an index"));
            }
            let at = *at as u32;
            match value.indexed_variables.unwrap_or(0) {
                // A real container, and the adapter counted it. Fetch **only** that element: the
                // plain children call is capped, and `v[400000]` on a Vec of a million is a fair
                // question with an exact answer.
                length if length > 0 => {
                    if at >= length {
                        return Err(format!(
                            "index {at} is outside `{so_far}`, which holds {length}"
                        ));
                    }
                    session
                        .with(|s| s.indexed_variable(value.variables_reference, at))?
                        .ok_or_else(|| format!("`{so_far}[{at}]` came back empty"))
                }
                // No indexed children: either a struct, or an adapter that did not count. Look for
                // the row by the name the tree gives it rather than by position, because taking the
                // n-th child of a struct would answer confidently about the wrong field.
                _ => {
                    let children =
                        session.with(|s| s.variables(value.variables_reference, Some(STEP_WIDTH)))?;
                    let wanted = format!("[{at}]");
                    children
                        .iter()
                        .find(|c| c.name == wanted)
                        .cloned()
                        .ok_or_else(|| format!("`{so_far}` has no `{wanted}` to read"))
                }
            }
        }
        Step::Deref => {
            let children =
                session.with(|s| s.variables(value.variables_reference, Some(STEP_WIDTH)))?;
            match children.len() {
                1 => Ok(children.into_iter().next().unwrap()),
                0 => Err(format!("`{so_far}` points at nothing the debugger can read")),
                // A reference whose fields the debugger already shows in place — which is the usual
                // case for `&Foo`. Said rather than silently ignored: dropping the `*` and answering
                // anyway would make `*x` and `x` the same watch, and then one of the two is a lie.
                n => Err(format!(
                    "`*` needs a single value to read — `{so_far}` already shows the {n} things it \
                     points at, so drop the `*`"
                )),
            }
        }
    }
}

/// ` — this one has `a`, `b`, `c``, or nothing when there is nothing to suggest.
fn suggestions(names: &[String]) -> String {
    let listed: Vec<String> =
        names.iter().filter(|n| !n.is_empty()).take(MAX_SUGGESTIONS).map(|n| format!("`{n}`")).collect();
    if listed.is_empty() {
        return String::new();
    }
    let more = if names.len() > listed.len() { ", …" } else { "" };
    format!(" — there is {}{more}", listed.join(", "))
}

// ── explaining a failure ────────────────────────────────────────────────────────

/// The shape of an expression that is not a path — what decides whether the adapter's complaint is
/// worth repeating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    /// `v.len()`, `foo(1)`.
    Call,
    /// `println!(…)`, `vec![…]`.
    Macro,
    /// `x.parse::<u32>()`, `Vec::<u8>::new`.
    Turbofish,
    /// `|x| x + 1`.
    Closure,
    /// A trailing `?`.
    Try,
    /// `.await`.
    Await,
    /// Arithmetic, a comparison, a cast, an address — things a native evaluator may well manage.
    Other,
}

/// Classify, from the text alone.
///
/// Deliberately syntactic and deliberately cheap: this is not a Rust parser, it is the difference
/// between "no debugger can do this, and here is why" and "the debugger said no, here is what it
/// said". Getting it wrong in the safe direction means forwarding the adapter's message, which is
/// where we started.
pub(crate) fn shape(expression: &str) -> Shape {
    let bytes = expression.as_bytes();

    if expression.contains("::<") {
        return Shape::Turbofish;
    }
    if expression.contains(".await") {
        return Shape::Await;
    }
    // A `!` that opens a delimiter is a macro. `a != b` is not, and neither is `!flag`.
    if bytes
        .windows(2)
        .any(|w| w[0] == b'!' && matches!(w[1], b'(' | b'[' | b'{'))
    {
        return Shape::Macro;
    }
    // A `(` with an identifier immediately in front of it is a call. `(a + b)` has a space or a
    // start-of-string there, and `f (x)` is not something anyone types into a watch box.
    let call = bytes.iter().enumerate().any(|(i, c)| {
        *c == b'(' && i > 0 && {
            let before = bytes[i - 1];
            before.is_ascii_alphanumeric() || before == b'_'
        }
    });
    if call {
        return Shape::Call;
    }
    if expression.trim_end().ends_with('?') {
        return Shape::Try;
    }
    // Two bars with something between them. Not `a || b`, which is adjacent bars.
    if let Some(first) = expression.find('|') {
        let rest = &expression[first + 1..];
        if !rest.starts_with('|') && rest.contains('|') {
            return Shape::Closure;
        }
    }
    Shape::Other
}

/// Turn a failed evaluation into something worth reading.
///
/// `dialect` is the evaluator that answered — it decides both whose accent the message is written in
/// and which *other* evaluators are worth offering.
pub(crate) fn explain(
    expression: &str,
    adapter_error: &str,
    spec: &AdapterSpec,
    dialect: Evaluator,
) -> String {
    let reason = match shape(expression) {
        // The one everybody hits, and the one whose real reason is genuinely surprising.
        Shape::Call => Some(
            "a Rust method or function call cannot be evaluated: the debugger can only call what is \
             actually in the binary, and a generic function nobody called was never compiled. \
             Expand the value in the tree instead — the fields are all there.",
        ),
        Shape::Macro => Some(
            "a macro is expanded by the compiler, so there is nothing by that name in the running \
             program.",
        ),
        Shape::Turbofish => Some(
            "generic arguments are resolved at compile time; the program holds the instantiations it \
             was compiled with and cannot be asked for another.",
        ),
        Shape::Closure => Some(
            "a closure would have to be compiled and then run inside the debuggee, which a watch \
             deliberately never does.",
        ),
        Shape::Try => Some("`?` returns from a function, and a watch has no function to return from."),
        Shape::Await => Some(
            "`.await` needs an executor to poll the future; a stopped program has none running.",
        ),
        // Arithmetic, a comparison, a cast — the adapter may genuinely be the authority here, so its
        // own words are kept.
        Shape::Other => None,
    };

    match reason {
        Some(reason) => format!("{reason}{}", dialect_hint(spec, dialect)),
        None => {
            format!("{}{}", rephrase(adapter_error, spec, dialect), dialect_hint(spec, dialect))
        }
    }
}

/// The adapter's message, with the C++ accent taken off it where we can recognise it.
///
/// LLDB evaluates Rust with a **C++** parser, so its complaints are about C++ concepts wearing Rust
/// type names — `no member named 'len' in 'alloc::vec::Vec<i32>'`. Forwarding that is what makes a
/// working debugger look broken, so where the shape is recognisable it is said in Rust's terms and the
/// original is kept as the second half rather than thrown away.
fn rephrase(adapter_error: &str, spec: &AdapterSpec, dialect: Evaluator) -> String {
    let trimmed = adapter_error.trim();
    if trimmed.is_empty() {
        return "the debugger could not evaluate that".to_string();
    }
    // Only the native parser is the C++ one. CodeLLDB's own reader has its own vocabulary, and
    // labelling its message as C++ would be a confident lie about where it came from.
    if dialect == Evaluator::Native && trimmed.contains("no member named") {
        return format!(
            "that is not a field of the value — the wording is {}'s C++ expression parser, which is \
             what it uses on Rust types too ({trimmed})",
            spec.label
        );
    }
    trimmed.to_string()
}

/// What else the user could try, when the adapter in use gives them a choice.
///
/// The ones that are **not** the evaluator that just failed — offering `/nat` to somebody who typed
/// `/nat` is noise, and after a `/nat` failure the interesting suggestions are the other two.
fn dialect_hint(spec: &AdapterSpec, current: Evaluator) -> String {
    let others: Vec<&str> = evaluators(spec)
        .iter()
        .filter(|e| **e != current)
        .map(|e| e.prefix())
        .collect();
    if others.is_empty() {
        return String::new();
    }
    format!(" Prefix with {} to hand it to another evaluator.", others.join(" or "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_dap::prelude::spec_by_id;

    fn codelldb() -> &'static AdapterSpec {
        spec_by_id("codelldb").unwrap()
    }
    fn lldb() -> &'static AdapterSpec {
        spec_by_id("lldb-dap").unwrap()
    }

    fn field(name: &str) -> Step {
        Step::Field(name.to_string())
    }

    /// The point of the whole module: a path never reaches an evaluator, so it means the same thing
    /// whichever adapter resolved.
    #[test]
    fn a_path_is_walked_on_every_adapter() {
        for spec in [codelldb(), lldb(), spec_by_id("gdb").unwrap()] {
            assert_eq!(
                route("order.customer.name", spec).unwrap(),
                Route::Walk(vec![field("order"), field("customer"), field("name")]),
                "{}",
                spec.label
            );
            assert_eq!(route("v[3]", spec).unwrap(), Route::Walk(vec![field("v"), Step::Index(3)]));
            assert_eq!(route("*head", spec).unwrap(), Route::Walk(vec![field("head"), Step::Deref]));
        }
    }

    #[test]
    fn anything_else_goes_to_the_adapters_own_default_dialect() {
        // CodeLLDB's default is its simple reader, not the native parser — it follows the formatters
        // and runs nothing in the debuggee, which is what a watch wants.
        assert_eq!(
            route("a + b", codelldb()).unwrap(),
            Route::Ask { text: "a + b".into(), dialect: Evaluator::Simple }
        );
        assert_eq!(
            route("a + b", lldb()).unwrap(),
            Route::Ask { text: "a + b".into(), dialect: Evaluator::Native }
        );
    }

    /// CodeLLDB parses the prefix; the others have never heard of it and would report a syntax error
    /// about a slash.
    #[test]
    fn a_prefix_is_kept_for_the_adapter_that_parses_it_and_stripped_for_the_one_that_does_not() {
        assert_eq!(
            route("/nat v.size()", codelldb()).unwrap(),
            Route::Ask { text: "/nat v.size()".into(), dialect: Evaluator::Native }
        );
        assert_eq!(
            route("/nat v.size()", lldb()).unwrap(),
            Route::Ask { text: "v.size()".into(), dialect: Evaluator::Native }
        );
    }

    /// Refused by name, which is the whole tier-one idea: a dialect the adapter does not have is a
    /// fact we know, and sending it produces a message about a stray slash instead.
    #[test]
    fn a_dialect_the_adapter_does_not_have_is_refused_by_name() {
        let err = route("/py len($v)", lldb()).unwrap_err();
        assert!(err.contains("/py"), "{err}");
        assert!(err.contains("CodeLLDB"), "and says where it comes from: {err}");
        assert!(route("/py len($v)", codelldb()).is_ok());
    }

    #[test]
    fn a_bare_prefix_and_an_empty_watch_are_both_said_plainly() {
        assert!(route("/nat", codelldb()).unwrap_err().contains("and then what"));
        assert!(route("", lldb()).unwrap_err().contains("empty"));
        assert!(route("   ", lldb()).unwrap_err().contains("empty"));
    }

    /// An expression that merely begins with a slash is not a dialect selector — and `/self.len` is
    /// not a path either, so it goes to the evaluator whole.
    #[test]
    fn a_slash_that_is_not_a_dialect_is_left_alone() {
        assert_eq!(
            route("/self.len", lldb()).unwrap(),
            Route::Ask { text: "/self.len".into(), dialect: Evaluator::Native }
        );
    }

    #[test]
    fn the_shapes_that_can_never_work_are_recognised() {
        assert_eq!(shape("v.len()"), Shape::Call);
        assert_eq!(shape("foo(1, 2)"), Shape::Call);
        assert_eq!(shape("v.iter().count()"), Shape::Call);
        assert_eq!(shape("vec![1, 2]"), Shape::Macro);
        assert_eq!(shape("format!(\"{}\", x)"), Shape::Macro);
        assert_eq!(shape("x.parse::<u32>()"), Shape::Turbofish);
        assert_eq!(shape("f.await"), Shape::Await);
        assert_eq!(shape("x.get(0)?"), Shape::Call, "the call is the first thing that fails");
        assert_eq!(shape("value?"), Shape::Try);
        assert_eq!(shape("|x| x + 1"), Shape::Closure);
    }

    /// The near-misses. Each of these WOULD be misclassified by the obvious one-character check, and
    /// each is something a native evaluator can genuinely answer — so it must reach it.
    #[test]
    fn arithmetic_and_comparisons_are_not_mistaken_for_calls_or_macros() {
        assert_eq!(shape("a != b"), Shape::Other, "`!` before `=` is not a macro");
        assert_eq!(shape("!flag"), Shape::Other);
        assert_eq!(shape("a || b"), Shape::Other, "adjacent bars are an or, not a closure");
        assert_eq!(shape("(a + b) * c"), Shape::Other, "a `(` after a space is not a call");
        assert_eq!(shape("x as u64"), Shape::Other);
        assert_eq!(shape("&v"), Shape::Other);
        assert_eq!(shape("a + b"), Shape::Other);
    }

    /// The failure that produced this module: LLDB's answer is C++ prose about a Rust type, and
    /// repeating it reads as a broken debugger.
    #[test]
    fn a_method_call_is_explained_rather_than_forwarded() {
        let text = explain(
            "v.len()",
            "error: no member named 'len' in 'alloc::vec::Vec<int>'",
            lldb(),
            Evaluator::Native,
        );
        assert!(!text.contains("no member named"), "the C++ prose is replaced: {text}");
        // The reason, which is the part nobody guesses: it is not in the binary.
        assert!(text.contains("in the binary"), "{text}");
        // …and what to do instead.
        assert!(text.contains("Expand"), "{text}");
        // lldb-dap has one evaluator, so it is not offered a choice it does not have.
        assert!(!text.contains("Prefix with"), "{text}");
    }

    #[test]
    fn codelldb_is_offered_its_other_evaluators_and_the_others_are_not() {
        // Answered by CodeLLDB's default reader, so the other two are what is left to try…
        let text = explain("v.len()", "nope", codelldb(), Evaluator::Simple);
        assert!(text.contains("/nat"), "{text}");
        assert!(text.contains("/py"), "{text}");
        assert!(!text.contains("/se"), "not the one that just answered: {text}");
        // …and after a `/nat` failure it is the other two again, not `/nat` itself.
        let native = explain("v.len()", "nope", codelldb(), Evaluator::Native);
        assert!(!native.contains("/nat"), "{native}");
        assert!(native.contains("/se") && native.contains("/py"), "{native}");
        // An adapter with one evaluator is not offered a choice it does not have.
        assert!(!explain("v.len()", "nope", lldb(), Evaluator::Native).contains("Prefix with"));
    }

    /// An expression the adapter is genuinely the authority on keeps the adapter's words.
    #[test]
    fn an_ordinary_failure_keeps_the_adapters_own_message() {
        let text = explain("a + b", "error: use of undeclared identifier 'b'", lldb(), Evaluator::Native);
        assert!(text.contains("undeclared identifier 'b'"), "{text}");
    }

    /// …but the C++ accent is named where it shows through, instead of being passed off as Rust.
    #[test]
    fn a_cxx_field_complaint_is_labelled_as_one() {
        let error = "no member named 'b' in 'geode::Order'";
        let text = explain("(a).b", error, lldb(), Evaluator::Native);
        assert!(text.contains("C++"), "{text}");
        assert!(text.contains("no member named 'b'"), "the original is kept, not lost: {text}");
        // Only the native parser is the C++ one. Labelling CodeLLDB's own reader as C++ would be a
        // confident lie about where the message came from.
        let simple = explain("(a).b", error, codelldb(), Evaluator::Simple);
        assert!(!simple.contains("C++"), "{simple}");
        assert!(simple.contains("no member named 'b'"), "{simple}");
    }

    #[test]
    fn an_empty_adapter_error_still_says_something() {
        assert!(explain("a + b", "   ", lldb(), Evaluator::Native).contains("could not evaluate"));
    }

    #[test]
    fn the_suggestions_are_bounded_and_say_when_they_are_cut() {
        assert_eq!(suggestions(&[]), "");
        let one = suggestions(&["order".to_string()]);
        assert!(one.contains("`order`"));
        assert!(!one.contains('…'));
        let many: Vec<String> = (0..30).map(|i| format!("v{i}")).collect();
        let text = suggestions(&many);
        assert!(text.contains('…'), "cut, and says so: {text}");
        assert_eq!(text.matches('`').count() / 2, MAX_SUGGESTIONS);
    }
}
