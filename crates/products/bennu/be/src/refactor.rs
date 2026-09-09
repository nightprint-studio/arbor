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
    missing_type_at, new_type_source, plan_for, refactorings_at, transfer_into, Plan, RefactorEdit,
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
    /// The type a member move was told to go to, when the user picked one — see
    /// [`bennu_move_targets`]. Empty for every refactoring that needs no target and for the rows
    /// that carry theirs in the id.
    #[serde(default)]
    pub target: String,
    /// That type's file, which the picker already knew. Saves resolving the name a second time,
    /// and resolves it the way the picker did rather than a way that might differ.
    #[serde(default)]
    pub target_file: String,
}

impl RefactorArgs {
    /// What the plan calls itself once a target has been picked. The label is not decoration: it is
    /// what the editor shows while the edits are being previewed, and `Move member` says less than
    /// `Move member to \`Builder\`` about what is about to happen.
    fn label_for_target(&self) -> String {
        let verb = match bennu_refactor::prelude::MoveDirection::from_id(&self.id) {
            bennu_refactor::prelude::MoveDirection::Up => "Pull member up to",
            bennu_refactor::prelude::MoveDirection::Down => "Push member down to",
            bennu_refactor::prelude::MoveDirection::Across => "Move member to",
        };
        format!("{verb} `{}`", self.target)
    }
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
    /// This row does not act: it opens the **target picker**, and the choice comes back as
    /// `target` on the plan call. The editor needs to know before it runs the row, because the two
    /// are different gestures — one edits, one asks a question.
    pub picks_target: bool,
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
    let mut offers: Vec<RefactorOffer> = refactorings_at(&args.source, args.start, args.end)
        .into_iter()
        .map(|outcome| match outcome {
            Ok(plan) => RefactorOffer {
                id: plan.id,
                label: plan.label,
                reason: String::new(),
                name: plan.name.unwrap_or_default(),
                picks_target: false,
            },
            Err(refusal) => RefactorOffer {
                id: refusal.id,
                label: refusal.label,
                reason: refusal.reason,
                name: String::new(),
                picks_target: false,
            },
        })
        .collect();
    offers.extend(picker_rows(&args));
    Ok(offers)
}

/// The rows that open the target picker — one per move family, added to whatever the buffer alone
/// could answer.
///
/// **Added, not substituted.** The rows the pure crate produces name a target in this same file and
/// are one keystroke each; a picker in front of them would be a dialog between the user and the
/// thing they already pointed at. These are for the targets that are *not* here — a superclass in
/// another file, a subtype the index knows about — which is the case the buffer can say nothing
/// about, and until now the case that simply had no answer.
fn picker_rows(args: &RefactorArgs) -> Vec<RefactorOffer> {
    let Some(site) = bennu_refactor::prelude::move_site(&args.source, args.start, args.end) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    if !site.supertypes.is_empty() {
        rows.push(RefactorOffer {
            id: "pull-up-member@pick".to_string(),
            label: "Pull member up to…".to_string(),
            reason: String::new(),
            name: site.member.clone(),
            picks_target: true,
        });
    }
    rows.push(RefactorOffer {
        id: "push-down-member@pick".to_string(),
        label: "Push member down to…".to_string(),
        reason: String::new(),
        name: site.member.clone(),
        picks_target: true,
    });
    rows.push(RefactorOffer {
        id: "move-member@pick".to_string(),
        label: "Move member to…".to_string(),
        reason: String::new(),
        name: site.member,
        picks_target: true,
    });
    rows
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
    // A target the **picker** chose is not in the offer list — the list is answered from the buffer
    // and this target may be in a file it has never seen. So the plan is asked for directly, with
    // the direction read off the id the row carried: which of the three moves this is decides every
    // check that follows, and a string typed in the wrong place here would refuse every instance
    // member for a reason that has nothing to do with what was asked.
    let outcome = if args.target.is_empty() {
        plan_for(&args.id, &args.source, args.start, args.end)
            .ok_or_else(|| format!("`{}` no longer applies here", args.id))?
    } else {
        bennu_refactor::prelude::move_member_plan(
            &args.source,
            args.start,
            args.end,
            &args.target,
            bennu_refactor::prelude::MoveDirection::from_id(&args.id),
            &args.id,
            &args.label_for_target(),
        )
        .ok_or("there is no member to move at the caret")?
    };
    let mut plan = outcome.map_err(|refusal| refusal.reason)?;
    let mut imports: Vec<String> = Vec::new();

    // A claim the plan could only assert from the text, checked against the resolver. Only a type
    // that DISAGREES refuses: an unknown or unwritable answer is no evidence, and the plan is then
    // exactly what a caller without a resolver would have applied. See `TypeGuard`.
    if let Some(guard) = plan.type_guard.clone() {
        if let bennu_intel::prelude::Declarable::Writable(inferred, _) =
            crate::index_service::IndexService::global().infer_type_detail(
                &args.file,
                &args.source,
                guard.start,
                guard.end,
            )
        {
            if inferred != guard.written {
                return Err(format!(
                    "`var` would infer `{inferred}` here, not the `{}` this declares",
                    guard.written
                ));
            }
        }
    }

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

    // The Java the code this plan writes needs, against the Java the project targets. A `default`
    // method written into a Java 7 project compiles nowhere, and finding that out from the build is
    // the worst way to be told. Unknown on either side leaves the plan alone: a level nobody could
    // read is not evidence of an old one — see `NeedsLevel`.
    if let Some(needs) = plan.needs_level.clone() {
        let level = crate::index_service::IndexService::global()
            .root_for_file(&args.file)
            .and_then(|root| crate::index_service::IndexService::global().jdk_version_of(&root))
            .and_then(|declared| bennu_refactor::prelude::language_level(&declared));
        if let Some(level) = level {
            if level < needs.at_least {
                return Err(format!(
                    "this project targets Java {level}, and {} — Java {}",
                    needs.because, needs.at_least
                ));
            }
        }
    }

    // A nested type given its own file leaves behind every `Outer.Inner` in the project — an
    // `import a.b.Outer.Inner;` two packages away included. Nothing in the file the refactoring was
    // invoked in can show that, so the pure crate says what it needs checked and this is where it
    // is checked: see `NewSource::was_nested`.
    if let Some(created) = plan.new_source.clone() {
        if created.was_nested {
            if let Some(elsewhere) = used_by_another_file(&args, created.name_at) {
                return Err(format!(
                    "`{}` is used by {elsewhere} — a nested type reached from another file is \
                     written `Outer.{}` or imported as one, and both stop resolving once it is \
                     top-level",
                    created.name, created.name
                ));
            }
        }
    }

    // The half of a member move that lands in ANOTHER file. The plan already carries the removal,
    // so a failure here has to be an error and not a warning: applying half of it would delete a
    // member and write it nowhere.
    if plan.transfer.is_some() {
        let edits = transfer_edits(&plan, &args)?;
        plan.edits.extend(edits);
        plan.transfer = None;
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

    Ok(RefactorPlanDto::of(plan, &args.file))
}

/// Where a transfer is going to land — the picker's answer when there was one, go-to-declaration's
/// when there was not.
struct TargetFile {
    file: String,
}

/// A file other than this one that uses the symbol at `offset`, named for the sentence.
///
/// `None` also when the index cannot answer — it may still be building — and that is deliberate in
/// the same direction every other check here leans: only a **positive** finding refuses. A move
/// that goes ahead on a cold index is the outcome a user without an index would have had anyway.
fn used_by_another_file(args: &RefactorArgs, offset: usize) -> Option<String> {
    let found = crate::index_service::IndexService::global()
        .find_usages(&args.file, &args.source, offset)?;
    let mine = args.file.replace('\\', "/");
    let other = found
        .usages
        .iter()
        .find(|hit| hit.file.replace('\\', "/") != mine)?;
    Some(
        std::path::Path::new(&other.file)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| other.file.clone()),
    )
}

/// Find the type a [`MemberTransfer`] targets and plan the member into it.
///
/// The target is found by **asking go-to-declaration about the very offset the name is written at**
/// — the `extends` clause the pull-up read it off. That is not a shortcut around a name lookup: it
/// is the only way to resolve it the way the compiler does, through this file's imports and this
/// file's package, so a `Base` that names two different classes in two packages goes to the right
/// one.
///
/// [`MemberTransfer`]: bennu_refactor::prelude::MemberTransfer
fn transfer_edits(plan: &Plan, args: &RefactorArgs) -> Result<Vec<RefactorEdit>, String> {
    let transfer = plan.transfer.as_ref().ok_or("this plan moves nothing between files")?;
    // The picker already resolved the type — it is how it listed it — so the file comes back with
    // the choice rather than being looked up a second time, possibly a different way.
    let target = match args.target_file.is_empty() {
        false => TargetFile { file: args.target_file.clone() },
        true => TargetFile {
            file: crate::index_service::IndexService::global()
                .declaration(&args.file, &args.source, transfer.target_at)
                .ok_or_else(|| {
                    format!(
                        "`{}` could not be resolved — either the index is still building, or it is \
                         a type this project does not hold the source of",
                        transfer.target
                    )
                })?
                .file,
        },
    };
    if !is_java(&target.file) {
        return Err(format!(
            "`{}` is not a source file of this project, so the member cannot be written into it",
            transfer.target
        ));
    }
    // The buffer for the target when the editor already has it open would be better still, but the
    // backend does not hold the editor's buffers — so the file on disk it is, decoded the way the
    // index decodes it. A file whose bytes do not fit its declared encoding is refused rather than
    // edited: the editor and the index would disagree about every offset after the first bad byte.
    let encoding = crate::index_service::resolve_index_encoding(&target.file);
    let bytes = std::fs::read(&target.file)
        .map_err(|e| format!("could not read {}: {e}", target.file))?;
    let decoded = bennu_project::prelude::decode_for_index(&bytes, &encoding);
    if decoded.non_compliant {
        return Err(format!(
            "{} is not valid in the project's declared encoding, so an edit planned here would \
             land on the wrong bytes",
            target.file
        ));
    }
    let source = bennu_project::prelude::normalize_newlines(&decoded.text);
    transfer_into(plan, &source, &target.file)
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

/// Args for [`bennu_move_targets`].
#[derive(Deserialize)]
pub struct MoveTargetsArgs {
    pub file: String,
    pub source: String,
    pub start: usize,
    pub end: usize,
    /// The refactoring asking: `pull-up-member`, `push-down-member` or `move-member`, with or
    /// without the `@pick` the offer row carries.
    pub id: String,
}

/// One type a member could move into.
#[derive(Serialize)]
pub struct MoveTarget {
    /// Simple name — what the row says, and what the plan is asked for.
    pub name: String,
    /// Fully-qualified, for the subtitle: two `Builder`s in a project is the normal case.
    pub qualified: String,
    /// The file that declares it, so the plan can go straight there without a second lookup.
    pub file: String,
    /// `class` · `interface` · `enum` · `record` — the row's glyph.
    pub kind: String,
    /// Picking this one **widens** the member to `protected`: it is `private` and the class it
    /// leaves still reads it. Said on the row, because a visibility change nobody was told about is
    /// the kind of thing found later, in a review.
    pub widens: bool,
}

/// Every type the member at the caret could move into, for the picker.
///
/// ## Why this is a separate call and not part of the offer list
///
/// The offer list is what Alt+Enter shows, and it is answered from the buffer alone — no index, no
/// project, a few microseconds. The candidates are the opposite: for *move member* they are every
/// type the project declares, which is thousands on a real codebase and needs the index warm. Asking
/// for them up front would put that cost on every Alt+Enter, for a list nobody opened.
///
/// So the menu carries one row per family, and the row opens the picker, and the picker asks this.
///
/// ## Where each family's candidates come from
///
/// - **pull up** — the supertypes the class *writes*, resolved through this file's own imports.
///   Reading them off the source rather than the hierarchy is deliberate: those are the ones a pull
///   up can name, and the hierarchy would also offer `Object`.
/// - **push down** — the subtypes the index knows, which is the half the buffer cannot see and the
///   reason this refactoring needed a picker at all.
/// - **move** — every type in the project, minus the one the member is already in. The picker
///   filters; a list is not a menu.
#[arbor_rpc::handler]
pub(crate) fn bennu_move_targets(
    _ctx: &BennuState,
    args: MoveTargetsArgs,
) -> Result<Vec<MoveTarget>, String> {
    if !is_java(&args.file) {
        return Ok(Vec::new());
    }
    let Some(site) = bennu_refactor::prelude::move_site(&args.source, args.start, args.end) else {
        return Ok(Vec::new());
    };
    let service = crate::index_service::IndexService::global();
    // Only a pull up can widen: down and across keep the member where everything can already see
    // it, and an interface makes its members public anyway.
    let widening = site.widens_private
        && bennu_refactor::prelude::MoveDirection::from_id(&args.id)
            == bennu_refactor::prelude::MoveDirection::Up;
    let by_name = |name: &str| -> Option<MoveTarget> {
        let root = service.root_for_file(&args.file)?;
        let entry = service
            .class_index(&root)?
            .into_iter()
            .find(|e| e.simple == name)?;
        Some(MoveTarget {
            widens: widening && entry.kind != "interface" && entry.kind != "annotation",
            name: entry.simple,
            qualified: entry.fqcn,
            file: entry.file,
            kind: entry.kind,
        })
    };

    match bennu_refactor::prelude::MoveDirection::from_id(&args.id) {
        // The supertypes this class writes. A name the project does not declare — `Serializable`,
        // anything from a jar — is dropped rather than offered: a member cannot be written into a
        // class file, and finding that out after picking it is a worse answer than not offering it.
        bennu_refactor::prelude::MoveDirection::Up => {
            Ok(site.supertypes.iter().filter_map(|s| by_name(simple_of(s))).collect())
        }
        bennu_refactor::prelude::MoveDirection::Down => {
            let Some(binary) = service.classify_type(&args.file, &args.source, args.start) else {
                return Ok(Vec::new());
            };
            let items = service.hierarchy_step(
                &args.file,
                &bennu_intel::prelude::HierarchyHandle::Type { binary },
                bennu_intel::prelude::HierarchyDirection::Subtypes,
            );
            Ok(items
                .into_iter()
                // A subtype with no file is one from a dependency: nothing here can write into it.
                .filter(|item| is_java(&item.file))
                .map(|item| MoveTarget {
                    qualified: item.detail.clone().unwrap_or_else(|| item.name.clone()),
                    name: item.name,
                    file: item.file,
                    kind: item.kind,
                    widens: false,
                })
                .collect())
        }
        bennu_refactor::prelude::MoveDirection::Across => {
            let root = service
                .root_for_file(&args.file)
                .ok_or("no project holds this file")?;
            let index = service
                .class_index(&root)
                .ok_or("the project index is still building — the list of types is not ready yet")?;
            Ok(index
                .into_iter()
                .filter(|e| e.simple != site.owner)
                .map(|e| MoveTarget {
                    name: e.simple,
                    qualified: e.fqcn,
                    file: e.file,
                    kind: e.kind,
                    widens: false,
                })
                .collect())
        }
    }
}

/// A written type name without its package or its type arguments.
fn simple_of(written: &str) -> &str {
    let base = written.split('<').next().unwrap_or(written).trim();
    base.rsplit('.').next().unwrap_or(base)
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
    /// The file this refactoring also has to create, when it creates one — *move class* is the
    /// only one that does. The path is beside the file the refactoring was invoked in, which is
    /// what "the same package" means on a filesystem.
    pub new_file: Option<NewFileDto>,
    /// Every file this plan touches besides the one it was invoked in, so the editor can say so
    /// before applying and can re-read them after.
    pub other_files: Vec<String>,
}

/// A source file a plan needs written.
#[derive(Serialize)]
pub struct NewFileDto {
    pub path: String,
    pub text: String,
}

impl RefactorPlanDto {
    fn of(plan: Plan, file: &str) -> Self {
        let unresolved_type =
            plan.edits.iter().any(|e| e.reason == "declaration" && e.text.contains(TYPE_PLACEHOLDER));
        // Beside the file that lost the type: the new type stays in the same package, and a package
        // is a directory. The same rule `bennu_create_class` follows, and for the same reason — it
        // is the one that cannot get a multi-module build wrong.
        let new_file = plan.new_source.as_ref().and_then(|created| {
            let folder = std::path::Path::new(file).parent()?;
            let path = folder.join(format!("{}.java", created.name));
            Some(NewFileDto { path: path.display().to_string(), text: created.text.clone() })
        });
        let mut other_files: Vec<String> =
            plan.edits.iter().filter(|e| !e.file.is_empty()).map(|e| e.file.clone()).collect();
        other_files.sort();
        other_files.dedup();
        Self {
            id: plan.id,
            label: plan.label,
            name: plan.name.unwrap_or_default(),
            caret: plan.caret,
            unresolved_type,
            new_file,
            other_files,
            edits: plan
                .edits
                .into_iter()
                .map(|e| RefactorEditDto {
                    start: e.start,
                    end: e.end,
                    text: e.text,
                    reason: e.reason,
                    file: e.file,
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
    /// The file this edit lands in — **empty for the buffer the refactoring was invoked in**, which
    /// is every edit of every refactoring but a member move that crosses a file.
    pub file: String,
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
        assert!(RefactorPlanDto::of(plan, "A.java").unresolved_type);
    }

    #[test]
    fn a_resolved_plan_does_not_claim_an_unresolved_type() {
        let plan = Plan::new(
            "extract-variable",
            "Extract variable",
            vec![RefactorEdit::new(0, 0, "List<String> name = x;", "declaration")],
        );
        assert!(!RefactorPlanDto::of(plan, "A.java").unresolved_type);
    }
}
