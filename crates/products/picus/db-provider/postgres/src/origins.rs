//! Which relation each result column is read from.
//!
//! ## The answer is already on the wire
//!
//! PostgreSQL's `RowDescription` names, for every column it describes, the relation
//! the column is read from and its attribute number there. Picus already prepares
//! every statement in order to type its columns, so that description is in hand
//! before a single row is fetched — this costs no round trip of its own to obtain.
//!
//! Nothing here reads the SQL. That is the point: a `*` across a join, an alias, a
//! view, a subquery in the projection all report their true origin, and no amount
//! of parsing the statement text would match the server on any of them. It is the
//! same reasoning that moved [`crate::session`]'s large-object detection off the
//! text and onto the description.
//!
//! ## Oids are resolved once per connection
//!
//! What the wire hands over is an **oid**, and people read names. An oid does not
//! change under a live connection, so the mapping is cached for the session's
//! lifetime: the first result that mentions a relation pays one small indexed
//! catalogue read, every result after it pays nothing. A relation that is looked up
//! and turns out to be gone is cached as *unnameable* rather than left absent —
//! otherwise every result would re-ask the same unanswerable question.
//!
//! ## A view is reported as itself
//!
//! Worth stating because the opposite is the natural guess. PostgreSQL stamps a
//! column's origin in the **parser** (`markTargetListOrigin`), which runs before the
//! rewriter expands anything — and to the parser a view is an ordinary relation. So
//! `SELECT * FROM v_elenchi` reports `v_elenchi`, not the seven tables it joins, and
//! a query over one view has exactly **one** origin however many tables built it.
//!
//! That is the better answer anyway: the view is the thing the statement names and
//! the thing the reader is thinking about. But it does mean a caller cannot use this
//! to decompose a view, and a caller that only draws something when there are two or
//! more origins will correctly draw nothing for a single-view select.
//!
//! A subquery in `FROM` is different: there the parser copies the origin **up** from
//! the subquery's own target entry, so `SELECT * FROM (SELECT a.x FROM a) s` still
//! reports `a`.
//!
//! ## It is decoration, so it never fails a query
//!
//! Every failure mode here is silence: a statement that will not prepare, a
//! catalogue read that errors, a relation dropped between the description and the
//! lookup, a poisoned cache. All of them yield *no sources*, which is the state a
//! result from an engine that does not report origins is in anyway — so the caller
//! needs no separate handling for "it went wrong", only for "nothing is claimed".

use std::collections::HashMap;
use std::sync::Mutex;

use picus_db_api::prelude::ColumnSource;
use tokio_postgres::{Client, Column as PgColumn};

/// Everything needed to name a column of one relation.
struct Relation {
    /// Unqualified, as the catalogue spells it. Empty for a relation that was asked
    /// about and could not be named — see the note on caching negatives above.
    name: String,
    /// Attribute number → column name. System columns (`ctid` and friends, whose
    /// attribute numbers are negative) are deliberately not in here: they are an
    /// address, not a column anyone selected.
    columns: HashMap<i16, String>,
}

impl Relation {
    /// A relation that was looked up and could not be named.
    fn unnameable() -> Self {
        Self { name: String::new(), columns: HashMap::new() }
    }
}

/// Every attribute of the relations named in a description, in one indexed read.
///
/// Whole relations rather than the individual columns asked about: a table's
/// attribute list is small, the join is on the index either way, and fetching it
/// entire is what makes the *second* query mentioning that table free.
const LOOKUP: &str = "\
    SELECT c.oid, c.relname, a.attnum, a.attname \
      FROM pg_class c \
      JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped \
     WHERE c.oid = ANY($1::oid[])";

/// One column's origin exactly as the server stated it: the oid of the relation it
/// is read from and its attribute number there, both absent for anything computed.
///
/// A plain pair rather than the driver's own `Column`, and that is what makes the
/// decisions below testable: `tokio_postgres::Column` has private fields and cannot
/// be constructed outside its crate, so every branch here would otherwise be
/// reachable only against a live server.
type Described = (Option<u32>, Option<i16>);

/// Relation and column names by oid, for the lifetime of one session.
#[derive(Default)]
pub struct RelationCache(Mutex<HashMap<u32, Relation>>);

impl RelationCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Where each described column comes from, resolving whatever is not cached yet.
    ///
    /// Returns an entry only for columns that have an origin, so the result is
    /// shorter than `described` whenever the projection computes anything.
    pub async fn sources(&self, client: &Client, described: &[PgColumn]) -> Vec<ColumnSource> {
        let described: Vec<Described> =
            described.iter().map(|c| (c.table_oid(), c.column_id())).collect();

        let wanted = self.unknown_oids(&described);
        if !wanted.is_empty() {
            self.learn(client, &wanted).await;
        }
        self.read(&described)
    }

    /// The oids this description mentions that the cache cannot yet name.
    ///
    /// Sorted and deduped so a forty-column `SELECT *` asks about one relation once.
    fn unknown_oids(&self, described: &[Described]) -> Vec<u32> {
        let cache = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let mut wanted: Vec<u32> = described
            .iter()
            .filter_map(|&(oid, _)| oid)
            .filter(|oid| *oid != 0 && !cache.contains_key(oid))
            .collect();
        wanted.sort_unstable();
        wanted.dedup();
        wanted
    }

    /// Read the catalogue for `oids` and remember what it said.
    ///
    /// Every oid asked about gets an entry whether or not the read produced rows for
    /// it, which is what stops a dropped or unreadable relation from being looked up
    /// again on every result for the rest of the session.
    async fn learn(&self, client: &Client, oids: &[u32]) {
        // Silence on failure, by design — see the module note. A cosmetic lookup has
        // no business turning a query that produced rows into an error.
        let Ok(rows) = client.query(LOOKUP, &[&oids]).await else { return };

        let mut cache = self.0.lock().unwrap_or_else(|p| p.into_inner());
        for oid in oids {
            cache.entry(*oid).or_insert_with(Relation::unnameable);
        }
        for row in rows {
            let oid: u32 = row.get(0);
            let name: String = row.get(1);
            let attnum: i16 = row.get(2);
            let attname: String = row.get(3);
            let relation = cache.entry(oid).or_insert_with(Relation::unnameable);
            relation.name = name;
            relation.columns.insert(attnum, attname);
        }
    }

    /// The sources the cache can account for, and nothing else.
    fn read(&self, described: &[Described]) -> Vec<ColumnSource> {
        let cache = self.0.lock().unwrap_or_else(|p| p.into_inner());
        described
            .iter()
            .enumerate()
            .filter_map(|(index, &(table_oid, column_id))| {
                let oid = table_oid.filter(|oid| *oid != 0)?;
                let relation = cache.get(&oid)?;
                // An unnameable relation is an answer, but not one worth showing:
                // "this column comes from something" helps nobody.
                if relation.name.is_empty() {
                    return None;
                }
                // Absent for a system column, whose attribute number is negative and
                // therefore not in the map. The relation is still the truth, so the
                // entry stands with an empty name rather than being dropped.
                let name = column_id
                    .and_then(|attnum| relation.columns.get(&attnum))
                    .cloned()
                    .unwrap_or_default();
                Some(ColumnSource { index, table: relation.name.clone(), name })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seed the cache directly. The catalogue read needs a server; everything that
    /// decides what a description *means* does not, and that is where the branches
    /// are — see [`Described`] for why it is a plain pair.
    fn cache_of(entries: &[(u32, &str, &[(i16, &str)])]) -> RelationCache {
        let cache = RelationCache::new();
        {
            let mut guard = cache.0.lock().unwrap();
            for (oid, name, columns) in entries {
                guard.insert(
                    *oid,
                    Relation {
                        name: name.to_string(),
                        columns: columns.iter().map(|(n, c)| (*n, c.to_string())).collect(),
                    },
                );
            }
        }
        cache
    }

    #[test]
    fn a_computed_column_has_no_origin() {
        // `count(*)` and every literal arrive with no relation at all, and an oid of
        // zero means the same thing on the wire. Neither may invent a source.
        let cache = cache_of(&[(7, "comunicazioni", &[(1, "id")])]);
        assert!(cache.read(&[(None, None)]).is_empty());
        assert!(cache.read(&[(Some(0), Some(1))]).is_empty(), "oid 0 is `no relation`");
    }

    #[test]
    fn a_column_of_an_unnameable_relation_is_left_out() {
        // Dropped between the description and the lookup, or unreadable. "It comes
        // from something" is not information, so nothing is claimed.
        let cache = cache_of(&[(42, "", &[])]);
        assert!(cache.read(&[(Some(42), Some(1))]).is_empty());
    }

    #[test]
    fn an_alias_is_reported_under_the_relations_own_name() {
        // The whole point of carrying the attribute number: the result column may be
        // called `quando`, and what the reader needs is `data_invio`.
        let cache = cache_of(&[(7, "comunicazioni", &[(3, "data_invio")])]);
        let sources = cache.read(&[(Some(7), Some(3))]);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].table, "comunicazioni");
        assert_eq!(sources[0].name, "data_invio");
    }

    #[test]
    fn a_row_address_names_its_table_and_nothing_more() {
        // `ctid` arrives with a negative attribute number, which no map of real
        // columns holds. The relation is still true, so the entry stands.
        let cache = cache_of(&[(7, "comunicazioni", &[(1, "id")])]);
        let sources = cache.read(&[(Some(7), Some(-1))]);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].table, "comunicazioni");
        assert!(sources[0].name.is_empty(), "there is no column here to name");
    }

    #[test]
    fn positions_are_the_descriptions_own() {
        // The sparseness is load-bearing: entry `n` is NOT column `n`, and a caller
        // reading these positionally would attribute every column after a computed
        // one to the wrong table.
        let cache = cache_of(&[(7, "comunicazioni", &[(1, "id")]), (9, "enti", &[(2, "denom")])]);
        let sources = cache.read(&[
            (Some(7), Some(1)), // column 0
            (None, None),       // column 1 — count(*)
            (Some(9), Some(2)), // column 2
        ]);
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].index, 0);
        assert_eq!(sources[1].index, 2, "the gap must not close up");
        assert_eq!(sources[1].table, "enti");
    }

    #[test]
    fn only_what_is_missing_is_asked_about() {
        // A forty-column `SELECT *` across two tables must produce one lookup for one
        // of them, not forty for both.
        let cache = cache_of(&[(7, "comunicazioni", &[(1, "id")])]);
        let wanted = cache.unknown_oids(&[
            (Some(7), Some(1)), // cached
            (Some(9), Some(1)), // not
            (Some(9), Some(2)), // not, same relation
            (None, None),       // computed
            (Some(0), Some(1)), // no relation
        ]);
        assert_eq!(wanted, vec![9]);
    }

    #[test]
    fn nothing_is_asked_about_twice_once_it_is_known() {
        let cache = cache_of(&[(7, "comunicazioni", &[(1, "id")]), (42, "", &[])]);
        // The unnameable one included: caching the negative is what stops a dropped
        // relation being looked up again on every result for the rest of the session.
        assert!(cache.unknown_oids(&[(Some(7), Some(1)), (Some(42), Some(1))]).is_empty());
    }
}
