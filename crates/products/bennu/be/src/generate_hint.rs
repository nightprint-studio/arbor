//! `generate hint` domain — `bennu_generate_hint` (the ghost text for what is certainly being written).
//!
//! Two proposals share the one ghost-text seam, asked in order:
//!
//! 1. **A member being written.** Typing `getCust` in a class body has, most of the time, exactly one
//!    thing it can mean, and the editor already knows what: the field is in the buffer, the
//!    accessor's name is Java's own convention, and its body is the only body it could have.
//! 2. **A declaration's name.** After `private final OrderRepository ` the next word is a name, and
//!    the one IntelliJ would propose is `orderRepository` (see `bennu_java::declaration_name`) —
//!    spelled the project's way: the convention its naming rules declare for fields, locals or
//!    parameters, and where they declare none, the one the file's own names already follow
//!    (`order_repository` among `identity_resolver`s).
//!
//! **Never a guess.** Ghost text sits inline, where it reads like text that is already there —
//! being wrong there costs trust rather than a keystroke. So the accessor answers only when exactly
//! one matches (the same rule `bennu-complete`'s `unique_continuation` holds every other ghost
//! proposal in Arbor to), and the name only where the next word can be nothing but a declaration's
//! name.
//!
//! Both read the buffer, not the diagnostics — which is the point. A half-typed member is a syntax
//! error, so validation stops for the whole file (`bennu-check` returns the syntax error and nothing
//! else, deliberately: a tree nobody believes produces a page of errors about code that compiles).
//! The offer has to survive being typed, so it comes from the source.
//!
//! The project's resolver is consulted for one thing only: telling a method the class does not
//! declare from one it INHERITS. Without it that family is skipped rather than guessed at.

use bennu_core::prelude::BennuState;
use bennu_intel::prelude::AccessorHint;
use bennu_java::prelude::{DeclarationKind, NameContext};
use bennu_naming::prelude::{Convention, Target};
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_generate_hint`].
#[derive(Deserialize)]
pub struct GenerateHintArgs {
    /// Absolute path (forward slashes) of the buffer. Only its extension is consulted — the
    /// answer comes from `source`, not from disk.
    pub file: String,
    /// The live buffer.
    pub source: String,
    /// UTF-8 byte offset of the caret.
    pub offset: usize,
    /// The editor's match-case setting, as everywhere else completion is asked.
    #[serde(default)]
    pub case_sensitive: bool,
}

/// What is certainly being written at the caret, or `None` — which is the ordinary answer.
#[arbor_rpc::handler]
fn bennu_generate_hint(
    _ctx: &BennuState,
    args: GenerateHintArgs,
) -> Result<Option<AccessorHint>, String> {
    if !args.file.to_ascii_lowercase().ends_with(".java") {
        return Ok(None);
    }
    let resolver = IndexService::global().caret_resolver_for(&args.file);
    let accessor = bennu_intel::prelude::generated_hint(
        &args.source,
        args.offset,
        bennu_complete::prelude::MatchCase::from_flag(args.case_sensitive),
        resolver.as_deref().map(|r| r as &dyn bennu_java::prelude::TypeResolver),
    );
    Ok(accessor.or_else(|| {
        declaration_name_hint(&args.source, args.offset, args.case_sensitive, |target| {
            crate::naming::declared_convention(&args.file, target)
        })
    }))
}

/// The predicted name of the declaration at the caret, shaped as ghost text.
///
/// Drawn as the rest of the name when what was typed is its exact start — it reads as the word being
/// finished — and as `→ name` otherwise, since accepting then rewrites the typed part (`myms` becomes
/// `myMsRestClientApi`), and a continuation drawn after it would spell a word that is not the result.
///
/// `declared` is the convention the project's naming rules state for a target, if any.
fn declaration_name_hint(
    source: &str,
    offset: usize,
    case_sensitive: bool,
    declared: impl Fn(Target) -> Option<Convention>,
) -> Option<AccessorHint> {
    let found = bennu_java::prelude::declaration_name_at(source, offset, case_sensitive, |name, context| {
        spelled(name, context, declared(target_of(context.kind)))
    })?;
    let typed = &source[found.typed_start..found.typed_end];
    let preview = match found.name.strip_prefix(typed) {
        Some(rest) => rest.to_string(),
        None => format!(" → {}", found.name),
    };
    Some(AccessorHint {
        preview,
        insert: found.name,
        replace_start: found.typed_start,
        replace_end: found.typed_end,
    })
}

/// `name` (camelCase, as predicted) in the convention the project declared, or else in the one the
/// file's names already follow; left as it is when neither says.
fn spelled(name: String, context: &NameContext<'_>, declared: Option<Convention>) -> String {
    Convention::for_variables(declared, context.declared_in_file.iter().copied())
        .and_then(|convention| convention.render(&name))
        .unwrap_or(name)
}

fn target_of(kind: DeclarationKind) -> Target {
    match kind {
        DeclarationKind::Field => Target::Field,
        DeclarationKind::Local => Target::Local,
        DeclarationKind::Parameter => Target::Parameter,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project that declared no naming rule.
    fn undeclared(_: Target) -> Option<Convention> {
        None
    }

    fn name_in(src: &str, declared: impl Fn(Target) -> Option<Convention>) -> String {
        declaration_name_hint(src, src.len(), false, declared).expect("a hint").insert
    }

    #[test]
    fn a_name_with_nothing_typed_is_drawn_whole_and_inserted_at_the_caret() {
        let src = "class A { private final MyMsRestClientApi ";
        let hint = declaration_name_hint(src, src.len(), false, undeclared).expect("a hint");
        assert_eq!(hint.preview, "myMsRestClientApi");
        assert_eq!(hint.insert, "myMsRestClientApi");
        assert_eq!((hint.replace_start, hint.replace_end), (src.len(), src.len()));
    }

    #[test]
    fn a_name_whose_start_is_typed_is_drawn_as_its_rest_and_replaces_the_typed_part() {
        let src = "class A { private final MyMsRestClientApi my";
        let hint = declaration_name_hint(src, src.len(), false, undeclared).expect("a hint");
        assert_eq!(hint.preview, "MsRestClientApi");
        assert_eq!(hint.insert, "myMsRestClientApi");
        assert_eq!((hint.replace_start, hint.replace_end), (src.len() - 2, src.len()));
    }

    /// Typed in another case, the rest alone would read `mymsRestClientApi` — not what Tab writes.
    #[test]
    fn a_name_typed_in_another_case_is_drawn_as_the_rewrite() {
        let src = "class A { private final MyMsRestClientApi myms";
        let hint = declaration_name_hint(src, src.len(), false, undeclared).expect("a hint");
        assert_eq!(hint.preview, " → myMsRestClientApi");
    }

    #[test]
    fn no_hint_where_no_declaration_is_being_named() {
        let src = "class A { void m() { return Order ";
        assert!(declaration_name_hint(src, src.len(), false, undeclared).is_none());
    }

    /// No rule declared: the file's own names decide.
    #[test]
    fn a_file_written_in_snake_case_gets_a_snake_case_name() {
        let src = "class A {\n    private final IdentityResolver identity_resolver;\n    private final FilterConfigurator ";
        assert_eq!(name_in(src, undeclared), "filter_configurator");
        let local = "class A { void m(String first_name) { URLBuilder ";
        assert_eq!(name_in(local, undeclared), "url_builder");
    }

    #[test]
    fn a_camel_case_file_or_one_that_says_nothing_keeps_camel_case() {
        assert_eq!(name_in("class A { IdentityResolver identityResolver; FilterConfigurator ", undeclared), "filterConfigurator");
        assert_eq!(name_in("class A { Order order; FilterConfigurator ", undeclared), "filterConfigurator");
    }

    /// A declared rule is the project's decision, and outweighs what a file happens to contain.
    #[test]
    fn a_declared_convention_wins_over_the_files_names() {
        let src = "class A { IdentityResolver identity_resolver; FilterConfigurator ";
        assert_eq!(name_in(src, |_| Some(Convention::Camel)), "filterConfigurator");
    }

    #[test]
    fn the_declared_convention_is_the_one_for_the_kind_of_declaration() {
        let parameters_snake = |target: Target| (target == Target::Parameter).then_some(Convention::LowerSnake);
        assert_eq!(name_in("class A { void m(FilterConfigurator ", parameters_snake), "filter_configurator");
        assert_eq!(name_in("class A { FilterConfigurator ", parameters_snake), "filterConfigurator");
    }

    /// A snake-case name that is already taken gets its digit in the same spelling.
    #[test]
    fn a_taken_snake_case_name_gets_a_digit() {
        let src = "class A { Order first_order; FilterConfigurator filter_configurator; FilterConfigurator ";
        assert_eq!(name_in(src, undeclared), "filter_configurator1");
    }
}
