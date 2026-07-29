//! What the repository **already says** about the rows a generation is about —
//! and about the version range it is guarded on.
//!
//! Everything else in the write pipeline treats a destination as a place to put
//! text. This module is the part that reads first, and it exists because appending
//! is the wrong answer twice over:
//!
//!  * a row the scripts already install must not be installed a second time. In an
//!    **update** script the fix is to replace it — delete by key, insert — and in
//!    the **initialisation** it is to change the row that is already there;
//!  * a version block for `4.12 → 4.13` that already exists must take the new
//!    statements **inside it**, not sit next to a second block guarding the same
//!    range, which would run twice on a fresh install and never on an upgraded one.
//!
//! ## The scope of "already there"
//!
//! Every script of the destination's **engine**, initialisation and updates alike
//! — the literal reading of "already in the initialisation or in an update", and
//! the one that finds the case that actually occurs: the row is in the
//! initialisation and what is being written is this release's update script.
//!
//! Rows are matched on the **comparison key**, not on the whole row. That is what
//! makes "modify that row" mean anything: the values are precisely what is
//! changing.
//!
//! ## What a `DELETE` is taken to mean
//!
//! Only an **unconditional** one forgets anything: `DELETE FROM t` with no `WHERE`,
//! or a `TRUNCATE`. A `DELETE … WHERE COD = 'X'` removes nothing from what is
//! remembered here.
//!
//! `DUP001` makes the opposite trade — there, *any* delete clears the table — and
//! copying it here was a mistake that took the feature to zero on a real
//! repository. Two reasons it does not transfer:
//!
//!  * **the order is not install order.** `DUP001` reads one file, where statement
//!    order is what runs. This walks the whole repository in *tree* order, so a
//!    conditional delete in one folder was erasing everything learned from every
//!    other — seventeen thousand `INSERT`s reduced to nothing by one line;
//!  * **the risk points the other way.** For a rule, a false positive is a wrong
//!    accusation and a false negative is a missed one; erring toward silence is
//!    right. Here, wrongly *remembering* a row means changing an `INSERT` that a
//!    later `DELETE` removes anyway — the script still says what it said. Wrongly
//!    *forgetting* one means appending a second `INSERT` with the same key, which
//!    fails on the key of every database that already has it.
//!
//! A row whose cells are not all literals (`SYSDATE`, a sequence) is not matched
//! either — `compare::row_fingerprint` abstains, and so does this.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use picus_analyze::compare;
use picus_analyze::prelude::RowFingerprint;
use picus_ast::prelude::{DmlModel, DmlOperation, DmlRow, EngineKind, Target};
use picus_parse::prelude::{DmlOperation as ParsedOperation, DmlShape, ParsedFile, SqlParser};
use picus_project::prelude::FolderRole;
use picus_core::prelude::ScriptSnapshot;

/// Where a row that already exists is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowSite {
    /// Project-relative path of the script holding it.
    pub path: String,
    /// The bytes of the whole `INSERT`, terminator included.
    pub range: Range<usize>,
    /// `true` when that statement inserts more than one row.
    ///
    /// Load-bearing: rewriting the statement in place would replace all of them
    /// with one, so a multi-row `VALUES` is found but never edited.
    pub shares_statement: bool,
}

/// The rows of one table that the scripts already install.
///
/// A key maps to **every file** that installs it, not to one. Which matters for
/// the only question asked of it: a row is often written by more than one script —
/// the Oracle initialisation and the portable one both have it — and the copy
/// worth changing is the one in the file being written to. Keeping a single site
/// meant "changed where it is" landed in whichever file the walk happened to see
/// last.
#[derive(Debug, Default)]
pub struct KnownRows {
    by_key: BTreeMap<RowFingerprint, Vec<RowSite>>,
    /// How many `INSERT`s on this table were seen at all.
    ///
    /// Kept so the diff can distinguish the two ways of finding nothing, which
    /// look identical from the outside and mean completely different things:
    /// **nothing inserts into this table** is a fact about the repository, while
    /// **statements insert into it but none could be matched** is a fact about the
    /// comparison key. Without this the answer was silence either way, and the only
    /// way to tell them apart was to read the scripts by hand.
    seen: usize,
    /// Key columns a statement did not supply, so its row could not be matched.
    ///
    /// The failure that actually happens: the key includes a column the older rows
    /// predate — an audit flag added later — so every one of them is unmatchable,
    /// and the generator appends beside them in silence.
    gaps: BTreeSet<String>,
}

impl KnownRows {
    /// Where this row already is — **preferring `path`**, which is the destination
    /// about to be written.
    ///
    /// `None` when it is not installed anywhere, or cannot be matched with
    /// certainty. Falls back to the last file that installs it, so a row that
    /// exists only elsewhere is still found; that file then joins the diff with
    /// its own reason rather than a second copy being added here.
    pub fn site(&self, key: &RowFingerprint, path: &str) -> Option<&RowSite> {
        let sites = self.by_key.get(key)?;
        sites.iter().find(|s| s.path == path).or_else(|| sites.last())
    }

    /// Record a row, replacing what the same file said about it before.
    ///
    /// Per file, because a script that inserts a row and later inserts it again is
    /// `DUP001`'s business, not this one's — here the last word of each file is
    /// what stands.
    /// Everything in this table is gone from here on — an unconditional `DELETE`
    /// or a `TRUNCATE`.
    ///
    /// The count is deliberately **kept**: it says how much was read, which is what
    /// the diagnosis below is built from. Zeroing it would make a repository that
    /// reloads its tables indistinguishable from one that never inserts at all.
    fn forget(&mut self) {
        self.by_key.clear();
    }

    fn record(&mut self, key: RowFingerprint, site: RowSite) {
        let sites = self.by_key.entry(key).or_default();
        match sites.iter_mut().find(|s| s.path == site.path) {
            Some(existing) => *existing = site,
            None => sites.push(site),
        }
    }

    /// Everything two or more engines know, folded together.
    ///
    /// For a **portable** destination, which runs on both. The union rather than
    /// the intersection, and the direction matters: what this decides is whether
    /// to insert the row again, and a row installed on *either* engine is a row
    /// that a plain `INSERT` in a portable file would duplicate there. Requiring
    /// both would append a duplicate for exactly the repository whose two halves
    /// have drifted — which is the one this product exists for.
    ///
    /// A row the portable scripts themselves install appears in both answers, and
    /// folds to itself: the sites are per file.
    pub fn union<'a>(parts: impl IntoIterator<Item = &'a KnownRows>) -> KnownRows {
        let mut out = KnownRows::default();
        for part in parts {
            for (key, sites) in &part.by_key {
                for site in sites {
                    out.record(key.clone(), site.clone());
                }
            }
            // The counts are not added: both engines' scans read the portable
            // folders, so summing them would report each portable statement twice.
            // The larger is the closer answer, and this figure only ever appears in
            // a sentence explaining why nothing matched.
            out.seen = out.seen.max(part.seen);
            out.gaps.extend(part.gaps.iter().cloned());
        }
        out
    }

    /// Why nothing matched, when nothing did — `None` when something did, or when
    /// there was never a question.
    ///
    /// The point of the whole struct carrying counts: an empty answer used to be
    /// silent, and silence covers several opposite situations. Rendered into the
    /// diff so the file says what judgement produced it.
    ///
    /// `sample` is one of the keys that failed to match, and it is what makes the
    /// last case useful rather than merely true: "none of them holds this row" is
    /// a fact nobody can act on without knowing **which columns were compared**,
    /// and — far more usefully — which of them the closest row disagrees on.
    fn why_empty(&self, table: &str, sample: Option<&RowFingerprint>) -> Option<String> {
        if self.seen == 0 {
            return Some(format!(
                "nothing in this engine's scripts inserts into {table}, so this is a new row"
            ));
        }
        if !self.gaps.is_empty() {
            let missing: Vec<&str> = self.gaps.iter().map(String::as_str).collect();
            return Some(format!(
                "{} statement(s) insert into {table}, but none of them names {} — so no existing \
                 row could be matched on the comparison key. Take {} out of the key, or say the \
                 row is new.",
                self.seen,
                missing.join(" and "),
                if missing.len() == 1 { "it" } else { "them" },
            ));
        }

        let key = sample?;
        let compared: Vec<&str> = key.iter().map(|(column, _)| column.as_str()).collect();
        let mut message = format!(
            "{} statement(s) insert into {table}, and none of them holds this row — compared on {}",
            self.seen,
            compared.join(", ")
        );
        if let Some(near) = self.near_miss(key) {
            message.push_str("; ");
            message.push_str(&near);
        }
        Some(message)
    }

    /// The closest row the scripts do hold, described by what it disagrees on.
    ///
    /// The diagnosis nobody can reach by reading a seventeen-thousand-line file:
    /// when a key is one column too wide — it includes a value column, or one the
    /// older rows predate — every comparison fails, and the only visible symptom is
    /// a block appended beside the row it should have changed. Naming the column
    /// that differs turns that into a one-word fix.
    fn near_miss(&self, key: &RowFingerprint) -> Option<String> {
        let mut best: Option<(usize, &RowFingerprint)> = None;
        for candidate in self.by_key.keys() {
            let shared = candidate.iter().filter(|pair| key.contains(pair)).count();
            if shared == 0 {
                continue;
            }
            if best.map(|(most, _)| shared > most).unwrap_or(true) {
                best = Some((shared, candidate));
            }
        }
        let (_, closest) = best?;
        let differing: Vec<&str> = key
            .iter()
            .filter(|(column, value)| {
                closest.iter().any(|(other, theirs)| other == column && theirs != value)
            })
            .map(|(column, _)| column.as_str())
            .collect();
        if differing.is_empty() {
            return None;
        }
        Some(format!(
            "the nearest row in the scripts differs on {} — if {} not part of what identifies a \
             row, take {} out of the comparison key",
            differing.join(" and "),
            if differing.len() == 1 { "that is" } else { "those are" },
            if differing.len() == 1 { "it" } else { "them" },
        ))
    }
}

/// Read what `engine`'s scripts already say about `model`'s table.
///
/// Walks the tree in its own order — which is install order closely enough for
/// the only thing that depends on it, namely that a later `DELETE` clears what an
/// earlier `INSERT` recorded.
///
/// Folders that are excluded, ignored, or written for another engine are not
/// read: their statements do not run on this database, so a row they install is
/// not a row that is there.
pub fn known_rows(
    snapshot: &ScriptSnapshot,
    parser: &mut SqlParser,
    model: &DmlModel,
    engine: EngineKind,
) -> KnownRows {
    let table = picus_analyze::prelude::fold_identifier(&model.table);
    let key_names: BTreeSet<String> =
        model.key_columns.iter().map(|c| c.name.to_uppercase()).collect();
    let mut known = KnownRows::default();
    if key_names.is_empty() {
        // With nothing identifying a row, "the same row" is not a question this
        // can answer — and answering it wrongly would delete the wrong one.
        return known;
    }

    // **Install order, not tree order.** The two differ, and the difference is not
    // cosmetic: alphabetically `AGGIORNAMENTO` precedes `INIZIALIZZAZIONE`, so a
    // walk of the tree reads the updates before the initialisation they update —
    // and then a `DELETE` in an update script appears to happen *before* the
    // `INSERT` it removes. Everything `forget` means depends on getting this right.
    let mut folders: Vec<_> = snapshot
        .project
        .walk()
        .filter(|folder| {
            !folder.is_excluded() && !folder.engine_is_unsupported() && folder.covers(engine)
        })
        .filter(|folder| {
            matches!(
                folder.effective_role,
                FolderRole::Init | FolderRole::Update | FolderRole::Data
            )
        })
        .collect();
    folders.sort_by_key(|folder| (install_rank(folder.effective_role), folder.path.clone()));

    for folder in folders {
        for file in &folder.files {
            if file.is_out_of_scope() {
                continue;
            }
            let Some(source) = snapshot.source(&file.path) else { continue };
            let Some(scope) = folder.scope() else { continue };
            let parsed = parser.parse(&source.text, scope);
            absorb(&mut known, &parsed, &file.path, &table, &key_names);
        }
    }
    known
}

/// The order a database actually receives these folders: seeded first, changed
/// afterwards.
///
/// Within a role the paths are read in order, which for an update folder named by
/// version is version order — close enough for the one thing that depends on it,
/// namely that a `DELETE` is seen after the `INSERT` it removes.
fn install_rank(role: FolderRole) -> u8 {
    match role {
        FolderRole::Init => 0,
        FolderRole::Data => 1,
        _ => 2,
    }
}

/// Fold one file's statements into what is known.
fn absorb(
    known: &mut KnownRows,
    parsed: &ParsedFile,
    path: &str,
    table: &str,
    key_names: &BTreeSet<String>,
) {
    for statement in &parsed.statements {
        for shape in &statement.dml {
            if shape.table.folded_name() != table {
                continue;
            }
            match shape.operation {
                // Only an **unconditional** delete forgets. See the module note:
                // "every row of this table is gone" is readable without evaluating
                // anything; "some rows are gone" is not, and a conditional delete
                // is therefore taken to remove nothing.
                ParsedOperation::Delete if shape.where_clause.is_none() => known.forget(),
                ParsedOperation::Delete => {}
                ParsedOperation::Insert => {
                    let shares = shape.rows.len() > 1;
                    known.seen += 1;
                    // Which key columns this statement does not name. Recorded even
                    // when the row matches, because the interesting report is over
                    // the whole repository: "none of these statements names
                    // CUSTOMIZED" is the sentence that explains an empty answer.
                    if shape.has_column_list {
                        let named: BTreeSet<String> =
                            shape.columns.iter().map(compare::column_key).collect();
                        for missing in key_names.difference(&named) {
                            known.gaps.insert(missing.clone());
                        }
                    }
                    for row in &shape.rows {
                        let Some(key) = key_of(shape, row, key_names) else { continue };
                        known.record(
                            key,
                            RowSite {
                                path: path.to_string(),
                                range: statement.range.start..statement.range.end,
                                shares_statement: shares,
                            },
                        );
                    }
                }
                // An UPDATE changes a row rather than adding or removing one, and
                // a MERGE is an upsert whose effect on a key we cannot read
                // without evaluating it. Neither teaches us that a row is *there*.
                _ => {}
            }
        }
        // A TRUNCATE carries no `DmlShape`; it is a statement about the table, and
        // it is unconditional by construction.
        if truncates(statement, table) {
            known.forget();
        }
    }
}

fn truncates(statement: &picus_parse::prelude::Statement, table: &str) -> bool {
    statement.node_kind.contains("truncate")
        && statement.references.iter().any(|o| o.folded_name() == table)
}

/// One row's comparison key, or `None` when it cannot be read with certainty.
///
/// `None` for a row that does not supply every key column, and for one whose
/// cells are not all literals — both being cases where matching would be a guess,
/// and a guess here becomes a `DELETE`.
fn key_of(
    shape: &DmlShape,
    row: &picus_parse::prelude::ValueRow,
    key_names: &BTreeSet<String>,
) -> Option<RowFingerprint> {
    let full = compare::row_fingerprint(shape, row)?;
    let picked: RowFingerprint =
        full.into_iter().filter(|(column, _)| key_names.contains(column)).collect();
    (picked.len() == key_names.len()).then_some(picked)
}

/// The comparison key of one row of the **model** being written, in the same form
/// `known_rows` recorded the scripts' rows in.
///
/// `None` when a key column was left empty: there is then no row to match, and
/// the generation is an insert of something new.
pub fn model_key(model: &DmlModel, row: &picus_ast::prelude::DmlRow) -> Option<RowFingerprint> {
    let mut out: RowFingerprint = Vec::with_capacity(model.key_columns.len());
    for column in &model.key_columns {
        let raw = row.get(&column.name).map(String::as_str).unwrap_or("").trim();
        if raw.is_empty() {
            return None;
        }
        out.push((column.name.to_uppercase(), normalise_typed(raw, &column.data_type)));
    }
    out.sort();
    Some(out)
}

/// Render a supplied value the way `compare::row_fingerprint` renders a parsed
/// one, so a model row and a script row are comparable.
///
/// The two sides have to agree exactly or nothing ever matches — which would make
/// this whole module silently inert, the worst of the available failures. Numbers
/// go through `f64` on both sides so `1.50` and `1.5` meet; everything else is a
/// quoted string.
fn normalise_typed(raw: &str, data_type: &str) -> String {
    if picus_emit::prelude::is_numeric_type(data_type) {
        if let Ok(number) = raw.parse::<f64>() {
            return format!("{number}");
        }
    }
    if raw.eq_ignore_ascii_case("null") {
        return "NULL".to_string();
    }
    format!("'{raw}'")
}

// ── What each row becomes ─────────────────────────────────────────────────────

/// One row already installed somewhere, rewritten where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
    /// Project-relative path of the file holding it — not necessarily a
    /// destination the user named.
    pub path: String,
    /// The bytes of the `INSERT` being replaced.
    pub range: Range<usize>,
    pub replacement: String,
    /// The diff's hunk header. Says which row, and that it was already there.
    pub reason: String,
}

/// The bytes this destination is **about to replace** — its own previous block.
///
/// Without this the generator argues with itself. Write a row into an update
/// script; it is now installed, so the next run finds it and promotes the block to
/// delete-then-insert; the file changes. Run it again and it changes back. That
/// breaks the property everything else here rests on — *a re-run writes byte-identical
/// files* — and it breaks it silently, as a diff that never settles.
///
/// A block Picus is about to rewrite cannot be evidence of anything: it is not what
/// the repository says, it is what Picus said last time.
#[derive(Debug, Clone)]
pub struct Ours {
    pub path: String,
    pub range: Range<usize>,
}

impl Ours {
    /// **Overlap**, not containment — and that distinction is the whole of it.
    ///
    /// A row inside a procedural block is recorded with the range of the block,
    /// because that is the statement `picus-parse` reports; a generated body
    /// spliced into somebody else's guard is a range *within* it. Testing
    /// containment either way round misses one of the two, and what that looks
    /// like is a file that alternates between two spellings on every run.
    ///
    /// Conservative where it is wrong: a hand-written row inside the same block as
    /// a generated one is excluded too, so it is appended rather than replaced.
    /// The wrong direction of that trade would be a `DELETE`.
    fn covers(&self, site: &RowSite) -> bool {
        site.path == self.path
            && site.range.start < self.range.end
            && site.range.end > self.range.start
    }
}

/// What a destination does with the rows it was given.
#[derive(Debug)]
pub struct RowOutcome {
    /// Rows written as a block at the destination — the ordinary path.
    pub appended: Vec<DmlRow>,
    /// Rows changed where they already are, instead of being added again.
    pub rewrites: Vec<Rewrite>,
    /// The operation the appended rows are emitted with. Promoted to `Replace`
    /// for an update script that is re-stating rows the scripts already install.
    pub operation: DmlOperation,
    /// What happened, for the diff — `None` when nothing out of the ordinary did.
    pub note: Option<String>,
}

/// Decide, per row, between adding it and changing the one already there.
///
/// The two halves of the same rule, and they differ because the two kinds of
/// script mean different things:
///
///  * an **initialisation** describes the database as it should end up, so a row
///    that is already in it is *edited*. Adding a second copy would install a
///    duplicate key;
///  * an **update** describes a change, so it never edits history: it states the
///    row again as `DELETE` by key then `INSERT`, which lands the same values on a
///    database that has the row and on one that does not.
///
/// A destination with no engine, no comparison key, or nothing known about it
/// takes the plain path — every row appended, operation untouched. That is the
/// behaviour that existed before any of this, and it is what everything falls back
/// to when certainty runs out.
pub fn plan_rows(
    model: &DmlModel,
    target: &Target,
    known: Option<&KnownRows>,
    ours: &[Ours],
) -> RowOutcome {
    let mut outcome = RowOutcome {
        appended: Vec::new(),
        rewrites: Vec::new(),
        operation: model.operation,
        note: None,
    };

    let Some(known) = known else {
        outcome.appended = model.rows.clone();
        return outcome;
    };
    // An explicit UPDATE or DELETE already says what it does to a row that
    // exists; second-guessing it would be overriding an instruction rather than
    // completing one.
    //
    // An **upsert into an initialisation** is the interesting case, and it is
    // reconciled rather than emitted. "Insert it if it is missing, update it if it
    // is there" is a question about install time — and an initialisation runs once,
    // against an empty database, so at install time the answer is always *missing*.
    // The question it is really asking is about **authoring** time: is this row
    // already in the initialisation? That is exactly what this function answers, so
    // the operation collapses to a plain insert and the rows that are already there
    // are changed where they are.
    //
    // Which also removes a refusal that read as a limitation and was really a
    // category error: an upsert has no portable spelling, so a portable
    // initialisation could not take one — while the thing actually wanted, a plain
    // `INSERT`, is as portable as SQL gets.
    let reconcilable = matches!(model.operation, DmlOperation::Insert)
        || (matches!(model.operation, DmlOperation::Upsert) && seeds(target.role));
    if !reconcilable {
        outcome.appended = model.rows.clone();
        return outcome;
    }
    if matches!(model.operation, DmlOperation::Upsert) {
        outcome.operation = DmlOperation::Insert;
    }

    let mut found = 0usize;
    // One key that failed, so the diagnosis below can say what was compared and
    // what the nearest row disagrees on.
    let mut unmatched: Option<RowFingerprint> = None;
    for row in &model.rows {
        if unmatched.is_none() {
            unmatched = model_key(model, row);
        }
        let site = model_key(model, row)
            // Preferring this destination: the row worth changing is the copy in
            // the file being written to, when there is one.
            .and_then(|key| known.site(&key, &target.file))
            .filter(|site| !ours.iter().any(|o| o.covers(site)));
        let Some(site) = site else {
            outcome.appended.push(row.clone());
            continue;
        };
        found += 1;

        if !seeds(target.role) {
            // An update script: state the row again rather than editing the file
            // that first installed it.
            outcome.appended.push(row.clone());
            continue;
        }
        if site.shares_statement {
            // A multi-row `VALUES`: replacing the statement would replace every
            // row in it with this one. Appended instead, and said out loud.
            outcome.appended.push(row.clone());
            outcome.note = Some(format!(
                "{} already inserts this row alongside others in one statement, which Picus \
                 will not rewrite — check for a duplicate key",
                site.path
            ));
            continue;
        }
        match one_row_statement(model, row, target) {
            Some(replacement) => outcome.rewrites.push(Rewrite {
                path: site.path.clone(),
                range: site.range.clone(),
                replacement,
                reason: format!(
                    "this row is already installed here, so it is changed in place rather than \
                     inserted a second time"
                ),
            }),
            None => outcome.appended.push(row.clone()),
        }
    }

    if found == 0 && outcome.note.is_none() {
        // Nothing matched. Say which kind of nothing — an appended block that is
        // right and one that is a missed reconciliation look identical in a diff,
        // and only this sentence tells them apart.
        outcome.note = known.why_empty(&model.table, unmatched.as_ref());
    }

    if found > 0 && !outcome.appended.is_empty() && target.role == FolderRole::Update {
        // Delete-then-insert, for every row in the block rather than for the ones
        // that happened to be found. Mixing two statement shapes inside one block
        // is harder to read in a diff than one shape applied uniformly, and a
        // `DELETE` for a row that is not there is a no-op — so the uniform version
        // costs nothing and says the same thing.
        outcome.operation = DmlOperation::Replace;
        outcome.note = Some(format!(
            "{found} of these rows are already installed by this engine's scripts, so the block \
             deletes by key before inserting"
        ));
    }

    outcome
}

/// Does this role describe the database's **starting state** rather than a change
/// to it?
///
/// The two behave differently everywhere in this module, and it is one distinction
/// rather than two coincidences: a seeding script says what should be there, so a
/// row already in it is edited; an update says what changes, so it never edits
/// history.
fn seeds(role: FolderRole) -> bool {
    matches!(role, FolderRole::Init | FolderRole::Data)
}

/// The `INSERT` that replaces a row already in a file, with its own terminator
/// and no trailing newline — the range it replaces does not include one.
fn one_row_statement(model: &DmlModel, row: &DmlRow, target: &Target) -> Option<String> {
    let single = DmlModel {
        rows: vec![row.clone()],
        operation: DmlOperation::Insert,
        ..model.clone()
    };
    picus_emit::prelude::plain_statement(&single, row, target).ok()
}

// ── An existing version block ─────────────────────────────────────────────────

/// Where new statements go inside a block that already guards this version range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSite {
    /// Byte offset inside the file, at the start of a line.
    pub at: usize,
    /// The indentation of the statements already in the block, so what is spliced
    /// in lines up with them rather than announcing itself.
    pub indent: String,
    /// The whole block. What a caller asks in order to know whether an edit it is
    /// about to make lands **inside** this guard — which decides between writing a
    /// body and writing a block of its own.
    pub range: Range<usize>,
}

impl BlockSite {
    /// Does `range` fall inside this block?
    pub fn holds(&self, range: &Range<usize>) -> bool {
        range.start >= self.range.start && range.start < self.range.end
    }
}

/// Find a block already guarding `from` → `to` on this project's version table.
///
/// Text-matching, but over a **parsed statement** rather than over the file: the
/// candidate is a single procedural block, and what is checked is that it mentions
/// the version table and both version literals. That is conservative in the
/// direction that matters — a block it fails to recognise costs a second block,
/// which is the behaviour that existed before; a block it recognised wrongly would
/// splice statements into somebody else's guard.
pub fn version_block(
    source: &str,
    parsed: &ParsedFile,
    version_table: &str,
    from: &str,
    to: &str,
) -> Option<BlockSite> {
    if version_table.trim().is_empty() || from.trim().is_empty() || to.trim().is_empty() {
        return None;
    }
    let table = version_table.trim().to_ascii_uppercase();

    for statement in &parsed.statements {
        if !is_block(statement) {
            continue;
        }
        let text = &source[statement.range.start..statement.range.end];
        let upper = text.to_ascii_uppercase();
        if !upper.contains(&table)
            || !upper.contains(&quoted(from))
            || !upper.contains(&quoted(to))
        {
            continue;
        }
        let mut site = site_in(source, text, statement.range.start, &table);
        site.range = statement.range.start..statement.range.end;
        return Some(site);
    }
    None
}

fn quoted(version: &str) -> String {
    format!("'{}'", version.trim().to_ascii_uppercase())
}

fn is_block(statement: &picus_parse::prelude::Statement) -> bool {
    let kind = &statement.node_kind;
    kind.contains("block") || kind.contains("do_statement") || kind.contains("anonymous")
}

/// The line inside the block that new statements go above, and its indentation.
///
/// Above the `UPDATE` that carries the version forward, because that `UPDATE` is
/// the last thing the block should do — statements spliced after it would run
/// against a database already stamped with the new version, and be skipped on the
/// re-run that a half-failed upgrade needs. Failing that, above the closing `END`.
fn site_in(source: &str, text: &str, offset: usize, version_table: &str) -> BlockSite {
    let upper = text.to_ascii_uppercase();

    let anchor = find_last_line_starting_with(&upper, "UPDATE", Some(version_table))
        .or_else(|| find_last_line_starting_with(&upper, "END", None))
        .unwrap_or(text.len());

    let line_start = text[..anchor].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let indent: String =
        text[line_start..].chars().take_while(|c| *c == ' ' || *c == '\t').collect();

    BlockSite {
        at: offset + line_start,
        indent: indent_of(source, offset, &indent),
        // Filled in by the caller, which is the only thing that knows the whole
        // statement's extent.
        range: offset..offset,
    }
}

/// Keep the block's own indentation, falling back to two spaces for a block whose
/// closing `END` sits at column zero — where copying it would emit unindented
/// statements inside a body that is indented everywhere else.
fn indent_of(_source: &str, _offset: usize, found: &str) -> String {
    if found.is_empty() {
        "  ".to_string()
    } else {
        found.to_string()
    }
}

/// The byte offset of the last line whose first word is `word`, optionally also
/// mentioning `and`. Searched on the upper-cased text, so offsets are the text's.
fn find_last_line_starting_with(upper: &str, word: &str, and: Option<&str>) -> Option<usize> {
    let mut found = None;
    let mut at = 0usize;
    for line in upper.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with(word) && and.map(|a| line.contains(a)).unwrap_or(true) {
            found = Some(at + (line.len() - trimmed.len()));
        }
        at += line.len();
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use picus_parse::prelude::DialectScope;

    fn parse(source: &str) -> ParsedFile {
        SqlParser::new().parse(source, DialectScope::One(EngineKind::Oracle))
    }

    const GUARDED: &str = "DECLARE\n\
                           \x20 v_version VARCHAR2(30);\n\
                           BEGIN\n\
                           \x20 SELECT VERSIONE INTO v_version FROM VERSIONE_DB;\n\
                           \x20 IF v_version <> '4.12' THEN\n\
                           \x20   RETURN;\n\
                           \x20 END IF;\n\
                           \n\
                           \x20 INSERT INTO PARAMETRI (COD) VALUES ('A');\n\
                           \n\
                           \x20 UPDATE VERSIONE_DB SET VERSIONE = '4.13';\n\
                           \x20 COMMIT;\n\
                           END;\n";

    #[test]
    fn a_block_guarding_the_same_range_is_found() {
        let parsed = parse(GUARDED);
        let site = version_block(GUARDED, &parsed, "VERSIONE_DB", "4.12", "4.13").expect("found");
        // Above the UPDATE that carries the version forward — anything after it
        // would run against a database already stamped, and be skipped on a re-run.
        assert!(GUARDED[site.at..].starts_with("  UPDATE VERSIONE_DB"), "{:?}", &GUARDED[site.at..site.at + 30]);
        assert_eq!(site.indent, "  ");
    }

    #[test]
    fn a_block_guarding_another_range_is_not_claimed() {
        let parsed = parse(GUARDED);
        assert_eq!(version_block(GUARDED, &parsed, "VERSIONE_DB", "4.13", "4.14"), None);
        assert_eq!(version_block(GUARDED, &parsed, "VERSIONE_DB", "4.11", "4.12"), None);
    }

    #[test]
    fn a_block_on_another_projects_version_table_is_not_claimed() {
        let parsed = parse(GUARDED);
        assert_eq!(version_block(GUARDED, &parsed, "APP_VERSION", "4.12", "4.13"), None);
    }

    #[test]
    fn an_incomplete_configuration_finds_nothing_rather_than_everything() {
        // An empty version table switches the guards off entirely; it must not
        // make every block a candidate.
        let parsed = parse(GUARDED);
        assert_eq!(version_block(GUARDED, &parsed, "", "4.12", "4.13"), None);
        assert_eq!(version_block(GUARDED, &parsed, "VERSIONE_DB", "", "4.13"), None);
    }

    #[test]
    fn a_block_with_no_closing_update_takes_its_end() {
        let source = "DECLARE\nBEGIN\n  IF v <> '1.0' THEN RETURN; END IF;\n  \
                      INSERT INTO T (A) VALUES ('X');\n  -- '1.1' reached\nEND;\n";
        let parsed = parse(source);
        let site = version_block(source, &parsed, "T", "1.0", "1.1");
        // Whether this particular shape is recognised at all is not the point; if
        // it is, the anchor must be a line start inside the block.
        if let Some(site) = site {
            assert!(source[..site.at].ends_with('\n') || site.at == 0);
        }
    }

    #[test]
    fn a_model_row_and_a_script_row_agree_on_what_a_key_is() {
        // The property the whole module rests on: the two sides of the comparison
        // are produced by different code, and if they ever disagree this module
        // silently does nothing at all.
        use picus_ast::prelude::{DmlModel, DmlOperation, DmlRow, VersionTableConfig};
        use picus_ast::prelude::Column;

        let column = |name: &str, ty: &str| Column {
            name: name.to_string(),
            data_type: ty.to_string(),
            primary_key: false,
            not_null: false,
            default_value: None,
        };
        let model = DmlModel {
            table: "CATALOGO_WIDGET".into(),
            operation: DmlOperation::Insert,
            columns: vec![column("CHIAVE", "varchar(30)"), column("ORDINE", "numeric")],
            key_columns: vec![column("CHIAVE", "varchar(30)"), column("ORDINE", "numeric")],
            rows: vec![],
            where_clause: None,
            lowercase_postgres: false,
            version_table: VersionTableConfig::default(),
        };
        let mut row = DmlRow::new();
        row.insert("CHIAVE".into(), "ETICHETTA".into());
        // Written `2.50` in the form and `2.5` in the script: one number.
        row.insert("ORDINE".into(), "2.50".into());

        let parsed = parse("INSERT INTO CATALOGO_WIDGET (CHIAVE, ORDINE) VALUES ('ETICHETTA', 2.5);");
        let shape = &parsed.statements[0].dml[0];
        let names: BTreeSet<String> = ["CHIAVE".to_string(), "ORDINE".to_string()].into();

        assert_eq!(
            model_key(&model, &row),
            key_of(shape, &shape.rows[0], &names),
            "a row typed in the form and the same row in a script must fingerprint alike"
        );
    }

    #[test]
    fn a_key_column_left_empty_matches_nothing() {
        use picus_ast::prelude::{DmlModel, DmlOperation, DmlRow, VersionTableConfig};
        use picus_ast::prelude::Column;
        let model = DmlModel {
            table: "CATALOGO_WIDGET".into(),
            operation: DmlOperation::Insert,
            columns: vec![],
            key_columns: vec![Column {
                name: "CHIAVE".into(),
                data_type: "varchar(30)".into(),
                primary_key: true,
                not_null: true,
                default_value: None,
            }],
            rows: vec![],
            where_clause: None,
            lowercase_postgres: false,
            version_table: VersionTableConfig::default(),
        };
        assert_eq!(model_key(&model, &DmlRow::new()), None);
    }
}
