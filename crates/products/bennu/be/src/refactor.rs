//! `refactor` domain — the Java refactorings the editor offers at a caret or over a selection.
//!
//! ## What is here and what is not
//!
//! The transforms themselves are in the pure `bennu-refactor` crate, where they are unit-tested
//! against real Java. This module is the two things that crate deliberately cannot do:
//!
//! 1. **Naming a type.** An *extract variable* has to write a declaration, and `var` is not an
//!    answer on the Java 8 codebases this editor exists for. The plan comes back with a
//!    [`TypeSlot`] naming the span it needs typed; the project's resolver answers it, and the
//!    import it needs is added in the same plan.
//! 2. **Refusing when nothing can be typed.** A project that is still indexing cannot answer, and a
//!    refactoring that writes `var` because the index was cold is a refactoring that changes how a
//!    codebase is written depending on when you invoked it.
//!
//! Renaming is not here either, and for a different reason: it is a *project* question — every
//! reference, every Spring bean, every XML config — and it has its own domain with the reference
//! index behind it.
//!
//! [`TypeSlot`]: bennu_refactor::prelude::TypeSlot

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::UsageHit;
use bennu_refactor::prelude::{
    missing_type_at, new_type_source, plan_for, refactorings_at, Plan, RefactorEdit,
    TYPE_PLACEHOLDER,
};
use serde::{Deserialize, Serialize};

/// Args for [`bennu_refactorings`] and [`bennu_refactor_plan`].
#[derive(Deserialize)]
pub struct RefactorArgs {
    /// Absolute path of the file the caret is in.
    pub file: String,
    /// The current buffer — unsaved, which is the only text a refactoring may be computed against.
    pub source: String,
    /// Selection start, or the caret when there is no selection.
    pub start: usize,
    /// Selection end. Equal to `start` for a caret.
    pub end: usize,
    /// Which refactoring to plan. Empty on [`bennu_refactorings`], which asks for all of them.
    #[serde(default)]
    pub id: String,
}

/// One row of the Alt+Enter list.
#[derive(Serialize)]
pub struct RefactorOffer {
    /// Stable id — sent back to [`bennu_refactor_plan`] when the row is chosen.
    pub id: String,
    pub label: String,
    /// Why it cannot be done here, empty when it can. A row with a reason is shown greyed rather
    /// than hidden: "cannot extract: the selection produces `total` and `count`" tells the user
    /// what to change, an absent row teaches nothing.
    pub reason: String,
    /// The name it would introduce, when it introduces one.
    pub name: String,
}

/// What can be refactored at the caret or over the selection.
///
/// Never errors: a file that is not Java, a buffer that does not parse, a caret in a comment — all
/// of them are an empty list, which is the answer.
#[arbor_rpc::handler]
pub(crate) fn bennu_refactorings(
    _ctx: &BennuState,
    args: RefactorArgs,
) -> Result<Vec<RefactorOffer>, String> {
    if !is_java(&args.file) {
        return Ok(Vec::new());
    }
    Ok(refactorings_at(&args.source, args.start, args.end)
        .into_iter()
        .map(|outcome| match outcome {
            Ok(plan) => RefactorOffer {
                id: plan.id,
                label: plan.label,
                reason: String::new(),
                name: plan.name.unwrap_or_default(),
            },
            Err(refusal) => RefactorOffer {
                id: refusal.id,
                label: refusal.label,
                reason: refusal.reason,
                name: String::new(),
            },
        })
        .collect())
}

/// The edits for one refactoring, with every type it needs resolved.
///
/// Computed against the buffer as it is **now** rather than reused from the offer list: the two
/// calls are a menu opening and a row being chosen, and a keystroke can happen in between.
#[arbor_rpc::handler]
pub(crate) fn bennu_refactor_plan(
    _ctx: &BennuState,
    args: RefactorArgs,
) -> Result<RefactorPlanDto, String> {
    if !is_java(&args.file) {
        return Err("refactorings are only offered for Java files".to_string());
    }
    let outcome = plan_for(&args.id, &args.source, args.start, args.end)
        .ok_or_else(|| format!("`{}` no longer applies here", args.id))?;
    let mut plan = outcome.map_err(|refusal| refusal.reason)?;
    let mut imports: Vec<String> = Vec::new();

    if let Some(slot) = plan.type_slot.clone() {
        use bennu_intel::prelude::Declarable;
        use bennu_refactor::prelude::TypeNeed;
        match crate::index_service::IndexService::global().infer_type_detail(
            &args.file,
            &args.source,
            slot.start,
            slot.end,
        ) {
            Declarable::Writable(written, needed) => {
                plan.fill_type(&written);
                imports = needed;
            }
            // A type WAS inferred and is not one a declaration may carry — `void`, a type variable,
            // a captured wildcard flattened to `Object`. Both kinds of requirement refuse here: for
            // a field or a whole statement because the placeholder never compiles, and in a
            // target-typed position because this answer is exactly the poly expression re-inferring
            // itself against nothing.
            Declarable::Unwritable
                if matches!(slot.need, TypeNeed::Required | TypeNeed::RequiredOnceInferred) =>
            {
                return Err(unnameable(&args.source, &slot))
            }
            // Nothing was inferred, so there is no signal to act on. Only a slot that cannot take
            // the placeholder AT ALL refuses; everywhere else `var` stands, which is what javac
            // would have inferred anyway.
            Declarable::Unknown if matches!(slot.need, TypeNeed::Required) => {
                return Err(unnameable(&args.source, &slot))
            }
            // The plan stands and carries `var`; the caller is told, because an editor that writes
            // `var` into a Java 8 project without saying so is worse than one that declines.
            _ => plan.type_slot = None,
        }
    }

    // The `throws` the plan could only guess at, answered exactly. The refactoring crate reads the
    // tree and can see the enclosing method's clause and the catches around the selection; what it
    // cannot see is which of those a call actually raises, nor a checked exception that reaches the
    // moved body through a `try` the selection itself contains. The resolver's answer REPLACES the
    // guess when it is complete and is dropped when it is not — never added to it. See
    // `merge_throws`: the guess is already a sound upper bound, because the code compiled before.
    if let Some(slot) = plan.throws_slot.clone() {
        if let Some(resolver) =
            crate::index_service::IndexService::global().caret_resolver_for(&args.file)
        {
            let proven = bennu_check::prelude::checked_exceptions_in(
                &args.source,
                slot.start,
                slot.end,
                &*resolver,
            );
            plan.fill_throws(&bennu_refactor::prelude::merge_throws(
                &slot.placeholder,
                &proven.kinds,
                proven.complete,
                &args.source,
            ));
        }
    }

    // The import goes in as one more edit, so accepting the refactoring is a single undo.
    for fqn in imports {
        if let Some(edit) = bennu_intentions::prelude::insert_import_edit(&args.source, &fqn) {
            plan.edits.push(RefactorEdit::new(edit.start, edit.end, edit.replacement, "import"));
        }
    }
    // The invariant every consumer stands on — see `Plan::new`. Pushing broke it; this restores it,
    // through the crate's own rule rather than a second copy of the comparator.
    plan.reorder();

    Ok(RefactorPlanDto::of(plan))
}

/// Why a refactoring that needs a written type could not be applied.
///
/// Its wording is the whole of what the user is told, so it names both ways of getting here rather
/// than the one that happened to be measured first.
fn unnameable(source: &str, slot: &bennu_refactor::prelude::TypeSlot) -> String {
    format!(
        "the type of `{}` could not be resolved, and this refactoring needs it written out — the call may return nothing to name, or its type may be decided by the context it sits in",
        source.get(slot.start..slot.end).unwrap_or_default().trim()
    )
}

/// A planned refactoring, on the wire.
#[derive(Serialize)]
pub struct RefactorPlanDto {
    pub id: String,
    pub label: String,
    /// **Descending by start**: applying them in this order needs no offset re-mapping, which is the
    /// contract the FE applies them under.
    pub edits: Vec<RefactorEditDto>,
    /// The name the refactoring introduces, for the editor to offer for renaming.
    pub name: String,
    /// Where the caret should land, when the refactoring has an opinion.
    pub caret: Option<usize>,
    /// True when a type could not be resolved and the plan still carries `var`. The editor says so
    /// rather than letting it land silently.
    pub unresolved_type: bool,
}

impl RefactorPlanDto {
    fn of(plan: Plan) -> Self {
        let unresolved_type =
            plan.edits.iter().any(|e| e.reason == "declaration" && e.text.contains(TYPE_PLACEHOLDER));
        Self {
            id: plan.id,
            label: plan.label,
            name: plan.name.unwrap_or_default(),
            caret: plan.caret,
            unresolved_type,
            edits: plan
                .edits
                .into_iter()
                .map(|e| RefactorEditDto {
                    start: e.start,
                    end: e.end,
                    text: e.text,
                    reason: e.reason,
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
pub struct RefactorEditDto {
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub reason: String,
}

fn is_java(file: &str) -> bool {
    file.rsplit('.').next().is_some_and(|e| e.eq_ignore_ascii_case("java"))
}

/// Args for [`bennu_create_class`].
#[derive(Debug, Deserialize)]
pub struct CreateClassArgs {
    /// Absolute path of the file the unresolved type is written in — the new file goes beside it.
    pub file: String,
    pub source: String,
    /// The span of the type name, from the `unresolved-type` diagnostic.
    pub start: usize,
    pub end: usize,
}

/// Create the file for a type that does not exist, and answer with its path.
///
/// ## Why beside the current file and not somewhere chosen
///
/// The new type goes in the **same package** as the code that named it, which is what an unqualified
/// reference means — and a package is a directory, so "the same package" and "the same folder" are
/// the same instruction. That removes the whole question of source roots, which is the part that
/// gets wrong on a multi-module build and puts the file where nothing compiles it.
///
/// A file that already exists is an error and not an overwrite: the diagnostic said the *type* does
/// not resolve, which on a file that exists means something else is wrong — a bad package line, a
/// missing import — and replacing it would delete somebody's code to fix a squiggle.
#[arbor_rpc::handler]
fn bennu_create_class(_ctx: &BennuState, args: CreateClassArgs) -> Result<String, String> {
    if !is_java(&args.file) {
        return Err("classes are only created for Java files".to_string());
    }
    let tree = bennu_java::prelude::parse_java(&args.source).ok_or("this file does not parse")?;
    let missing = missing_type_at(tree.root_node(), &args.source, args.start, args.end)
        .ok_or("there is no unresolved type name here")?;

    let here = std::path::Path::new(&args.file);
    let folder = here.parent().ok_or("the file has no folder")?;
    let target = folder.join(format!("{}.java", missing.name));
    if target.exists() {
        return Err(format!("{}.java already exists in this package", missing.name));
    }
    let package = package_of(&args.source);
    let body = new_type_source(package.as_deref(), missing.keyword, &missing.name);
    std::fs::write(&target, body).map_err(|e| format!("could not write {}: {e}", target.display()))?;
    Ok(target.display().to_string())
}

/// The package a Java source declares, if it declares one.
fn package_of(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let line = line.trim();
        line.strip_prefix("package ")
            .map(|rest| rest.trim_end_matches(';').trim().to_string())
            .filter(|p| !p.is_empty())
    })
}

#[cfg(test)]
mod throws_tests {
    use bennu_refactor::prelude::merge_throws;

    /// The analysis is a lower bound, so what the plan already had survives.
    #[test]
    fn what_the_plan_guessed_is_kept() {
        assert_eq!(merge_throws(" throws IOException", &[], false, "class A {}"), " throws IOException");
    }

    /// …and what the resolver proved is added to it.
    #[test]
    fn what_the_resolver_proved_is_added() {
        let source = "import java.sql.SQLException;\nclass A {}";
        assert_eq!(
            merge_throws(" throws IOException", &["java/io/IOException".to_string(), "java/sql/SQLException".to_string()], true, source),
            " throws IOException, SQLException"
        );
    }

    /// The two halves spell names differently; the same type must not be listed twice.
    #[test]
    fn the_same_exception_spelled_two_ways_is_listed_once() {
        assert_eq!(
            merge_throws(" throws IOException", &["java/io/IOException".to_string()], true, "class A {}"),
            " throws IOException"
        );
    }

    #[test]
    fn a_body_that_throws_nothing_and_a_plan_that_guessed_nothing_is_no_clause() {
        assert_eq!(merge_throws("", &[], true, "class A {}"), "");
    }

    #[test]
    fn nothing_thrown_is_no_clause_at_all() {
        assert_eq!(merge_throws("", &[], true, "class A {}"), "");
    }

    #[test]
    fn a_type_the_file_imports_is_written_the_way_the_file_writes_it() {
        let source = "import java.io.IOException;\nclass A {}";
        assert_eq!(
            merge_throws("", &["java/io/IOException".to_string()], true, source),
            " throws IOException"
        );
    }

    /// Not an added import: a `throws` clause is not a reason to change what the file imports, and
    /// the dotted name compiles anywhere.
    #[test]
    fn a_type_the_file_does_not_import_keeps_its_package() {
        assert_eq!(
            merge_throws("", &["java/io/IOException".to_string()], true, "class A {}"),
            " throws java.io.IOException"
        );
    }

    #[test]
    fn java_lang_needs_no_import_to_be_written_short() {
        assert_eq!(
            merge_throws("", &["java/lang/Exception".to_string()], true, "class A {}"),
            " throws Exception"
        );
    }

    #[test]
    fn several_are_listed_in_order() {
        let source = "import java.io.IOException;\nimport java.sql.SQLException;\nclass A {}";
        assert_eq!(
            merge_throws(
                "",
                &["java/io/IOException".to_string(), "java/sql/SQLException".to_string()],
                true,
                source
            ),
            " throws IOException, SQLException"
        );
    }
}

/// Where the caret is, for a safe delete.
#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SafeDeleteArgs {
    /// Absolute path of the file the caret is in.
    pub file: String,
    /// The buffer's current text — the editor's, not the disk's.
    pub source: String,
    /// Byte offset of the caret.
    pub offset: usize,
}

/// What a safe delete would do, on the wire.
#[derive(Serialize)]
pub struct SafeDeleteDto {
    /// `method Order.total()` — what is about to go.
    pub label: String,
    /// The file the declaration is in, which need not be the caret's.
    pub file: String,
    /// The byte range to remove. Only meaningful when `safe`.
    pub start: usize,
    pub end: usize,
    /// Whether it may be applied. **The one field a caller has to read.**
    pub safe: bool,
    /// Why it may not be, whatever the usages say.
    pub blocked: Option<String>,
    /// The uses that have to go first. The list IS the answer — "it is used" is not one.
    ///
    /// The same [`UsageHit`] find-usages returns, so the editor renders both lists with one widget
    /// and a row means the same thing in each.
    pub usages: Vec<UsageHit>,
    /// The file to delete along with the declaration: a top-level type is its file.
    pub file_delete: Option<String>,
}

/// Plan a **safe delete** at the caret: what would be removed, or who still needs it.
///
/// Never deletes anything itself. The caller reads `safe`, and on `false` shows `blocked` or the
/// `usages` list instead — which is the whole feature: a delete that silently broke four call sites
/// in files nobody opened would be worse than not offering one.
#[arbor_rpc::handler(mcp(
    title = "Plan a safe delete",
    safety = read,
    description = "What deleting the member at the caret would remove, or every use that still \
needs it. Plans only — nothing is written. Read `safe` first: when it is false, either `blocked` \
says why the member can never be removed, or `usages` lists every site that has to go first.",
))]
pub(crate) fn bennu_safe_delete(
    _ctx: &BennuState,
    args: SafeDeleteArgs,
) -> Result<Option<SafeDeleteDto>, String> {
    if !is_java(&args.file) {
        return Ok(None);
    }
    let Some(plan) =
        crate::index_service::IndexService::global().plan_safe_delete(&args.file, &args.source, args.offset)
    else {
        return Ok(None);
    };
    Ok(Some(SafeDeleteDto {
        safe: plan.is_safe(),
        label: plan.label,
        file: plan.file,
        start: plan.start,
        end: plan.end,
        blocked: plan.blocked,
        usages: plan
            .usages
            .into_iter()
            .map(|u| crate::references::usage_hit(u, None))
            .collect(),
        file_delete: plan.file_delete,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_java_files_are_offered_refactorings() {
        assert!(is_java("/p/App.java"));
        assert!(!is_java("/p/pom.xml"));
        assert!(!is_java("/p/App"));
    }

    /// The flag that keeps a `var` from landing silently on a Java 8 project.
    #[test]
    fn a_plan_still_holding_the_placeholder_says_so() {
        let plan = Plan::new(
            "extract-variable",
            "Extract variable",
            vec![RefactorEdit::new(0, 0, "var name = x;", "declaration")],
        );
        assert!(RefactorPlanDto::of(plan).unresolved_type);
    }

    #[test]
    fn a_resolved_plan_does_not_claim_an_unresolved_type() {
        let plan = Plan::new(
            "extract-variable",
            "Extract variable",
            vec![RefactorEdit::new(0, 0, "List<String> name = x;", "declaration")],
        );
        assert!(!RefactorPlanDto::of(plan).unresolved_type);
    }
}
