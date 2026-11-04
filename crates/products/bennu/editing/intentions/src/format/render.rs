//! The last pass: walk the tokens once, wrap each unit as it is reached, lay out each line and join
//! the lines with the blank lines they keep.

use super::classify::Info;
use super::layout::{indent_string, lines_of, Ctx, Machine};
use super::lex::{Kind, Tok};
use super::spacing::Gap;
use super::structure::Unit;
use super::style::FormatStyle;
use super::wrap::wrap_unit;

/// The formatted text of `toks` with `\n` line endings and no final-newline policy applied.
/// `base` is the indent (in columns) the first line starts at — non-zero when formatting a selection.
pub(crate) fn render(
    toks: &mut [Tok],
    info: &[Info],
    gaps: &[Gap],
    units: &[Unit],
    style: &FormatStyle,
    base: usize,
) -> String {
    let n = toks.len();
    if n == 0 {
        return String::new();
    }
    // A line comment ends its line, whatever the author's spacing said.
    for i in 1..n {
        if toks[i - 1].kind == Kind::LineComment && toks[i].nl_before == 0 {
            toks[i].nl_before = 1;
        }
    }

    let ctx = Ctx { info, gaps, style };
    let mut unit_at: Vec<Option<usize>> = vec![None; n];
    for (k, u) in units.iter().enumerate() {
        unit_at[u.start] = Some(k);
    }

    // Blank lines forced by the style may exceed the general limit.
    let blank_cap = style.max_blank_lines.max(style.blank_lines_after_header).max(style.blank_lines_before_member);

    let mut machine = Machine::new(base);
    let mut lines: Vec<String> = Vec::new();
    let mut line_first = 0usize;

    let mut flush = |toks: &[Tok], machine: &mut Machine, first: usize, last: usize, lines: &mut Vec<String>| {
        let out = machine.layout_line(toks, &ctx, first, last);
        if first > 0 {
            let blanks = toks[first].nl_before.saturating_sub(1).min(blank_cap);
            for _ in 0..blanks {
                lines.push(String::new());
            }
        }
        lines.push(format!("{}{}", indent_string(out.indent, style), out.text));
    };

    for i in 0..n {
        if toks[i].nl_before > 0 && i > line_first {
            flush(toks, &mut machine, line_first, i - 1, &mut lines);
            line_first = i;
        }
        if let Some(k) = unit_at[i] {
            wrap_unit(toks, &ctx, &machine, line_first, &units[k]);
        }
    }
    // The wrap pass may have broken lines inside the last unit: lay out whatever remains line by line.
    for (f, l) in lines_of(toks, line_first, n - 1) {
        flush(toks, &mut machine, f, l, &mut lines);
    }
    lines.join("\n")
}
