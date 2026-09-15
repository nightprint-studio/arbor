//! The builder every template writes its expansion with, and the indentation step it indents by.

use super::{Expansion, Stop, Written};

/// An expansion being written: text, the stops placed in it, the imports it needs.
///
/// A builder rather than snippet syntax, because the subject is spliced in verbatim and Java allows
/// `$` in an identifier — `order$1.nn` in a `$1`-style template would be read as a stop.
pub(crate) struct Body {
    detail: String,
    text: String,
    stops: Vec<Stop>,
    imports: Vec<String>,
}

impl Body {
    /// An empty expansion the popup describes as `detail`.
    pub(crate) fn new(detail: impl Into<String>) -> Self {
        Self { detail: detail.into(), text: String::new(), stops: Vec::new(), imports: Vec::new() }
    }

    pub(crate) fn push(mut self, text: &str) -> Self {
        self.text.push_str(text);
        self
    }

    /// `placeholder`, selected when Tab reaches it. Stops of one non-zero `group` are one value.
    pub(crate) fn stop(mut self, placeholder: &str, group: u32) -> Self {
        let start = self.text.len();
        self.text.push_str(placeholder);
        self.stops.push(Stop { start, end: self.text.len(), group });
        self
    }

    /// Where the caret lands once the named stops are filled in.
    pub(crate) fn caret(self) -> Self {
        self.stop("", 0)
    }

    pub(crate) fn import(mut self, fqn: &str) -> Self {
        self.imports.push(fqn.to_string());
        self
    }

    /// The imports a written type needs.
    pub(crate) fn imports_of(mut self, written: &Written) -> Self {
        self.imports.extend(written.imports.iter().cloned());
        self
    }

    /// ` {`, a line one step in with the caret on it, `}`.
    pub(crate) fn block(self, unit: &str) -> Self {
        self.push(" {\n").push(unit).caret().push("\n}")
    }

    pub(crate) fn finish(self, name: &'static str) -> Expansion {
        let mut imports = self.imports;
        imports.sort();
        imports.dedup();
        Expansion { name, detail: self.detail, text: self.text, stops: visiting_order(self.stops), imports }
    }
}

/// The stops in the order Tab visits them: each group straight behind its first stop.
///
/// The editor skips the rest of a group when it leaves one of its stops, and it walks the list in
/// order — so a group whose stops were interleaved with another's (`found` declared, then tested, then
/// read, with the value's name in between) would be visited once per run rather than once.
fn visiting_order(stops: Vec<Stop>) -> Vec<Stop> {
    let mut out = Vec::with_capacity(stops.len());
    for (i, stop) in stops.iter().enumerate() {
        if stop.group != 0 && stops[..i].iter().any(|earlier| earlier.group == stop.group) {
            continue;
        }
        out.push(*stop);
        if stop.group != 0 {
            out.extend(stops[i + 1..].iter().filter(|later| later.group == stop.group).copied());
        }
    }
    out
}

/// One indentation step, read off the file rather than assumed: a tab if its lines are indented with
/// tabs, else the smallest indentation any line uses — `4` when that says nothing useful.
///
/// Javadoc continuation lines (` * `) are skipped: their one space is alignment, not a step.
pub fn indent_unit(source: &str) -> String {
    let mut smallest: Option<usize> = None;
    for line in source.lines() {
        let content = line.trim_start_matches([' ', '\t']);
        if content.is_empty() || content.starts_with('*') {
            continue;
        }
        let lead = &line[..line.len() - content.len()];
        if lead.starts_with('\t') {
            return "\t".to_string();
        }
        if !lead.is_empty() {
            smallest = Some(smallest.map_or(lead.len(), |s| s.min(lead.len())));
        }
    }
    " ".repeat(smallest.filter(|n| (2..=8).contains(n)).unwrap_or(4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_step_is_the_smallest_indentation_the_file_uses() {
        let src = "class A {\n  void m() {\n    run();\n  }\n}\n";
        assert_eq!(indent_unit(src), "  ");
    }

    #[test]
    fn a_file_indented_with_tabs_steps_by_a_tab() {
        assert_eq!(indent_unit("class A {\n\tvoid m() {}\n}\n"), "\t");
    }

    /// The single space in front of a Javadoc `*` is alignment; reading it as the step would indent
    /// every block by one space.
    #[test]
    fn javadoc_alignment_is_not_a_step() {
        let src = "/**\n * Doc.\n */\nclass A {\n    int x;\n}\n";
        assert_eq!(indent_unit(src), "    ");
    }

    #[test]
    fn a_file_with_no_indentation_gets_four_spaces() {
        assert_eq!(indent_unit("class A {}\n"), "    ");
    }

    #[test]
    fn stops_are_byte_ranges_into_the_text_in_the_order_written() {
        let expansion = Body::new("d").push("for (int ").stop("i", 1).push(" = 0;").caret().finish("fori");
        assert_eq!(expansion.text, "for (int i = 0;");
        assert_eq!(expansion.stops, vec![Stop { start: 9, end: 10, group: 1 }, Stop { start: 15, end: 15, group: 0 }]);
    }
}
