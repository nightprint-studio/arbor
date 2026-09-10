//! From a workbook's bytes to a grid — the whole of what this crate does at its seam.
//!
//! [`calamine`] answers *what the file says*; everything here answers *what a reader should see*,
//! which is three separate questions it does not have an opinion about:
//!
//! 1. **where a sheet starts.** A range is reported from its first non-empty cell, so a sheet whose
//!    data begins at `C5` comes back as a block with no memory of the gap. Drawn as-is, its first
//!    column would be labelled `A` and every reference anybody reads off the screen would be wrong
//!    by two. The gap is put back.
//! 2. **how much is worth drawing** — [`MAX_ROWS`], [`MAX_COLS`], [`MAX_SHEETS`], and saying so
//!    rather than quietly stopping.
//! 3. **how a value reads**, which is [`crate::model`] and is shared with nothing else.
//!
//! > [!NOTE] The 1904 date system is not honoured
//! > A workbook can opt into counting days from 1904 instead of 1900 — a Mac Excel setting, and one
//! > this reader cannot see: calamine carries the flag inside its date value and only applies it
//! > through the `dates` feature, whose own conversion is worse here for times and for the phantom
//! > leap day (see the manifest). Dates in such a file read four years and a day early. It is rare
//! > enough to be worth saying rather than worth chasing, and it is what this crate did before
//! > calamine too.

use std::io::Cursor;

use calamine::{Data, Range, Reader, Sheets};

use crate::model::{Cell, Sheet, Workbook, MAX_COLS, MAX_ROWS, MAX_SHEETS};

/// Read a spreadsheet, whatever container it turns out to be.
///
/// By **content and not by extension**: a `.xls` that is really an OOXML file is something every
/// export tool has shipped, and a reader that trusted the name would refuse a file that opens
/// perfectly. What it turned out to be comes back in [`Workbook::format`], so the viewer can say.
pub fn read(bytes: &[u8]) -> Result<Workbook, String> {
    // A borrowed cursor: `open_workbook_auto_from_rs` tries each format in turn and so needs a
    // reader it can clone, and cloning a cursor over a slice copies the cursor, not the file.
    let mut book = calamine::open_workbook_auto_from_rs(Cursor::new(bytes)).map_err(|_| {
        "that is not a spreadsheet Bennu can read — it is neither an Excel workbook \
         (.xls / .xlsx / .xlsb) nor an OpenDocument sheet (.ods)"
            .to_string()
    })?;

    let format = format_of(&book).to_string();
    let names = book.sheet_names();
    let truncated = names.len() > MAX_SHEETS;

    let sheets = names
        .into_iter()
        .take(MAX_SHEETS)
        .map(|name| match book.worksheet_range(&name) {
            Ok(range) => grid(name, &range),
            // A sheet that will not open is shown empty rather than failing the workbook: the other
            // nine are what the reader came for, and a whole file refused because of one damaged
            // sheet is a file they cannot look at at all.
            Err(_) => Sheet::finish(name, Vec::new(), false),
        })
        .collect();

    Ok(Workbook { format, sheets, truncated })
}

/// What the bytes turned out to be — the name a person would use for it, not calamine's type.
fn format_of<RS>(book: &Sheets<RS>) -> &'static str {
    match book {
        Sheets::Xls(_) => "xls",
        Sheets::Xlsx(_) => "xlsx",
        Sheets::Xlsb(_) => "xlsb",
        Sheets::Ods(_) => "ods",
    }
}

/// One sheet's used range as a grid that starts where the FILE starts.
fn grid(name: String, range: &Range<Data>) -> Sheet {
    let Some((first_row, first_col)) = range.start() else {
        return Sheet::finish(name, Vec::new(), false);
    };
    let (top, left) = (first_row as usize, first_col as usize);
    // Reported before anything is cut, from the sheet's real extent: the leading gap counts towards
    // the width and height a reader would have to scroll, so a block of ten columns starting at
    // `CV1` is past the limit however small the block is.
    let truncated = top + range.height() > MAX_ROWS || left + range.width() > MAX_COLS;

    // The empty rows above the first one with anything in it. Kept, so row 12 of the file is row 12
    // of the grid — `Sheet::finish` drops the trailing ones, which are the ones nobody meant.
    let mut rows: Vec<Vec<Cell>> = vec![Vec::new(); top.min(MAX_ROWS)];
    for row in range.rows() {
        if rows.len() >= MAX_ROWS {
            break;
        }
        let mut out = vec![Cell::default(); left.min(MAX_COLS)];
        for value in row {
            if out.len() >= MAX_COLS {
                break;
            }
            out.push(cell(value));
        }
        rows.push(out);
    }
    Sheet::finish(name, rows, truncated)
}

/// One value, as text plus what it is — the only place the two libraries' vocabularies meet.
fn cell(value: &Data) -> Cell {
    match value {
        Data::Empty => Cell::default(),
        Data::String(text) => Cell::text(text.as_str()),
        Data::Int(n) => Cell::number(*n as f64, false),
        Data::Float(n) => Cell::number(*n, false),
        Data::Bool(b) => Cell::boolean(*b),
        // A serial number the file's own format calls a date. Spelled by us: see the module note on
        // why calamine's own conversion is not used for it.
        Data::DateTime(when) => Cell::number(when.as_f64(), true),
        // Already a date, written out by the file (ODS, and the ISO spelling OOXML allows). Passed
        // through rather than re-parsed — re-spelling a date that is already unambiguous can only
        // lose to it.
        Data::DateTimeIso(text) => Cell::date(text.as_str()),
        // A duration is not a date, and rendering it as one would put `PT8H30M` in 1899.
        Data::DurationIso(text) => Cell::text(text.as_str()),
        Data::Error(what) => Cell::error(what.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CellKind;
    use calamine::{CellErrorType, ExcelDateTime, ExcelDateTimeType};

    fn range_from(cells: &[((u32, u32), Data)]) -> Range<Data> {
        let mut range = Range::new(cells[0].0, cells[0].0);
        for ((row, col), value) in cells {
            range.set_value((*row, *col), value.clone());
        }
        range
    }

    /// The defect this reader would have if it drew the used range as it comes: a sheet whose data
    /// starts at `C5` would be drawn with that data under the header `A`, and every cell reference
    /// read off the screen would be wrong.
    #[test]
    fn a_sheet_that_starts_away_from_the_corner_keeps_its_place() {
        // C5 and D5 — row index 4, column index 2.
        let range = range_from(&[
            ((4, 2), Data::String("name".into())),
            ((4, 3), Data::String("qty".into())),
        ]);
        let sheet = grid("S".into(), &range);
        assert_eq!(sheet.rows.len(), 5, "four empty rows above, then the data");
        assert!(sheet.rows[0..4].iter().all(|r| r.iter().all(Cell::is_empty)));
        assert!(sheet.rows[4][0].is_empty());
        assert!(sheet.rows[4][1].is_empty());
        assert_eq!(sheet.rows[4][2].text, "name");
        assert_eq!(sheet.rows[4][3].text, "qty");
        assert_eq!(sheet.columns, 4);
        assert!(!sheet.truncated);
    }

    #[test]
    fn a_sheet_in_the_corner_is_not_padded() {
        let range = range_from(&[((0, 0), Data::String("a".into()))]);
        let sheet = grid("S".into(), &range);
        assert_eq!(sheet.rows.len(), 1);
        assert_eq!(sheet.rows[0][0].text, "a");
    }

    #[test]
    fn an_empty_sheet_is_a_sheet_with_no_rows() {
        let sheet = grid("S".into(), &Range::empty());
        assert!(sheet.rows.is_empty());
        assert_eq!(sheet.columns, 0);
        assert!(!sheet.truncated);
    }

    /// The cut is announced, and it counts the gap: a small block far to the right is still past
    /// the edge of anything that could draw it.
    #[test]
    fn a_sheet_wider_than_the_viewer_says_so() {
        let range = range_from(&[((0, MAX_COLS as u32 + 5), Data::String("far".into()))]);
        let sheet = grid("S".into(), &range);
        assert!(sheet.truncated);
        assert!(sheet.columns <= MAX_COLS);
    }

    #[test]
    fn a_sheet_taller_than_the_viewer_is_cut_at_the_limit() {
        let cells: Vec<_> = (0..MAX_ROWS as u32 + 50)
            .map(|r| ((r, 0), Data::Float(r as f64)))
            .collect();
        let sheet = grid("S".into(), &range_from(&cells));
        assert!(sheet.truncated);
        assert_eq!(sheet.rows.len(), MAX_ROWS);
    }

    #[test]
    fn every_kind_of_value_arrives_as_itself() {
        assert_eq!(cell(&Data::Empty).kind, CellKind::Empty);
        assert_eq!(cell(&Data::String("hello".into())).kind, CellKind::Text);
        assert_eq!(cell(&Data::Bool(true)).text, "TRUE");
        assert_eq!(cell(&Data::Error(CellErrorType::Div0)).text, "#DIV/0!");
        assert_eq!(cell(&Data::Error(CellErrorType::Div0)).kind, CellKind::Error);
        // A count is a count, not a measurement — the float behind it must not show.
        assert_eq!(cell(&Data::Float(3.0)).text, "3");
        assert_eq!(cell(&Data::Float(3.0)).kind, CellKind::Number);
        assert_eq!(cell(&Data::Int(42)).text, "42");
    }

    /// The one conversion in reading a spreadsheet, and the reason it is ours: a date is a number
    /// until its format says otherwise, and the same number must read the same in every container.
    #[test]
    fn a_date_serial_is_spelled_and_not_shown_as_a_number() {
        let serial = |v: f64| {
            cell(&Data::DateTime(ExcelDateTime::new(v, ExcelDateTimeType::DateTime, false)))
        };
        assert_eq!(serial(45_292.0).text, "2024-01-01");
        assert_eq!(serial(45_292.0).kind, CellKind::Date);
        // A time on its own — the case calamine's own conversion puts in 1899.
        assert_eq!(serial(0.5).text, "12:00");
    }

    /// A duration is not a date. Read as one it lands in 1899, which is the kind of wrong that
    /// looks like data.
    #[test]
    fn a_duration_stays_text() {
        assert_eq!(cell(&Data::DurationIso("PT8H30M".into())).kind, CellKind::Text);
        assert_eq!(cell(&Data::DateTimeIso("2024-01-01T00:00:00".into())).kind, CellKind::Date);
    }

    /// The bytes decide, and bytes that are neither say so in a sentence rather than by panicking.
    #[test]
    fn something_that_is_not_a_spreadsheet_is_refused_with_a_reason() {
        let err = read(b"not a spreadsheet at all").unwrap_err();
        assert!(err.contains("not a spreadsheet"), "{err}");
    }
}
