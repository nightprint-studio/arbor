//! `naming_fix` domain — `bennu_naming_fix_plan`: fix every naming violation at once.
//!
//! Alt+Enter fixes the name under the caret. This fixes a file, or a project, and it exists because
//! the alternative is visiting several hundred squiggles by hand — which is not a workflow, it is a
//! reason to leave the check switched off.
//!
//! ## It plans; it never writes
//!
//! The handler returns **edits**, exactly like [`crate::rename`] does, and the editor applies them
//! through CodeMirror so a single Undo takes the whole thing back. A bulk rename that wrote to disk
//! would be the one operation in the editor with no way back.
//!
//! ## Types are planned as a batch, and that is the whole performance story
//!
//! Planning a **type** rename costs a pass over every project source: the declaration and the
//! `import` statements can only be found by reading them. One rename at a time that is
//! `types × files` parses — invisible for a single Shift+F6, and *minutes* for a bulk fix over a
//! legacy tree, with the caller's request blocked for all of it.
//!
//! So the run has two phases. Everything that is cheap — locals, parameters, members, all index
//! lookups or single-file work — is planned as each file is read. Types are only **classified**
//! there (a lookup, no scan) and planned together at the end, in one pass, through
//! [`IndexService::plan_type_renames`].
//!
//! ## It reports progress and it can be stopped
//!
//! A long operation with no progress is indistinguishable from a hung one, and a long operation
//! with no cancel is a decision you cannot take back. Both were missing in the first cut of this,
//! and a project-wide run on a real tree was exactly the unbounded wait that predicts.
//!
//! ## What it refuses, and says so
//!
//! A bulk fix that silently skips things is worse than one that fixes less. Refusals are reported
//! by name: a **collision** (two names that would become the same name, or a name already spelled
//! the way this one wants to be), **nothing to rename** (the engine returned no edits — the caret
//! is on something it cannot rename, or the index is still building), a rename the engine refuses
//! outright (a method overriding a library type, whose name is fixed by a jar we cannot edit), or a
//! file whose bytes are **not valid in the project's declared encoding**.
//!
//! That last one is not fussiness. The index recovers such a file (`decode_for_index` retries
//! UTF-8, then Windows-1252) so its classes are still indexed; the editor reads it lossily, turning
//! each bad byte into U+FFFD. The two texts differ in LENGTH, so every offset after the first bad
//! byte disagrees, and edits planned against one and applied to the other land in the wrong place.
//! The apply path's per-edit check would catch it and drop them — safe, and unexplainable. Refusing
//! here says why.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};

use bennu_core::prelude::BennuState;
use bennu_intel::prelude::TypeRename;
use bennu_naming::prelude::{Target, Violation};
use bennu_proto::prelude::RenameEdit;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::index_service::IndexService;

/// Progress while a fix is being planned (`{ root, done, total, phase }`).
const EVT_FIX_PROGRESS: &str = "arbor://bennu/naming-fix-progress";

/// Upper bound on the files a project-wide fix will read.
///
/// The type pass is now one pass whatever the batch holds, so this bounds the *reading*, which is
/// linear. `capped` says when it bit, so the number on screen is never mistaken for "all of them".
const MAX_PROJECT_FILES: usize = 5_000;

/// Set by [`bennu_cancel_naming_fix`], cleared at the start of every run.
///
/// A plain flag rather than a per-run token: only one fix can be in flight from the UI, and a
/// stale cancel that arrives before the next run starts is cleared by that run rather than
/// silently killing it.
static CANCELLED: AtomicBool = AtomicBool::new(false);

fn cancelled() -> bool {
    CANCELLED.load(Ordering::Relaxed)
}

// ── the wire ────────────────────────────────────────────────────────────────────
//
// snake_case, like every other bennu wire type (`RenamePreview`, `RenameEdit`, `DeclarationTarget`).
// These three carried `rename_all = "camelCase"`, which renamed exactly ONE field — `file_rename` →
// `fileRename` — while the `RenameEdit`s nested inside them stayed snake_case, because `rename_all`
// does not recurse. So the frontend read `file_rename`, got `undefined`, and the bulk fix silently
// never moved a single file: a rename of a public top-level type left the class renamed and the
// file behind it, which does not compile. One convention per wire, and it is the one already in use.

/// One name the plan would change.
#[derive(Debug, Clone, Serialize)]
pub struct RenamedName {
    pub file: String,
    /// 1-based line of the declaration — one file legitimately holds several declarations with
    /// the same name, and without this the list repeats itself with nothing to tell them apart.
    pub line: usize,
    pub from: String,
    pub to: String,
    /// The target slug (`method`, `local`, …) — lets the FE group the summary.
    pub target: String,
    /// The file this one rename also has to move, if any — renaming a public top-level type
    /// without its file leaves code that does not compile. Carried per name, like the edits, so
    /// unticking the name in the review skips its file move too.
    pub file_rename: Option<bennu_proto::prelude::RenameFileMove>,
    /// The edits THIS rename contributes, and only those.
    ///
    /// Carried per rename rather than pooled into one list for the whole plan, because the review
    /// lets the user drop individual names before applying: with a flat pool there is no way to
    /// say which edits belonged to the name being dropped, so it was all or nothing. Applying a
    /// reviewed plan means applying the union of the entries still selected.
    pub edits: Vec<RenameEdit>,
}

/// One name the plan would NOT change, and why.
#[derive(Debug, Clone, Serialize)]
pub struct FixRefusal {
    pub file: String,
    /// 1-based line of the declaration.
    pub line: usize,
    pub name: String,
    pub reason: String,
}

/// What a bulk fix would do. Nothing is written.
#[derive(Debug, Clone, Default, Serialize)]
pub struct NamingFixPlan {
    /// Every name the plan would change, each carrying its own edits. There is deliberately no
    /// flat pool of edits beside this: the review can drop individual names, and two lists that
    /// have to agree about which edits belong to which name is one list too many.
    pub renamed: Vec<RenamedName>,
    pub refused: Vec<FixRefusal>,
    /// The distinct files the edits touch — which is **not** the files that were read: renaming a
    /// method edits its callers, wherever they live.
    pub files: Vec<String>,
    /// Whether the project scan stopped at [`MAX_PROJECT_FILES`].
    pub capped: bool,
    /// Whether the user stopped it. The partial plan is still returned and still valid — it is
    /// simply not everything.
    pub cancelled: bool,
}

/// Args for [`bennu_naming_fix_plan`] and [`bennu_cancel_naming_fix`].
#[derive(Deserialize)]
pub struct NamingFixArgs {
    /// The open project's root.
    pub root: String,
    /// Fix only this file. `None` fixes the whole project.
    #[serde(default)]
    pub file: Option<String>,
    /// The live (possibly unsaved) buffer for `file`, so the plan matches what is on screen.
    /// Ignored for a project-wide fix, which reads from disk.
    #[serde(default)]
    pub source: Option<String>,
}

// ── handlers ────────────────────────────────────────────────────────────────────

/// Stop the fix currently being planned. Fire-and-forget; a no-op when nothing is running.
#[arbor_rpc::handler]
fn bennu_cancel_naming_fix(_ctx: &BennuState, _args: NamingFixArgs) -> Result<(), String> {
    CANCELLED.store(true, Ordering::Relaxed);
    Ok(())
}

/// Plan the fix for one file or for the whole project.
#[arbor_rpc::handler]
fn bennu_naming_fix_plan(ctx: &BennuState, args: NamingFixArgs) -> Result<NamingFixPlan, String> {
    // Refuse the whole run rather than every name in it. Without the semantic engine EVERY plan comes
    // back empty, and the scan would answer with one identical "nothing here can be renamed" per
    // violation — thousands of rows blaming the code for what is really "ask again in a moment".
    // One honest sentence is the whole difference.
    if !IndexService::global().has_semantic_engine(&args.root) {
        return Err("The semantic engine is still building — the whole-project reference index has \
                    to finish before names can be moved. Try again when indexing completes."
            .to_string());
    }
    CANCELLED.store(false, Ordering::Relaxed);
    let sink = ctx.event_sink();
    let progress = |phase: &str, done: usize, total: usize| {
        sink.emit(
            EVT_FIX_PROGRESS,
            json!({ "root": &args.root, "phase": phase, "done": done, "total": total }),
        );
    };

    let mut run = Run::default();
    match &args.file {
        Some(file) => {
            let scanned = match &args.source {
                // The editor's buffer is already decoded and LF-normalised — it came through
                // `bennu_read_file`, so it IS the text an edit will be applied to. Reading from
                // disk instead is what needs the care.
                Some(source) => ScannedSource { text: source.clone(), non_compliant: false },
                None => {
                    let encoding = crate::index_service::resolve_index_encoding(&args.root);
                    read_project_source(file, &encoding)
                        .ok_or_else(|| format!("could not read {file}"))?
                }
            };
            progress("reading", 0, 1);
            run.read_file(file, &scanned.text, scanned.non_compliant);
            progress("reading", 1, 1);
        }
        None => run.read_project(&args.root, &progress),
    }

    // Phase two: every type in one pass, reporting per FILE of that pass — not per rename. The
    // pass is where the time goes and a rename count would show "1 / 1" with a full bar for the
    // whole of it, which is the shape of a hang.
    if !run.pending_types.is_empty() && !cancelled() {
        run.plan_types(&args.root, &|done, total| {
            if done % 16 == 0 || done == total {
                progress("planning types", done, total);
            }
            !cancelled()
        });
    }

    let mut plan = run.plan;
    plan.cancelled = cancelled();
    let files: BTreeSet<&str> =
        plan.renamed.iter().flat_map(|r| r.edits.iter()).map(|e| e.file.as_str()).collect();
    plan.files = files.into_iter().map(str::to_string).collect();
    Ok(plan)
}

// ── the run ─────────────────────────────────────────────────────────────────────

/// A type violation waiting for the batched pass, with its resolved binary name.
///
/// Carries its `line` because the source it was found in is long gone by the time the batch runs —
/// the second phase has the plan, not the buffers.
struct PendingType {
    file: String,
    line: usize,
    violation: Violation,
    binary: String,
}

#[derive(Default)]
struct Run {
    plan: NamingFixPlan,
    pending_types: Vec<PendingType>,
    /// The byte ranges already spoken for, per file. An index rather than a scan of `plan.edits`,
    /// because a project-wide fix accumulates thousands of them and the check runs per edit.
    taken: HashMap<String, Vec<(usize, usize)>>,
}

impl Run {
    fn read_project(&mut self, root: &str, progress: &dyn Fn(&str, usize, usize)) {
        let mut budget = MAX_PROJECT_FILES;
        let mut files = Vec::new();
        collect_checkable(std::path::Path::new(root), &mut files, &mut budget);
        self.plan.capped = budget == 0;

        // Once for the whole sweep — see `read_project_source`.
        let encoding = crate::index_service::resolve_index_encoding(root);
        let total = files.len();
        for (done, file) in files.into_iter().enumerate() {
            if cancelled() {
                return;
            }
            // Every 16 files, not every file: a progress event per file on a 5k-file project is
            // 5k round-trips to the UI to move a bar that is 0.02% further along.
            if done % 16 == 0 {
                progress("reading", done, total);
            }
            let Some(scanned) = read_project_source(&file, &encoding) else { continue };
            self.read_file(&file.replace('\\', "/"), &scanned.text, scanned.non_compliant);
        }
        progress("reading", total, total);
    }

    /// Plan one file's cheap fixes, and set its type violations aside for the batch.
    ///
    /// `non_compliant` marks a file whose bytes don't fit the project's declared encoding: its
    /// names are still REPORTED — you want to know they are there — but every one is refused,
    /// because the text they were found in is not the text the editor would apply edits to. See
    /// [`read_project_source`].
    fn read_file(&mut self, file: &str, source: &str, non_compliant: bool) {
        let violations = crate::naming::violations_for(file, source);
        if violations.is_empty() {
            return;
        }
        let claimed = claims(&violations);

        for violation in violations {
            if cancelled() {
                return;
            }
            let line = line_of(source, violation.start);
            if non_compliant {
                self.refuse(file, line, &violation, NON_COMPLIANT_ENCODING.to_string());
                continue;
            }
            if let Some(reason) = collision(&violation, &claimed, source) {
                self.refuse(file, line, &violation, reason);
                continue;
            }
            // A type costs a pass over the project to plan, so it is only CLASSIFIED here.
            if violation.target == Target::Type {
                match IndexService::global().classify_type(file, source, violation.start) {
                    Some(binary) => self.pending_types.push(PendingType {
                        file: file.to_string(),
                        line,
                        violation,
                        binary,
                    }),
                    None => self.refuse(file, line, &violation, nothing_to_rename(violation.target)),
                }
                continue;
            }
            let planned = rename_edits(file, source, &violation);
            // The engine refusing outright is not the same as finding nothing: it means applying
            // would break the code, and the review has to say which.
            if let Some(reason) = planned.blocked {
                self.refuse(file, line, &violation, reason);
                continue;
            }
            self.accept(file, line, &violation, planned.edits);
        }
    }

    /// Phase two: every pending type, in one pass over the project's sources.
    fn plan_types(&mut self, root: &str, on_file: &dyn Fn(usize, usize) -> bool) {
        let renames: Vec<TypeRename> = self
            .pending_types
            .iter()
            .map(|p| TypeRename {
                binary: p.binary.clone(),
                new_name: p.violation.suggested.clone(),
            })
            .collect();
        let (buckets, completed) =
            IndexService::global().plan_type_renames(root, &renames, on_file);

        let pending = std::mem::take(&mut self.pending_types);

        // A stopped pass leaves every bucket half-built: the *references* to a type come from the
        // index and are all there, but its declaration and imports come from the walk that was
        // interrupted. Applying that renames a type's call sites and leaves `class Foo` alone —
        // code that no longer compiles. So a stop refuses every type outright rather than offering
        // a partial plan that looks applicable.
        if !completed {
            for p in &pending {
                self.refuse(&p.file, p.line, &p.violation, STOPPED_BEFORE_PLANNED.to_string());
            }
            self.pending_types = pending;
            return;
        }

        // `plan_type_renames` answers one bucket per input, in order — so a bucket that is missing
        // (an engine still building answers with nothing at all) is a type refused, not a silent
        // success.
        for (i, p) in pending.iter().enumerate() {
            let edits: Vec<RenameEdit> = buckets
                .get(i)
                .map(|es| es.iter().cloned().map(crate::rename::wire_edit).collect())
                .unwrap_or_default();
            self.accept(&p.file, p.line, &p.violation, edits);
        }
        self.pending_types = pending;
    }

    fn accept(&mut self, file: &str, line: usize, violation: &Violation, edits: Vec<RenameEdit>) {
        // Renaming a public top-level type without its file leaves code that does not compile, so
        // the move travels with the name that causes it. `file_rename_for` answers `None` for
        // anything whose file is not named after it — a member, a local, a nested type.
        let file_rename = bennu_intel::prelude::file_rename_for(
            file,
            &violation.name,
            &violation.suggested,
        )
        .map(|r| bennu_proto::prelude::RenameFileMove { from: r.from, to: r.to });
        if edits.is_empty() {
            self.refuse(file, line, violation, nothing_to_rename(violation.target));
            return;
        }
        // Every planner marks the definition site `declaration` — a local's declarator, a member's
        // signature, a type's name token. A plan without one rewrites the *uses* of something and
        // leaves what it is called alone, which does not compile.
        //
        // This is a structural check, not a check for one bug: it caught a `record` whose uses were
        // renamed and whose declaration was not, because the walk that finds declarations did not
        // list `record_declaration` among the kinds it recognised. Any future grammar the engine
        // half-knows fails the same way, and fails safely here instead of in the user's source.
        if !edits.iter().any(|e| e.reason == "declaration") {
            self.refuse(file, line, violation, NO_DECLARATION_EDIT.to_string());
            return;
        }
        // The SAME rename reached twice is not a conflict — it is one rename, seen from its second
        // declaration. Java overloads share a name by definition, and the engine renames the whole
        // set from any one of them, so the scan meets `foo(int)` and `foo(String)` as two
        // violations that plan the identical edits. Refusing the second reported a collision
        // between a method and itself (106 of 512 refusals on a real project); dropping it silently
        // is right, because the rename the user asked for is already in the plan.
        if self.already_planned(violation, &edits) {
            return;
        }
        if let Some(clash) = self.overlaps(&edits) {
            self.refuse(
                file,
                line,
                violation,
                format!("its edits overlap a rename already planned in {clash}"),
            );
            return;
        }
        for edit in &edits {
            self.taken.entry(edit.file.clone()).or_default().push((edit.start, edit.end));
        }
        self.plan.renamed.push(RenamedName {
            file: file.to_string(),
            line,
            from: violation.name.clone(),
            to: violation.suggested.clone(),
            target: violation.target.to_string(),
            file_rename,
            edits,
        });
    }

    fn refuse(&mut self, file: &str, line: usize, violation: &Violation, reason: String) {
        self.plan.refused.push(FixRefusal {
            file: file.to_string(),
            line,
            name: violation.name.clone(),
            reason,
        });
    }

    /// The file of the first already-planned range that overlaps one of `fresh`, if any.
    /// Whether an accepted rename already covers exactly this one — same old name, same new name,
    /// and the same set of edit sites. Deliberately strict: only an identical plan is a duplicate,
    /// so two genuinely different renames that happen to touch one shared byte range still reach
    /// the overlap check and are still refused.
    fn already_planned(&self, violation: &Violation, fresh: &[RenameEdit]) -> bool {
        self.plan.renamed.iter().any(|done| {
            done.from == violation.name
                && done.to == violation.suggested
                && done.edits.len() == fresh.len()
                && done
                    .edits
                    .iter()
                    .zip(fresh)
                    .all(|(a, b)| a.file == b.file && a.start == b.start && a.end == b.end)
        })
    }

    fn overlaps(&self, fresh: &[RenameEdit]) -> Option<String> {
        for edit in fresh {
            let Some(ranges) = self.taken.get(&edit.file) else { continue };
            if ranges.iter().any(|(start, end)| *start < edit.end && edit.start < *end) {
                return Some(edit.file.clone());
            }
        }
        None
    }
}

/// Read a project source **the way every offset in this product is measured against**.
///
/// `std::fs::read_to_string` is wrong here twice over, and both ways corrupt code rather than
/// failing loudly:
///
/// * It is **UTF-8 only**. Legacy trees are frequently Cp1252, where it either fails or mangles.
/// * It keeps **CRLF**. The index, the semantic engine and the editor's buffer all work on
///   LF-normalized text (see `bennu_project::normalize_newlines`), so an offset computed against
///   the CRLF bytes lands one byte further along for every line before it. Applied, that is a
///   rename spliced into the middle of whatever happens to sit twenty-odd bytes later — a comment,
///   a string, the wrong identifier.
///
/// So: read bytes, decode in the project's declared encoding, normalize. The same chain the index
/// build uses, because a second way of reading a file is a second set of offsets.
/// One source file, read the way the index reads it.
struct ScannedSource {
    text: String,
    /// The file's bytes are not valid in the project's declared encoding, so this text is a
    /// RECOVERY — see [`read_project_source`] for why that makes it unrenameable.
    non_compliant: bool,
}

/// Read `file` exactly as the index does — the project's declared encoding, newlines normalised —
/// so every offset in this plan means what it means everywhere else.
///
/// `encoding` is resolved ONCE per run by the caller, not per file: deriving it reads the bennu
/// config and parses `pom.xml`, and doing that for each of a few hundred files is a thousand file
/// opens to arrive at the same constant string every time.
///
/// ## Why the compliance flag has to come back out
///
/// The index and the editor decode a mislabelled file DIFFERENTLY, and neither is wrong for its
/// own purpose. `decode_for_index` recovers — it retries UTF-8, then Windows-1252 — so a class is
/// still indexed rather than dropped. `decode`, which the editor reads through, is lossy: a byte
/// that doesn't fit becomes U+FFFD.
///
/// Those two texts are not the same length. U+FFFD is three UTF-8 bytes where the original may
/// have been one, so every offset after the first bad byte drifts — and a plan computed here would
/// be applied against text it was not computed from. The apply path's per-edit check catches it
/// and drops those edits, which is the safe outcome and a baffling one to read: "some edits did
/// not match" with nothing to say why.
fn read_project_source(file: &str, encoding: &str) -> Option<ScannedSource> {
    let bytes = std::fs::read(file).ok()?;
    let decoded = bennu_project::prelude::decode_for_index(&bytes, encoding);
    Some(ScannedSource {
        text: bennu_project::prelude::normalize_newlines(&decoded.text),
        non_compliant: decoded.non_compliant,
    })
}

/// The 1-based line `offset` falls on.
fn line_of(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())].bytes().filter(|b| *b == b'\n').count() + 1
}

const NON_COMPLIANT_ENCODING: &str =
    "this file is not valid in the project's declared encoding — the editor and the index read it \
     differently, so an edit planned here would land on the wrong bytes. Fix its encoding first";

/// Why the engine came back with no edits for `target`.
///
/// One sentence used to cover every case, and it named the wrong one: on a real project **850 of
/// 1001 refusals** read "the index may still be building" with the index built minutes earlier.
/// They were package segments — `package it.foo.portale_appalti;` breaks a `lowercase` rule once
/// per FILE, so a 750-file project produces 750 of them — and no engine renames a package with an
/// edit, because a package's name is the directory it lives in. A refusal reason is the only thing
/// the user has to act on; pointing it at the wrong cause sends them to look at the wrong thing.
fn nothing_to_rename(target: Target) -> String {
    match target {
        Target::Package => "a package segment is not renamed by editing text — its name IS the \
             directory holding the file. Move the files (Project tree → Move) and the package \
             declarations follow"
            .to_string(),
        _ => "the engine found no editable site for this declaration — it resolved to a symbol the \
              project index does not own, or the index is still building"
            .to_string(),
    }
}

const NO_DECLARATION_EDIT: &str =
    "the rename would rewrite its uses but not its declaration — refused as incomplete";

const STOPPED_BEFORE_PLANNED: &str =
    "stopped before this type could be planned — renaming a type is all-or-nothing";

// ── collisions ──────────────────────────────────────────────────────────────────

/// How many names in this file want each spelling.
fn claims(violations: &[Violation]) -> HashMap<String, HashSet<(Target, String)>> {
    let mut claimed: HashMap<String, HashSet<(Target, String)>> = HashMap::new();
    for violation in violations {
        claimed
            .entry(violation.suggested.clone())
            .or_default()
            .insert((violation.target, violation.name.clone()));
    }
    claimed
}

/// Why this rename must not happen, if it must not.
fn collision(
    violation: &Violation,
    claimed: &HashMap<String, HashSet<(Target, String)>>,
    source: &str,
) -> Option<String> {
    // Two declarations wanting the same spelling only clash if they share a namespace. Locals and
    // parameters do not: a variable called `source_directory` in five different methods is five
    // disjoint scopes, and renaming each to `sourceDirectory` is exactly right. Counting those as
    // a collision refused every one of them — the common case on legacy code, and the reason this
    // rule now asks *which kind* of declaration it is looking at.
    //
    // And what is counted is DISTINCT names, not occurrences. Java overloads share one name by
    // definition — five `lista_soggetti_sovrapposti(…)` are five declarations of ONE method, and
    // renaming them together is the only correct answer. Counting each occurrence made every
    // overloaded method in the project refuse itself for colliding with itself: on a real legacy
    // tree that was 213 of 629 refusals, the second-largest reason, all of them wrong.
    if !violation.file_local
        && claimed.get(&violation.suggested).map(HashSet::len).unwrap_or(0) > 1
    {
        return Some(format!(
            "more than one name in this file would become `{}`",
            violation.suggested
        ));
    }
    // Renaming onto a name the file already uses is how a bulk fix turns compiling code into two
    // members with one signature. Conservative on purpose: an unrelated occurrence blocks it, and
    // the refusal says which name, so the user can do that one by hand.
    if code_declares_word(source, &violation.suggested) {
        return Some(format!("`{}` already appears in this file", violation.suggested));
    }
    None
}

/// Whether the suggested spelling already occurs in this file's **code** as an unqualified word.
///
/// Three exclusions, each one a false refusal measured on a real project (127 of 629 there):
///
/// * **String literals and comments.** `prop.get("listaNature.sociRichiesti")` is a config key, not
///   a declaration; refusing a rename because a string mentions the word blocks a fix for a reason
///   that does not exist in the language.
/// * **Qualified uses.** `pdnd_import_service.importPdndImpresa(…)` is a method of ANOTHER type.
///   Nothing in this file can collide with it. A declaration is never preceded by a `.`, so
///   dropping qualified occurrences keeps every case this check exists for.
/// * **Nothing else.** An unqualified bare word could still be a use rather than a declaration, and
///   that stays refused: without scope analysis, being wrong in that direction only costs a rename
///   the user can do by hand, while being wrong the other way writes code that does not compile.
fn code_declares_word(source: &str, needle: &str) -> bool {
    contains_unqualified_word(&strip_literals_and_comments(source), needle)
}

/// The source with comments and string / char literals blanked to spaces — same length, so any
/// offset computed against the result still refers to the same place in the original.
fn strip_literals_and_comments(source: &str) -> String {
    let b = source.as_bytes();
    let mut out: Vec<u8> = b.to_vec();
    let mut i = 0usize;
    // Blank `b[i..end]`, keeping newlines so line structure (and any offset) survives.
    let mut blank = |out: &mut Vec<u8>, from: usize, to: usize| {
        for x in out[from..to].iter_mut() {
            if *x != b'\n' {
                *x = b' ';
            }
        }
    };
    while i < b.len() {
        match b[i] {
            b'/' if i + 1 < b.len() && b[i + 1] == b'/' => {
                let end = b[i..].iter().position(|c| *c == b'\n').map(|p| i + p).unwrap_or(b.len());
                blank(&mut out, i, end);
                i = end;
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'*' => {
                let mut end = i + 2;
                while end + 1 < b.len() && !(b[end] == b'*' && b[end + 1] == b'/') {
                    end += 1;
                }
                let end = (end + 2).min(b.len());
                blank(&mut out, i, end);
                i = end;
            }
            q @ (b'"' | b'\'') => {
                let mut end = i + 1;
                while end < b.len() && b[end] != q {
                    // A backslash escapes the next byte, including the closing quote.
                    end += if b[end] == b'\\' { 2 } else { 1 };
                }
                let end = (end + 1).min(b.len());
                blank(&mut out, i, end);
                i = end;
            }
            _ => i += 1,
        }
    }
    // Every replacement is a single ASCII byte for a single byte, so this cannot split a
    // multi-byte character — but take the lossless path rather than assert it.
    String::from_utf8(out).unwrap_or_else(|_| source.to_string())
}

/// Whether `needle` occurs in `haystack` as a whole identifier that is not member-qualified.
fn contains_unqualified_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let bytes = haystack.as_bytes();
    let mut from = 0;
    while let Some(hit) = haystack[from..].find(needle) {
        let start = from + hit;
        let end = start + needle.len();
        let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
        let after_ok = end == bytes.len() || !is_ident_byte(bytes[end]);
        // `recv.name` / `Type::name` belongs to whatever is on the left, not to this file.
        // A declaration never has one in front of it.
        let qualified = bytes[..start]
            .iter()
            .rposition(|b| !b.is_ascii_whitespace())
            .map(|p| bytes[p] == b'.' || (bytes[p] == b':' && p > 0 && bytes[p - 1] == b':'))
            .unwrap_or(false);
        if before_ok && after_ok && !qualified {
            return true;
        }
        from = start + 1;
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

// ── walking ─────────────────────────────────────────────────────────────────────

/// Every file under `dir` that some pack claims, up to `budget`.
///
/// Reuses the walk's skip list rather than a second one: a bulk fix must visit exactly the files
/// the check visits, or it would report violations it then declines to touch.
fn collect_checkable(dir: &std::path::Path, out: &mut Vec<String>, budget: &mut usize) {
    if *budget == 0 || cancelled() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if *budget == 0 || cancelled() {
            return;
        }
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if crate::find::SKIP_DIRS.contains(&name) {
                continue;
            }
            collect_checkable(&path, out, budget);
        } else if let Some(name) = path.to_str() {
            if bennu_naming::prelude::pack_for_path(name).is_some() {
                *budget -= 1;
                out.push(name.to_string());
            }
        }
    }
}

/// The edits that renaming `violation` would make — the same routing `bennu_rename_apply` uses, so
/// a bulk fix and a single one can never disagree about what a rename means.
///
/// Not for a type: those go through the batch, because this call would cost a pass over the
/// project's sources every time.
/// What planning one violation's rename came back with.
struct PlannedRename {
    edits: Vec<RenameEdit>,
    /// Set when the rename must not be applied at all — the engine's own refusal, carried through
    /// so the review can show the reason rather than a bare "nothing to rename".
    blocked: Option<String>,
}

fn rename_edits(file: &str, source: &str, violation: &Violation) -> PlannedRename {
    if let Some(edits) =
        crate::lsp_route::rename_apply(file, source, violation.start, &violation.suggested)
    {
        return PlannedRename { edits, blocked: None };
    }
    match IndexService::global().plan_rename(file, source, violation.start, &violation.suggested) {
        Some(plan) => PlannedRename {
            blocked: plan.blocked.clone(),
            edits: plan
                .files
                .into_iter()
                .flat_map(|f| f.edits)
                .map(crate::rename::wire_edit)
                .collect(),
        },
        None => PlannedRename { edits: Vec::new(), blocked: None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_word_matching_ignores_substrings() {
        assert!(contains_unqualified_word("int getUserName = 1;", "getUserName"));
        assert!(contains_unqualified_word("getUserName()", "getUserName"));
        // A longer identifier that merely contains it is not it.
        assert!(!contains_unqualified_word("getUserNameLong()", "getUserName"));
        assert!(!contains_unqualified_word("myGetUserName()", "getUserName"));
        assert!(!contains_unqualified_word("a_getUserName", "getUserName"));
        assert!(!contains_unqualified_word("", "getUserName"));
    }

    #[test]
    fn a_member_of_another_object_is_not_a_clash() {
        // `service.importThing()` is a method of whatever `service` is. Nothing in this file can
        // collide with it, and refusing on it blocked 127 renames on a real project.
        assert!(!contains_unqualified_word("return service.getUserName();", "getUserName"));
        assert!(!contains_unqualified_word("Helper::getUserName", "getUserName"));
        // A declaration is never preceded by a dot, so the case this check exists for survives.
        assert!(contains_unqualified_word("String getUserName() { return x; }", "getUserName"));
    }

    #[test]
    fn a_word_inside_a_string_or_comment_is_not_a_clash() {
        assert!(!code_declares_word(r#"prop.get("listaNature.sociRichiesti");"#, "sociRichiesti"));
        assert!(!code_declares_word("// renamed from sociRichiesti one day", "sociRichiesti"));
        assert!(!code_declares_word("/* sociRichiesti */ int x;", "sociRichiesti"));
        assert!(code_declares_word("boolean sociRichiesti = true;", "sociRichiesti"));
        // An escaped quote must not end the literal early and expose what follows.
        assert!(!code_declares_word(r#"String s = "a\" sociRichiesti";"#, "sociRichiesti"));
    }

    #[test]
    fn blanking_preserves_length_and_lines() {
        let src = "int a; // hey\nString s = \"xx\";\n";
        let out = strip_literals_and_comments(src);
        assert_eq!(out.len(), src.len(), "offsets must stay valid");
        assert_eq!(out.matches('\n').count(), src.matches('\n').count());
        assert!(out.starts_with("int a; "));
        assert!(!out.contains("hey"));
        assert!(!out.contains("xx"));
    }

    #[test]
    fn overloads_are_one_rename_not_a_collision() {
        // Two declarations of `lista_soggetti(…)` are two overloads of ONE method: they share a
        // name by definition and the engine renames them together. Counting each occurrence made
        // every overloaded method refuse itself — 213 refusals on a real project, all wrong.
        let overloads = vec![
            violation_of(Target::Method, "lista_soggetti", "listaSoggetti", false),
            violation_of(Target::Method, "lista_soggetti", "listaSoggetti", false),
        ];
        let claimed = claims(&overloads);
        assert!(collision(&overloads[0], &claimed, "").is_none());
        assert!(collision(&overloads[1], &claimed, "").is_none());
    }

    #[test]
    fn overlap_is_detected_per_file_and_per_range() {
        let edit = |file: &str, start: usize, end: usize| RenameEdit {
            file: file.to_string(),
            start,
            end,
            new_text: "x".to_string(),
            old: "y".to_string(),
            reason: "declaration".to_string(),
            inferred: false,
        };
        let mut run = Run::default();
        run.taken.insert("A.java".to_string(), vec![(10, 20)]);
        assert!(run.overlaps(&[edit("A.java", 15, 25)]).is_some());
        // Touching but not overlapping is fine — `[10,20)` and `[20,30)` are disjoint.
        assert!(run.overlaps(&[edit("A.java", 20, 30)]).is_none());
        // Same range in a different file is a different edit.
        assert!(run.overlaps(&[edit("B.java", 15, 25)]).is_none());
    }

    fn violation_of(target: Target, name: &str, suggested: &str, file_local: bool) -> Violation {
        Violation {
            target,
            convention: bennu_naming::prelude::Convention::Camel,
            name: name.to_string(),
            suggested: suggested.to_string(),
            start: 0,
            end: 0,
            file_local,
        }
    }

    #[test]
    fn two_members_wanting_one_spelling_are_both_refused() {
        let both = vec![
            violation_of(Target::Method, "get_user", "getUser", false),
            violation_of(Target::Method, "getUser_", "getUser", false),
        ];
        let claimed = claims(&both);
        // Neither may proceed: whichever went first, the second would rename onto it.
        assert!(collision(&both[0], &claimed, "").is_some());
        assert!(collision(&both[1], &claimed, "").is_some());
    }

    #[test]
    fn same_named_locals_in_one_file_are_not_a_collision() {
        // Five methods each with a `source_directory` — five disjoint scopes, five valid renames.
        // Treating them as one collision refused all of them, which is what a legacy file looks
        // like and what made the whole feature look broken.
        let locals: Vec<Violation> = (0..5)
            .map(|_| violation_of(Target::Local, "source_directory", "sourceDirectory", true))
            .collect();
        let claimed = claims(&locals);
        // One distinct name, however many declarations spell it.
        assert_eq!(claimed.get("sourceDirectory").map(HashSet::len), Some(1));
        for local in &locals {
            assert!(collision(local, &claimed, "int source_directory = 1;").is_none());
        }
    }

    #[test]
    fn a_local_whose_new_spelling_already_exists_is_still_refused() {
        // Scope-exact does not mean safe: renaming a local onto a name the file already uses can
        // shadow a field, and a bare reference in that method would silently change meaning.
        let local = violation_of(Target::Local, "source_directory", "sourceDirectory", true);
        let claimed = claims(std::slice::from_ref(&local));
        assert!(collision(&local, &claimed, "private String sourceDirectory;").is_some());
    }

    #[test]
    fn a_spelling_already_in_the_file_is_refused() {
        let violation = violation_of(Target::Method, "get_user", "getUser", false);
        let claimed = claims(std::slice::from_ref(&violation));
        assert!(collision(&violation, &claimed, "void getUser() {}").is_some());
        assert!(collision(&violation, &claimed, "void somethingElse() {}").is_none());
    }
}
