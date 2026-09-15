//! `bennu_read_sheet` — a spreadsheet in the project, as a grid.
//!
//! Read in the backend rather than in the viewer, and that is the whole design decision here: a
//! spreadsheet is a container format, and the alternative was a JavaScript library that parses it
//! in the WebView. Doing it here means the reading is [tested](bennu_sheet), the bytes cross the
//! seam once as rendered text rather than as a file to re-parse, and the one piece of real logic —
//! a serial number that is actually a date — lives where it can be checked.

use bennu_core::prelude::BennuState;
use bennu_sheet::prelude::Workbook;
use serde::Deserialize;

/// Args for [`bennu_read_sheet`].
#[derive(Deserialize)]
pub struct SheetArgs {
    /// Absolute path of the spreadsheet.
    pub file: String,
}

/// The most bytes read from one spreadsheet.
///
/// A viewer is for looking at a file, and past this the answer to "what is in it" is a tool that
/// filters rather than one that draws. Refused with a sentence rather than by running out of
/// memory quietly.
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Read a spreadsheet into sheets of rendered cells.
///
/// The format is decided by the file's own bytes, not by its extension: an `.xls` that is really an
/// OOXML file is something every export tool has shipped, and a reader that trusted the name would
/// refuse a file that opens perfectly. What it turned out to be comes back in `format`.
#[arbor_rpc::handler]
fn bennu_read_sheet(_ctx: &BennuState, args: SheetArgs) -> Result<Workbook, String> {
    let path = std::path::Path::new(&args.file);
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if size > MAX_BYTES {
        return Err(format!(
            "that spreadsheet is {} MB — too large to open in a viewer",
            size / (1024 * 1024)
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("could not read {}: {e}", args.file))?;
    bennu_sheet::prelude::read(&bytes)
}
