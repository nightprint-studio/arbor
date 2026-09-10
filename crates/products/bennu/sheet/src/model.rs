//! What a spreadsheet is, once it is something to look at.
//!
//! Everything here is a decision about a **viewer**, and that is why it is ours and not the
//! reader's: how a float is printed so that a column of counts does not read as measurements, how a
//! serial number is spelled as a date, how much of a sheet is worth drawing. A `.xls` and the
//! `.xlsx` it was saved as have to show the same grid — a cell that reads `45292` in one and
//! `2024-01-01` in the other is the same file disagreeing with itself — so the answer lives above
//! the format, in one place, with the tests.
//!
//! What is NOT here is which cells are dates in the first place. That is a property of the file's
//! own number formats, it is different in every container, and `calamine` answers it.

use serde::Serialize;

/// The most rows read from one sheet.
///
/// A viewer is for **looking at** a spreadsheet, not for holding one: past a couple of thousand
/// rows nobody is reading, they are filtering — which is a spreadsheet's job and not an editor's.
/// The cut is reported rather than hidden, so what is on screen is never mistaken for all of it.
pub const MAX_ROWS: usize = 2_000;
/// The most columns read from one sheet. Past this a row is wider than any screen.
pub const MAX_COLS: usize = 100;
/// The most sheets read from one workbook.
pub const MAX_SHEETS: usize = 30;

/// What a cell **is**, which is what a grid aligns and tints by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CellKind {
    #[default]
    Empty,
    Text,
    Number,
    Date,
    Bool,
    /// `#DIV/0!` and its family — a value the spreadsheet itself calls wrong.
    Error,
}

/// One cell, already rendered.
///
/// Rendered in the backend and not in the viewer, deliberately: turning a serial number into a
/// date is the one piece of real logic in reading a spreadsheet, and it belongs where it can be
/// tested rather than in a component.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Cell {
    pub text: String,
    pub kind: CellKind,
}

impl Cell {
    pub fn text(value: impl Into<String>) -> Self {
        let text: String = value.into();
        match text.is_empty() {
            true => Self::default(),
            false => Self { text, kind: CellKind::Text },
        }
    }

    /// A number, shown as a number — or as a date when the cell's format says it is one.
    pub fn number(value: f64, is_date: bool) -> Self {
        match is_date {
            true => Self { text: serial_to_date(value), kind: CellKind::Date },
            false => Self { text: format_number(value), kind: CellKind::Number },
        }
    }

    /// A date the file already wrote out — the ISO spelling ODS uses, and the one OOXML allows.
    ///
    /// Passed through rather than parsed and re-spelled: a date that is already unambiguous can
    /// only lose by being read into a number and back.
    pub fn date(value: impl Into<String>) -> Self {
        let text: String = value.into();
        match text.is_empty() {
            true => Self::default(),
            false => Self { text, kind: CellKind::Date },
        }
    }

    pub fn boolean(value: bool) -> Self {
        Self { text: if value { "TRUE" } else { "FALSE" }.to_string(), kind: CellKind::Bool }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self { text: text.into(), kind: CellKind::Error }
    }

    pub fn is_empty(&self) -> bool {
        self.kind == CellKind::Empty && self.text.is_empty()
    }
}

/// One sheet of the workbook.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Sheet {
    pub name: String,
    /// Every row padded to this width, so the viewer draws a rectangle without measuring.
    pub columns: usize,
    pub rows: Vec<Vec<Cell>>,
    /// The sheet has more than was read — see [`MAX_ROWS`] / [`MAX_COLS`].
    pub truncated: bool,
}

/// A workbook, as much of it as is worth looking at.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Workbook {
    /// `xls` / `xlsx` / `xlsb` / `ods` — what the bytes turned out to be, which is not always what
    /// the name said.
    pub format: String,
    pub sheets: Vec<Sheet>,
    /// The workbook has more sheets than were read.
    pub truncated: bool,
}

impl Sheet {
    /// Square off a sheet built as a sparse map of rows: pad every row to the widest, drop the
    /// trailing empty ones, and say whether anything was cut.
    ///
    /// The trailing rows matter more than they sound: a spreadsheet somebody has scrolled through
    /// records thousands of empty rows, and a viewer that drew them would open on a page of
    /// nothing under the last line of data.
    pub fn finish(name: String, mut rows: Vec<Vec<Cell>>, truncated: bool) -> Self {
        while rows.last().is_some_and(|r| r.iter().all(Cell::is_empty)) {
            rows.pop();
        }
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        for row in &mut rows {
            row.resize(columns, Cell::default());
        }
        Sheet { name, columns, rows, truncated }
    }
}

/// A number as a person writes it: no exponent for ordinary magnitudes, no trailing zeros, and no
/// `.0` on something that is a whole number.
///
/// Spreadsheets hold every number as a float, so the count `3` arrives as `3.0` and printing it
/// that way makes a column of counts read as measurements.
pub fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return String::new();
    }
    if value == value.trunc() && value.abs() < 1e15 {
        return format!("{}", value as i64);
    }
    // Excel itself keeps 15 significant digits; anything past that is the float's own noise, and
    // printing it turns 0.1 + 0.2 into a column nobody trusts.
    let mut text = format!("{value:.10}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    text
}

/// An Excel serial number as a date, in the one spelling that sorts.
///
/// ## The leap year that never happened
///
/// Serial 1 is 1900-01-01 and serial 60 is 1900-02-29 — a day that does not exist. Lotus 1-2-3 had
/// the bug, Excel copied it for compatibility and every file since encodes it, so a converter that
/// is *correct about the calendar* is wrong about the data by one day for everything before March
/// 1900. Serials at or below 60 are therefore read as one day later than the arithmetic says.
pub fn serial_to_date(serial: f64) -> String {
    if !serial.is_finite() || serial < 0.0 {
        return format_number(serial);
    }
    let days = serial.trunc() as i64;
    let fraction = serial - serial.trunc();
    // A time on its own — a duration, or a clock reading with no date attached.
    if days == 0 {
        return format_time(fraction);
    }
    // Serial 60 is the day that never was. It has no answer in any calendar, so it is written out
    // rather than computed — and it has to be written out, because every arithmetic that produces
    // the right answer on both sides of it is wrong exactly here.
    let (y, m, d) = match days {
        60 => (1900, 2, 29),
        // Below it the file counts from 1899-12-31 (serial 1 is 1900-01-01); above it, from
        // 1899-12-30, which is what absorbs the day nobody lived through.
        _ => {
            let epoch = days_from_civil(1899, 12, 30);
            civil_from_days(if days < 60 { epoch + days + 1 } else { epoch + days })
        }
    };
    match fraction > f64::EPSILON {
        true => format!("{y:04}-{m:02}-{d:02} {}", format_time(fraction)),
        false => format!("{y:04}-{m:02}-{d:02}"),
    }
}

/// A fraction of a day as a clock time. Seconds are shown only when there are any: a column of
/// `09:30` is read at a glance and a column of `09:30:00` is not.
fn format_time(fraction: f64) -> String {
    let total = (fraction * 86_400.0).round() as i64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    match s {
        0 => format!("{h:02}:{m:02}"),
        _ => format!("{h:02}:{m:02}:{s:02}"),
    }
}

/// Days from 1970-01-01 to a civil date, and back — Howard Hinnant's algorithm, which is exact
/// over the whole proleptic Gregorian range and needs no table.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m as i64 - 3 } else { m as i64 + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_number_is_not_printed_as_a_measurement() {
        assert_eq!(format_number(3.0), "3");
        assert_eq!(format_number(-12.0), "-12");
        assert_eq!(format_number(0.0), "0");
        assert_eq!(format_number(1.5), "1.5");
        assert_eq!(format_number(0.1 + 0.2), "0.3");
    }

    /// The bug every spreadsheet encodes, and the one date conversion that is not arithmetic.
    #[test]
    fn the_phantom_leap_day_of_1900_is_honoured_rather_than_corrected() {
        // 61 is the first serial the calendar and the file agree on.
        assert_eq!(serial_to_date(61.0), "1900-03-01");
        assert_eq!(serial_to_date(60.0), "1900-02-29");
        assert_eq!(serial_to_date(59.0), "1900-02-28");
        assert_eq!(serial_to_date(1.0), "1900-01-01");
    }

    #[test]
    fn the_dates_people_actually_have_come_out_right() {
        assert_eq!(serial_to_date(45_292.0), "2024-01-01");
        assert_eq!(serial_to_date(25_569.0), "1970-01-01");
        assert_eq!(serial_to_date(36_526.0), "2000-01-01");
        // A leap day that did happen.
        assert_eq!(serial_to_date(44_621.0), "2022-03-01");
        assert_eq!(serial_to_date(44_620.0), "2022-02-28");
    }

    #[test]
    fn a_time_is_shown_with_seconds_only_when_it_has_any() {
        assert_eq!(serial_to_date(0.5), "12:00");
        assert_eq!(serial_to_date(45_292.5), "2024-01-01 12:00");
        assert_eq!(serial_to_date(0.395_833_333_333), "09:30");
    }

    /// The rows a spreadsheet records because somebody scrolled through it are not data, and a
    /// viewer that drew them would open on a page of nothing.
    #[test]
    fn trailing_empty_rows_are_dropped_and_the_rest_squared_off() {
        let sheet = Sheet::finish(
            "S".into(),
            vec![
                vec![Cell::text("a"), Cell::text("b")],
                vec![Cell::text("c")],
                vec![Cell::default()],
                vec![],
            ],
            false,
        );
        assert_eq!(sheet.rows.len(), 2);
        assert_eq!(sheet.columns, 2);
        assert!(sheet.rows.iter().all(|r| r.len() == 2));
        assert!(sheet.rows[1][1].is_empty());
    }
}
