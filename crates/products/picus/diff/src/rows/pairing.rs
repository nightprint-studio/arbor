//! Deciding which row on one side is which row on the other, and what differs
//! between the two.
//!
//! Split out of [`super`] because the shapes and the matching are two jobs: the
//! module above is the vocabulary a caller reads, this one is the walk over it.

use std::collections::HashMap;

use crate::error::Side;
use crate::rows::{CellDiff, ChangedRow, ColumnPair, DiffRow, RowKey, RowSet, RowsComparison, Sink};
use crate::value::DiffValue;

fn side_index(pair: &ColumnPair, side: Side) -> usize {
    match side {
        Side::A => pair.ia,
        Side::B => pair.ib,
    }
}

fn project(row: &[DiffValue], compared: &[ColumnPair], side: Side) -> Vec<DiffValue> {
    compared.iter().map(|c| row[side_index(c, side)].clone()).collect()
}

fn push_only(out: &mut RowsComparison, sink: &mut Sink, side: Side, row: DiffRow) {
    match side {
        Side::A => out.only_in_a_total += 1,
        Side::B => out.only_in_b_total += 1,
    }
    if !sink.room() {
        out.truncated = true;
        return;
    }
    match side {
        Side::A => out.only_in_a.push(row),
        Side::B => out.only_in_b.push(row),
    }
}

fn compare_pair(
    key: RowKey,
    ra: &[DiffValue],
    rb: &[DiffValue],
    compared: &[ColumnPair],
    out: &mut RowsComparison,
    sink: &mut Sink,
) {
    let cells: Vec<CellDiff> = compared
        .iter()
        .filter_map(|c| {
            let (a, b) = (&ra[c.ia], &rb[c.ib]);
            (a != b).then(|| CellDiff { column: c.name.clone(), a: a.clone(), b: b.clone() })
        })
        .collect();

    if cells.is_empty() {
        out.matched += 1;
        return;
    }
    out.changed_total += 1;
    if sink.room() {
        out.changed.push(ChangedRow { key, cells });
    } else {
        out.truncated = true;
    }
}

pub(super) fn compare_positionally(
    a: &RowSet,
    b: &RowSet,
    compared: &[ColumnPair],
    out: &mut RowsComparison,
    sink: &mut Sink,
) {
    for i in 0..a.rows.len().max(b.rows.len()) {
        match (a.rows.get(i), b.rows.get(i)) {
            (Some(ra), Some(rb)) => compare_pair(RowKey::Position(i), ra, rb, compared, out, sink),
            (Some(ra), None) => push_only(
                out,
                sink,
                Side::A,
                DiffRow { key: RowKey::Position(i), values: project(ra, compared, Side::A) },
            ),
            (None, Some(rb)) => push_only(
                out,
                sink,
                Side::B,
                DiffRow { key: RowKey::Position(i), values: project(rb, compared, Side::B) },
            ),
            (None, None) => unreachable!("i < max(len_a, len_b)"),
        }
    }
}

/// Rows grouped by key value, keeping first-appearance order so the output does
/// not depend on hash iteration order.
#[derive(Debug, Default)]
struct Grouped {
    order: Vec<Vec<DiffValue>>,
    groups: HashMap<Vec<DiffValue>, Vec<usize>>,
}

fn group_rows(set: &RowSet, key: &[ColumnPair], side: Side) -> Grouped {
    let mut out = Grouped::default();
    for (i, row) in set.rows.iter().enumerate() {
        let values: Vec<DiffValue> =
            key.iter().map(|c| row[side_index(c, side)].clone()).collect();
        match out.groups.get_mut(&values) {
            Some(existing) => existing.push(i),
            None => {
                out.order.push(values.clone());
                out.groups.insert(values, vec![i]);
            }
        }
    }
    out
}

fn duplicates(grouped: &Grouped) -> Vec<RowKey> {
    grouped
        .order
        .iter()
        .filter(|k| grouped.groups[*k].len() > 1)
        .map(|k| RowKey::Values(k.clone()))
        .collect()
}

pub(super) fn compare_by_key(
    a: &RowSet,
    b: &RowSet,
    compared: &[ColumnPair],
    key: &[ColumnPair],
    out: &mut RowsComparison,
    sink: &mut Sink,
) {
    let ga = group_rows(a, key, Side::A);
    let gb = group_rows(b, key, Side::B);

    for k in &ga.order {
        let rows_a = &ga.groups[k];
        let Some(rows_b) = gb.groups.get(k) else {
            for &i in rows_a {
                let row = DiffRow {
                    key: RowKey::Values(k.clone()),
                    values: project(&a.rows[i], compared, Side::A),
                };
                push_only(out, sink, Side::A, row);
            }
            continue;
        };
        // A key that is not unique cannot pair rows by identity, so the groups are
        // zipped in read order and the surplus is reported as one-sided. Recorded
        // in `duplicate_keys_*` so the reader knows this happened.
        let paired = rows_a.len().min(rows_b.len());
        for p in 0..paired {
            compare_pair(
                RowKey::Values(k.clone()),
                &a.rows[rows_a[p]],
                &b.rows[rows_b[p]],
                compared,
                out,
                sink,
            );
        }
        for &i in &rows_a[paired..] {
            let row = DiffRow {
                key: RowKey::Values(k.clone()),
                values: project(&a.rows[i], compared, Side::A),
            };
            push_only(out, sink, Side::A, row);
        }
        for &j in &rows_b[paired..] {
            let row = DiffRow {
                key: RowKey::Values(k.clone()),
                values: project(&b.rows[j], compared, Side::B),
            };
            push_only(out, sink, Side::B, row);
        }
    }

    for k in &gb.order {
        if ga.groups.contains_key(k) {
            continue;
        }
        for &j in &gb.groups[k] {
            let row = DiffRow {
                key: RowKey::Values(k.clone()),
                values: project(&b.rows[j], compared, Side::B),
            };
            push_only(out, sink, Side::B, row);
        }
    }

    out.duplicate_keys_a = duplicates(&ga);
    out.duplicate_keys_b = duplicates(&gb);
}
