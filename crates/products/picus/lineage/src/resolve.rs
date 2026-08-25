//! Following a column back through the views until it reaches a table.
//!
//! ## The shape of the walk
//!
//! One column, one step at a time. At each step there is a *current relation* and a
//! *column of it*. If the relation is a table, the trail ends and the verdict is
//! [`Verdict::Resolved`]. If it is a view, its `SELECT` is parsed, the item
//! producing that column is found, and:
//!
//! * a plain reference → resolve its qualifier to a `FROM` item and step there;
//! * a `*` → find the source that carries a column of that name and step there;
//! * anything computed → stop, and report the ingredients.
//!
//! Derived tables and `WITH` names are steps too: they have a projection but no
//! relation of their own, so the walk continues inside them without adding a hop.
//!
//! ## Why the catalogue is a trait
//!
//! Everything above is decision-making over text, and every branch of it deserves a
//! test. A crate that read a database to be tested would have those branches covered
//! by a live view somebody has to keep working — so the database is a
//! [`Catalogue`] the caller implements, and the tests implement it with a map.
//!
//! ## Refusing beats guessing, everywhere
//!
//! A lineage is read in order to decide which table to write to. Every ambiguity is
//! therefore a stop with a reason, never a choice: a bare column name across two
//! sources that both carry it, a view whose definition will not parse, a set
//! operation whose arms disagree. The stop says what was ambiguous, which is a thing
//! the reader can act on; a guess is a thing they cannot even check.

use std::collections::HashSet;

use picus_parse::prelude::{
    project, ColumnSource, DialectScope, EngineKind, FromItem, FromSource, Projected, Projection,
};

use crate::model::{Hop, Ingredient, Lineage, Trace, Verdict};

/// How many views deep the walk will go before giving up.
///
/// Views on views on views is exactly the situation this feature exists for, so the
/// limit is generous. It is a guard against a cycle the catalogue should not be able
/// to contain rather than a judgement about how deep is reasonable.
pub const MAX_DEPTH: usize = 24;

/// What the resolver needs to know about a live database.
///
/// Implemented by the caller — the backend against a session, the tests against a
/// map. Both answers are allowed to be `None`: an object that is not in the
/// catalogue stops the walk with a reason rather than failing it.
pub trait Catalogue {
    /// Is `relation` a view? `None` when the catalogue has never heard of it.
    fn is_view(&self, relation: &str) -> Option<bool>;
    /// The `SELECT` a view is defined as. `None` for a table, or when the
    /// definition could not be read.
    fn definition(&self, view: &str) -> Option<String>;
    /// A relation's column names, folded, in declaration order. Used to expand a
    /// `*` and to attribute a bare column name.
    fn columns(&self, relation: &str) -> Option<Vec<String>>;
}

/// Trace every column of `relation` back to the tables it is read from.
///
/// The relation itself is **not** a hop: the reader is holding it and asking what is
/// behind it, so a chain that opened by naming the thing they already have would put
/// a step in front of the answer.
///
/// A table comes back with no columns traced, which is the true answer — there is
/// nothing behind a table, and inventing a one-hop chain to itself would make an
/// empty result look like a resolved one.
///
/// The **catalogue's** column list drives the walk, not the projection's, so the
/// order and the names are the ones the reader sees in the grid — and a view defined
/// with `SELECT *` still answers for every column it actually has.
pub fn trace_relation(catalogue: &dyn Catalogue, relation: &str, engine: EngineKind) -> Lineage {
    let mut lineage = Lineage { relation: relation.to_string(), ..Default::default() };
    let Some(columns) = catalogue.columns(relation) else {
        return lineage;
    };
    let Some(sql) = catalogue.definition(relation) else {
        return lineage;
    };
    let Some(projection) = project(&sql, DialectScope::One(engine)) else {
        return lineage;
    };

    let mut walk = Walk::new(catalogue, engine);
    for column in columns {
        let mut trace = blank(&column);
        walk.descend(&mut trace, &projection, &column, 0);
        lineage.columns.push(trace);
    }
    lineage.through = walk.visited;
    lineage.truncated = walk.truncated;
    lineage
}

/// A trace that has not gone anywhere yet. Resolved until something says otherwise —
/// the verdict is only ever narrowed, never widened.
fn blank(output: &str) -> Trace {
    Trace {
        output: output.to_string(),
        verdict: Verdict::Resolved,
        hops: Vec::new(),
        reads: Vec::new(),
        stopped: String::new(),
    }
}

/// Trace the columns a statement projects.
///
/// The statement's own `FROM` is the first step, so `SELECT g.codgar1 AS codgar FROM
/// v_elenchi g` traces `codgar` through `v_elenchi` and onward — which is what
/// tracing the *result on screen* means.
pub fn trace_statement(catalogue: &dyn Catalogue, sql: &str, engine: EngineKind) -> Lineage {
    let mut lineage = Lineage::default();
    let Some(projection) = project(sql, DialectScope::One(engine)) else {
        return lineage;
    };
    let mut walk = Walk::new(catalogue, engine);
    lineage.columns = walk.trace_projection(&projection, 0);
    lineage.through = walk.visited;
    lineage.truncated = walk.truncated;
    lineage
}

/// One traversal, carrying what it has learned along the way.
struct Walk<'a> {
    catalogue: &'a dyn Catalogue,
    engine: EngineKind,
    /// Views passed through, in order of first meeting.
    visited: Vec<String>,
    /// The depth limit was reached at least once.
    truncated: bool,
}

/// A step's outcome: where to go next, or why we are not going.
enum Step {
    /// Continue from this relation's column.
    Relation { relation: String, column: String },
    /// Continue inside a projection that has no relation of its own.
    Inside { projection: Projection, column: String },
    /// Stop, with a sentence for the reader.
    Stop(String),
}

impl<'a> Walk<'a> {
    fn new(catalogue: &'a dyn Catalogue, engine: EngineKind) -> Self {
        Self { catalogue, engine, visited: Vec::new(), truncated: false }
    }

    fn note_visit(&mut self, view: &str) {
        if !self.visited.iter().any(|held| held == view) {
            self.visited.push(view.to_string());
        }
    }

    /// Every column a projection produces, traced.
    fn trace_projection(&mut self, projection: &Projection, depth: usize) -> Vec<Trace> {
        let mut out = Vec::new();
        // A statement that is itself a set operation projects nothing of its own; the
        // names come from the first arm and each is resolved across all of them.
        if !projection.arms.is_empty() {
            let names: Vec<String> = projection.arms.first().map_or(Vec::new(), |arm| {
                arm.items.iter().map(|item| item.output().to_string()).collect()
            });
            for name in names.into_iter().filter(|n| !n.is_empty()) {
                let mut trace = blank(&name);
                self.through_union(&mut trace, projection, &name, depth);
                out.push(trace);
            }
            return out;
        }
        for item in &projection.items {
            match item {
                Projected::Star { qualifier } => {
                    // A `*` is as many columns as its source has. Expanding it needs
                    // the catalogue, and a source with no nameable columns simply
                    // contributes none rather than one entry called `*`.
                    for (name, source) in self.star_columns(projection, qualifier.as_deref()) {
                        out.push(self.from_source(&name, projection, &source, depth));
                    }
                }
                Projected::Column { output, source } => {
                    out.push(self.from_source(output, projection, source, depth));
                }
                Projected::Computed { output, reads } => {
                    out.push(Trace {
                        output: output.clone(),
                        verdict: Verdict::Derived,
                        hops: Vec::new(),
                        reads: self.ingredients(projection, reads),
                        stopped: String::new(),
                    });
                }
            }
        }
        out
    }

    /// The columns a `*` stands for, each paired with the reference that reaches it.
    fn star_columns(
        &self,
        projection: &Projection,
        qualifier: Option<&str>,
    ) -> Vec<(String, ColumnSource)> {
        let sources: Vec<&FromItem> = match qualifier {
            Some(name) => projection.from.iter().filter(|f| f.name == name).collect(),
            None => projection.from.iter().collect(),
        };
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for item in sources {
            for column in self.columns_of_item(item) {
                if seen.insert(column.clone()) {
                    out.push((
                        column.clone(),
                        ColumnSource::new(Some(item.name.clone()), column),
                    ));
                }
            }
        }
        out
    }

    /// The column names one `FROM` item offers.
    fn columns_of_item(&self, item: &FromItem) -> Vec<String> {
        if !item.column_aliases.is_empty() {
            return item.column_aliases.clone();
        }
        match &item.source {
            FromSource::Relation { name } => self.catalogue.columns(name).unwrap_or_default(),
            // A derived table's columns are its projection's output names. A `*`
            // inside one cannot be named without descending, and a `*` selecting
            // from a `*` is a shape nobody needs traced today.
            FromSource::Derived { projection } => projection
                .items
                .iter()
                .map(|item| item.output().to_string())
                .filter(|name| !name.is_empty())
                .collect(),
            FromSource::Opaque => Vec::new(),
        }
    }

    /// Trace one output column, given the reference that produces it.
    fn from_source(
        &mut self,
        output: &str,
        projection: &Projection,
        source: &ColumnSource,
        depth: usize,
    ) -> Trace {
        let mut trace = blank(output);
        match self.step_from(projection, source) {
            Step::Relation { relation, column } => self.follow(&mut trace, relation, column, depth),
            Step::Inside { projection, column } => {
                self.descend(&mut trace, &projection, &column, depth)
            }
            Step::Stop(why) => {
                trace.verdict = Verdict::Unresolved;
                trace.stopped = why;
            }
        }
        trace
    }

    /// Where a reference points, within one projection's scope.
    fn step_from(&self, projection: &Projection, source: &ColumnSource) -> Step {
        // A `WITH` name shadows the catalogue: `WITH ordini AS (…) SELECT … FROM
        // ordini` reads the CTE, not the table of that name, and resolving it to the
        // table would be a confident wrong answer about a real database.
        if let Some(name) = source.qualifier.as_deref() {
            if let Some(cte) = projection.ctes.iter().find(|c| c.name == name) {
                return match &cte.projection {
                    Some(inner) => Step::Inside {
                        projection: inner.clone(),
                        column: source.column.clone(),
                    },
                    None => Step::Stop(format!("`{name}` is a WITH clause that is not a query.")),
                };
            }
        }

        // An unqualified name is **never** attributed by elimination, not even when
        // there is a single source. `Projection::source_named(None)` hands back the
        // only `FROM` item without asking whether it has a column of that name, and
        // that shortcut is how a lineage acquires a default table: every unresolved
        // name lands on whichever relation the walk happened to read, wearing its own
        // name, and the page fills with `X ← THAT_TABLE.X`. The catalogue is asked
        // instead — see `attribute_bare`.
        let Some(name) = source.qualifier.as_deref() else {
            return self.attribute_bare(projection, &source.column);
        };
        let Some(item) = projection.from.iter().find(|item| item.name == name) else {
            return Step::Stop(format!(
                "`{name}` is not one of the sources this statement reads from."
            ));
        };
        self.enter(item, &source.column)
    }

    /// A bare column name, attributed only to a source the catalogue says has it.
    ///
    /// The rule is the same whether there is one source or ten: **a relation is named
    /// because it carries the column, never because it was the only one to hand.**
    /// The single-source case is the one that used to skip the check, and it is
    /// precisely the case a broken parse produces — one readable `FROM` item and a
    /// projection nobody could make sense of.
    fn attribute_bare(&self, projection: &Projection, column: &str) -> Step {
        let carriers: Vec<&FromItem> = projection
            .from
            .iter()
            .filter(|item| self.columns_of_item(item).iter().any(|held| held == column))
            .collect();
        match carriers.as_slice() {
            [only] => self.enter(only, column),
            [] => match projection.from.as_slice() {
                // One source whose columns cannot be listed at all — a relation in a
                // schema this connection has not read. Entering it is not a choice
                // between candidates, there being only one, and `follow` then reports
                // the unknown relation by name, which is the accurate complaint. The
                // invention this guard exists to stop needs *candidates*.
                [only] if self.columns_of_item(only).is_empty() => self.enter(only, column),
                _ => Step::Stop(format!(
                    "`{column}` is written without a table, and none of this statement's \
                     sources is known to have a column of that name."
                )),
            },
            many => Step::Stop(format!(
                "`{column}` is written without a table and {} of this statement's sources have a \
                 column of that name, so which one it is cannot be told from the SQL.",
                many.len()
            )),
        }
    }

    /// Step into one `FROM` item.
    fn enter(&self, item: &FromItem, column: &str) -> Step {
        match &item.source {
            FromSource::Relation { name } => {
                Step::Relation { relation: name.clone(), column: column.to_string() }
            }
            FromSource::Derived { projection } => {
                // `AS x(a, b)` renames positionally, so the name inside the derived
                // table is whatever sits at the same position.
                let inner = match item.column_aliases.iter().position(|a| a == column) {
                    Some(at) => projection
                        .items
                        .get(at)
                        .map(|item| item.output().to_string())
                        .unwrap_or_else(|| column.to_string()),
                    None => column.to_string(),
                };
                Step::Inside { projection: (**projection).clone(), column: inner }
            }
            FromSource::Opaque => Step::Stop(
                "the trail reaches a function or a construct Picus does not read, so it ends here."
                    .to_string(),
            ),
        }
    }

    /// Continue inside a projection — a view's definition, a derived table, a CTE.
    ///
    /// No hop is recorded here: a projection is not something the reader can go and
    /// look at, and only a *named relation* earns a step in the chain.
    fn descend(&mut self, trace: &mut Trace, projection: &Projection, column: &str, depth: usize) {
        if depth >= MAX_DEPTH {
            self.truncated = true;
            trace.verdict = Verdict::Unresolved;
            trace.stopped = format!("the trail is more than {MAX_DEPTH} levels deep.");
            return;
        }
        // A set operation has no single projection; every arm contributes.
        if !projection.arms.is_empty() {
            self.through_union(trace, projection, column, depth);
            return;
        }
        // Checked before `item_for`, which only ever answers for plain references —
        // without this a computed column would be reported as one the query does not
        // produce, which is both wrong and the opposite of informative.
        if let Some(Projected::Computed { reads, .. }) = projection.item_named(column) {
            trace.verdict = Verdict::Derived;
            trace.reads = self.ingredients(projection, reads);
            return;
        }
        // A projection the walker could not read in full poisons everything it did
        // not name **explicitly**. Anything else here is guesswork dressed as a
        // chain, and this whole crate exists on the principle that a stop with a
        // reason beats a plausible wrong table — most of all for a reader deciding
        // where to write.
        if projection.opaque && projection.item_named(column).is_none() {
            trace.verdict = Verdict::Unresolved;
            trace.stopped = format!(
                "`{column}` is not named by this query's projection, and the projection \
                 contains something Picus could not read — so where it comes from cannot be \
                 told from the SQL."
            );
            return;
        }
        match self.item_for(projection, column) {
            Some(source) => match self.step_from(projection, &source) {
                Step::Relation { relation, column } => {
                    self.follow(trace, relation, column, depth + 1)
                }
                Step::Inside { projection, column } => {
                    self.descend(trace, &projection, &column, depth + 1)
                }
                Step::Stop(why) => {
                    trace.verdict = Verdict::Unresolved;
                    trace.stopped = why;
                }
            },
            None => {
                trace.verdict = Verdict::Unresolved;
                trace.stopped = format!("`{column}` is not produced by the query that feeds it.");
            }
        }
    }

    /// Continue from a named relation. Records the hop, then decides whether the
    /// relation is a table (done) or a view (parse it and go on).
    fn follow(&mut self, trace: &mut Trace, relation: String, column: String, depth: usize) {
        if depth >= MAX_DEPTH {
            self.truncated = true;
            trace.verdict = Verdict::Unresolved;
            trace.stopped = format!("the trail is more than {MAX_DEPTH} levels deep.");
            return;
        }

        let is_view = self.catalogue.is_view(&relation);
        trace.hops.push(Hop {
            relation: relation.clone(),
            column: column.clone(),
            is_view: is_view.unwrap_or(false),
        });

        match is_view {
            // A table. The trail ends here, and this is the answer people came for.
            Some(false) => trace.verdict = Verdict::Resolved,
            None => {
                trace.verdict = Verdict::Unresolved;
                trace.stopped = format!(
                    "`{relation}` is not in this connection's catalogue — it may be in another \
                     schema, or newer than the catalogue."
                );
            }
            Some(true) => {
                self.note_visit(&relation);
                let Some(sql) = self.catalogue.definition(&relation) else {
                    trace.verdict = Verdict::Unresolved;
                    trace.stopped =
                        format!("the definition of `{relation}` could not be read.");
                    return;
                };
                let Some(projection) = project(&sql, DialectScope::One(self.engine)) else {
                    trace.verdict = Verdict::Unresolved;
                    trace.stopped = format!("the definition of `{relation}` could not be parsed.");
                    return;
                };
                // Unions, computed items and the depth limit are all `descend`'s to
                // handle — one place, so a shape met inside a view behaves exactly as
                // it does when the same view is traced directly.
                self.descend(trace, &projection, &column, depth + 1);
            }
        }
    }

    /// A set operation: the column comes from every arm at once.
    ///
    /// Resolved only when the arms **agree** on one base relation, which is the case
    /// worth resolving — a view that unions the archived and the live rows of the
    /// same table. When they disagree the value genuinely has several origins, and
    /// saying so beats naming whichever arm was written first.
    fn through_union(
        &mut self,
        trace: &mut Trace,
        projection: &Projection,
        column: &str,
        depth: usize,
    ) {
        let at = projection
            .arms
            .first()
            .and_then(|arm| arm.items.iter().position(|item| item.output() == column));

        let mut branches: Vec<Trace> = Vec::new();
        let mut computed: Vec<Ingredient> = Vec::new();
        for arm in &projection.arms {
            // Positional: `UNION` matches columns by position, and the arms are free
            // to name them differently — the second arm's name is not the reader's.
            let Some(item) = at.and_then(|at| arm.items.get(at)) else { continue };
            match item {
                Projected::Column { source, .. } => {
                    let source = source.clone();
                    branches.push(self.from_source(column, arm, &source, depth + 1));
                }
                // Collected rather than returned on: every arm contributes to what
                // this column is, and stopping at the first computed one reported the
                // ingredients of that arm alone as if they were the whole story.
                Projected::Computed { reads, .. } => {
                    for ingredient in self.ingredients(arm, reads) {
                        if !computed.contains(&ingredient) {
                            computed.push(ingredient);
                        }
                    }
                }
                Projected::Star { .. } => continue,
            }
        }

        // One arm computing it means the value is computed for those rows, so the
        // column as a whole cannot be written back through — `Derived` is the honest
        // verdict even when other arms read a plain column. Those arms' tables are
        // still named: they are part of what this column is made of.
        if !computed.is_empty() {
            trace.verdict = Verdict::Derived;
            trace.reads = computed;
            for branch in &branches {
                let relation = branch.base_relation();
                if !relation.is_empty()
                    && !trace.reads.iter().any(|held| held.relation == relation)
                {
                    trace.reads.push(Ingredient {
                        relation: relation.to_string(),
                        column: branch.base_column().to_string(),
                    });
                }
            }
            return;
        }

        let mut bases: Vec<&str> = branches
            .iter()
            .map(|branch| branch.base_relation())
            .filter(|base| !base.is_empty())
            .collect();
        bases.sort_unstable();
        bases.dedup();

        match bases.as_slice() {
            [_only] => {
                // One table behind every arm: the ordinary "live plus archive" view.
                if let Some(first) = branches.into_iter().find(|b| !b.base_relation().is_empty()) {
                    trace.hops.extend(first.hops);
                    trace.verdict = Verdict::Resolved;
                }
            }
            [] => {
                trace.verdict = Verdict::Unresolved;
                trace.stopped =
                    "the trail runs into a UNION whose arms could not be followed.".to_string();
            }
            // Every arm reads a real column, and they are not the same table. The
            // value IS one of these, per row — which is a different answer from
            // "computed", and saying "nothing to write back through" here would be
            // false twice over: there is a column behind every row, and there are two
            // writable tables rather than none.
            _several => {
                trace.verdict = Verdict::Split;
                trace.reads = branches
                    .iter()
                    .filter(|branch| !branch.base_relation().is_empty())
                    .map(|branch| Ingredient {
                        relation: branch.base_relation().to_string(),
                        column: branch.base_column().to_string(),
                    })
                    .collect();
                trace.reads.dedup();
            }
        }
    }

    /// The reference producing one output name, within a projection.
    fn item_for(&self, projection: &Projection, column: &str) -> Option<ColumnSource> {
        match projection.item_named(column) {
            Some(Projected::Column { source, .. }) => Some(source.clone()),
            // Not projected by name — it may be inside a `*`, in which case the name
            // belongs to whichever source carries it.
            //
            // **Never on an opaque projection.** This branch invents a reference out
            // of a name, and on a parse that went wrong it invents one for *every*
            // column: each then resolves to the single source the walk did manage to
            // read, under its own name, and the result is a full page of
            // `X ← THAT_TABLE.X` that looks like an answer and is an echo. A star the
            // parser really saw is worth expanding; a star that is all a broken parse
            // left behind is not.
            None if !projection.opaque
                && projection.items.iter().any(|i| matches!(i, Projected::Star { .. })) =>
            {
                Some(ColumnSource::new(None, column.to_string()))
            }
            _ => None,
        }
    }

    /// Name the ingredients of a computed value, attributing each where it can be.
    fn ingredients(&self, projection: &Projection, reads: &[ColumnSource]) -> Vec<Ingredient> {
        reads
            .iter()
            .map(|read| {
                let relation = projection
                    .source_named(read.qualifier.as_deref())
                    .map(|item| match &item.source {
                        FromSource::Relation { name } => name.clone(),
                        _ => item.name.clone(),
                    })
                    .unwrap_or_default();
                Ingredient { relation, column: read.column.clone() }
            })
            .collect()
    }
}
