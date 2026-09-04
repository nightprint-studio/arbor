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
        let typed = crate::index_service::IndexService::global().infer_type_source(
            &args.file,
            &args.source,
            slot.start,
            slot.end,
        );
        match typed {
            Some((written, needed)) => {
                plan.fill_type(&written);
                imports = needed;
            }
            // Nothing could type it. Where the placeholder is an acceptable answer the plan stands
            // and the caller is told — an editor that writes `var` into a Java 8 project without
            // saying so is worse than one that declines, but `var` is still correct from Java 10
            // on, so that is a note and not an error.
            //
            // Where it is NOT acceptable, it is an error: naming a whole statement whose call
            // returns `void` produces `var setName = obj.setName(x);`, which does not compile and
            // which no amount of `var` fixes. Refusing says why; applying leaves a broken line.
            None if slot.required => {
                return Err(format!(
                    "the type of `{}` could not be resolved, and this refactoring needs it — the call may return nothing to name",
                    args.source.get(slot.start..slot.end).unwrap_or_default().trim()
                ))
            }
            None => plan.type_slot = None,
        }
    }

    // The `throws` the plan could only guess at, answered exactly. The refactoring crate reads the
    // tree and can see the enclosing method's clause and the catches around the selection; what it
    // cannot see is which of those a call actually raises, nor a checked exception that reaches the
    // moved body through a `try` the selection itself contains. What the resolver proves is ADDED to
    // the guess and never substituted for it: the analysis is a lower bound (a call it cannot read
    // contributes nothing), and narrowing a `throws` clause on an incomplete set is a call site that
    // stops compiling.
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
            plan.fill_throws(&merge_throws(&slot.placeholder, &proven, &args.source));
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

/// The plan's guessed clause with everything the resolver proved added to it.
///
/// Added, never replaced. The analysis is a lower bound — a call it cannot read contributes nothing
/// — so treating its answer as the whole truth would drop exceptions the guess had right. Matching
/// is by SIMPLE name because the two halves are spelled differently: the guess carries the source's
/// own words, the analysis JVM binary names.
fn merge_throws(guessed: &str, proven: &[String], source: &str) -> String {
    let mut names: Vec<String> = guessed
        .trim()
        .trim_start_matches("throws")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let simple = |n: &str| n.rsplit(['.', '/', '$']).next().unwrap_or(n).to_string();
    for binary in proven {
        if names.iter().any(|n| simple(n) == simple(binary)) {
            continue;
        }
        names.push(written_name(binary, source));
    }
    if names.is_empty() {
        return String::new();
    }
    format!(" throws {}", names.join(", "))
}

/// How a binary name should be written in this file: short when the file already imports it (or it
/// is `java.lang`), dotted otherwise — a `throws` clause is not a reason to add an import.
fn written_name(binary: &str, source: &str) -> String {
    let dotted = binary.replace('/', ".").replace('$', ".");
    let simple = dotted.rsplit('.').next().unwrap_or(&dotted).to_string();
    let imported = source.contains(&format!("import {dotted};"))
        || (binary.starts_with("java/lang/") && binary.matches('/').count() == 2);
    if imported {
        simple
    } else {
        dotted
    }
}

/// ` throws IOException, SQLException` — or nothing at all, written the way the file writes names.
///
/// The simple name when the file already imports that exact type (or it is `java.lang`), and the
/// dotted name otherwise. Not "simple name plus a new import": a `throws` clause is not a reason to
/// change the file's imports, and a fully-qualified name in a signature always compiles.
fn throws_clause(kinds: &[String], source: &str) -> String {
    merge_throws("", kinds, source)
}

#[cfg(test)]
mod throws_tests {
    use super::{merge_throws, throws_clause};

    /// The analysis is a lower bound, so what the plan already had survives.
    #[test]
    fn what_the_plan_guessed_is_kept() {
        assert_eq!(merge_throws(" throws IOException", &[], "class A {}"), " throws IOException");
    }

    /// …and what the resolver proved is added to it.
    #[test]
    fn what_the_resolver_proved_is_added() {
        let source = "import java.sql.SQLException;\nclass A {}";
        assert_eq!(
            merge_throws(" throws IOException", &["java/sql/SQLException".to_string()], source),
            " throws IOException, SQLException"
        );
    }

    /// The two halves spell names differently; the same type must not be listed twice.
    #[test]
    fn the_same_exception_spelled_two_ways_is_listed_once() {
        assert_eq!(
            merge_throws(" throws IOException", &["java/io/IOException".to_string()], "class A {}"),
            " throws IOException"
        );
    }

    #[test]
    fn a_body_that_throws_nothing_and_a_plan_that_guessed_nothing_is_no_clause() {
        assert_eq!(merge_throws("", &[], "class A {}"), "");
    }

    #[test]
    fn nothing_thrown_is_no_clause_at_all() {
        assert_eq!(throws_clause(&[], "class A {}"), "");
    }

    #[test]
    fn a_type_the_file_imports_is_written_the_way_the_file_writes_it() {
        let source = "import java.io.IOException;\nclass A {}";
        assert_eq!(
            throws_clause(&["java/io/IOException".to_string()], source),
            " throws IOException"
        );
    }

    /// Not an added import: a `throws` clause is not a reason to change what the file imports, and
    /// the dotted name compiles anywhere.
    #[test]
    fn a_type_the_file_does_not_import_keeps_its_package() {
        assert_eq!(
            throws_clause(&["java/io/IOException".to_string()], "class A {}"),
            " throws java.io.IOException"
        );
    }

    #[test]
    fn java_lang_needs_no_import_to_be_written_short() {
        assert_eq!(
            throws_clause(&["java/lang/Exception".to_string()], "class A {}"),
            " throws Exception"
        );
    }

    #[test]
    fn several_are_listed_in_order() {
        let source = "import java.io.IOException;\nimport java.sql.SQLException;\nclass A {}";
        assert_eq!(
            throws_clause(
                &["java/io/IOException".to_string(), "java/sql/SQLException".to_string()],
                source
            ),
            " throws IOException, SQLException"
        );
    }
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
