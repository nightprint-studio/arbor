//! Removing a per-line prefix without losing the source offsets.
//!
//! A block quote and a list item both have the same shape: their content is the
//! block's text with something stripped off the front of every line (`> `, a
//! `- ` marker, four spaces of continuation indent). The clean way to read that
//! content is to strip the prefix and parse the result as a document in its own
//! right — which is exactly what [`reader`](crate::reader) does.
//!
//! The catch is spans. `Span` is documented as byte offsets **into the note**,
//! and after stripping, offsets in the derived text no longer line up with
//! anything. So the strip records where each line landed, and [`Offsets`] maps
//! back. Without it, a `[[wikilink]]` inside a quoted list item would report a
//! range drifting by two bytes per line of nesting and the editor would
//! highlight the wrong text — the kind of bug that only shows up in real notes.

/// How a local byte offset becomes an offset into the note.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Offsets<'a> {
    /// The text is a contiguous slice of the note starting at this offset.
    Linear(usize),
    /// The text was derived by stripping a per-line prefix; `shift` lets a
    /// caller address a sub-slice of it without rebuilding the map.
    Mapped { map: &'a Unprefixed, shift: usize },
}

impl<'a> Offsets<'a> {
    /// The note offset for byte `at` of the local text.
    pub fn at(&self, at: usize) -> usize {
        match *self {
            Offsets::Linear(base) => base + at,
            Offsets::Mapped { map, shift } => map.to_source(shift + at),
        }
    }

    /// The same mapping, addressed from byte `start` of the local text.
    pub fn sub(&self, start: usize) -> Offsets<'a> {
        match *self {
            Offsets::Linear(base) => Offsets::Linear(base + start),
            Offsets::Mapped { map, shift } => Offsets::Mapped {
                map,
                shift: shift + start,
            },
        }
    }
}

/// Text with a per-line prefix removed, plus the map back to the note.
#[derive(Debug)]
pub(crate) struct Unprefixed {
    pub text: String,
    /// `(offset in `text`, offset in the note)`, one per line, ascending.
    marks: Vec<(usize, usize)>,
}

impl Unprefixed {
    /// Strip a prefix from every line of `src`.
    ///
    /// `src` is a slice of the parent text starting at `local_start`, and
    /// `parent` maps that parent text to the note. `strip` is handed the
    /// zero-based line number and the line (newline included) and returns how
    /// many **bytes** to drop; it is `FnMut` because a list item's continuation
    /// indent is decided by its first line.
    pub fn build(
        src: &str,
        local_start: usize,
        parent: &Offsets<'_>,
        mut strip: impl FnMut(usize, &str) -> usize,
    ) -> Self {
        let mut text = String::with_capacity(src.len());
        let mut marks = Vec::new();
        let mut line_start = 0usize;
        let mut index = 0usize;
        loop {
            let newline = src[line_start..].find('\n').map(|i| line_start + i);
            let line_end = newline.map_or(src.len(), |i| i + 1);
            let line = &src[line_start..line_end];
            // Never eat the newline itself: an emptied line must stay a line,
            // or two paragraphs merge into one.
            let limit = line.len() - line.ends_with('\n') as usize;
            let drop = strip(index, line).min(limit);
            marks.push((text.len(), parent.at(local_start + line_start + drop)));
            text.push_str(&line[drop..]);
            index += 1;
            match newline {
                Some(_) if line_end < src.len() => line_start = line_end,
                _ => break,
            }
        }
        Self { text, marks }
    }

    /// The note offset for byte `at` of [`text`](Self::text).
    pub fn to_source(&self, at: usize) -> usize {
        match self.marks.binary_search_by_key(&at, |(local, _)| *local) {
            // Several lines can share a local offset once they are emptied;
            // the *last* of them is the one whose content starts here.
            Ok(found) => {
                let mut i = found;
                while i + 1 < self.marks.len() && self.marks[i + 1].0 == at {
                    i += 1;
                }
                self.marks[i].1
            }
            Err(0) => self.marks.first().map_or(0, |m| m.1),
            Err(next) => {
                let (local, source) = self.marks[next - 1];
                source + (at - local)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The block-quote strip: optional indent, `>`, one optional space.
    fn quote(_line_no: usize, line: &str) -> usize {
        let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
        let rest = &line[indent..];
        match rest.strip_prefix('>') {
            Some(after) => indent + 1 + after.starts_with(' ') as usize,
            None => 0,
        }
    }

    #[test]
    fn strips_a_quote_and_maps_every_line_back() {
        let note = "> prima\n> seconda\n";
        let up = Unprefixed::build(note, 0, &Offsets::Linear(0), quote);
        assert_eq!(up.text, "prima\nseconda\n");
        // `prima` starts at note byte 2, `seconda` at note byte 10.
        assert_eq!(up.to_source(0), 2);
        assert_eq!(up.to_source(6), 10);
        // Inside a line the mapping is linear again.
        assert_eq!(up.to_source(8), 12);
    }

    #[test]
    fn an_emptied_line_keeps_its_newline() {
        let up = Unprefixed::build("> a\n>\n> b\n", 0, &Offsets::Linear(0), quote);
        assert_eq!(up.text, "a\n\nb\n");
    }

    #[test]
    fn multibyte_prefixes_do_not_shift_the_map() {
        // 'à' is two bytes: a char-counting map would report 12 for `nota`.
        let note = "> città\n> nota\n";
        let up = Unprefixed::build(note, 0, &Offsets::Linear(0), quote);
        assert_eq!(up.text, "città\nnota\n");
        assert_eq!(up.to_source(0), 2);
        assert_eq!(&note[up.to_source(7)..], "nota\n");
    }

    #[test]
    fn a_parent_offset_is_carried_through() {
        let up = Unprefixed::build("> x\n", 0, &Offsets::Linear(500), quote);
        assert_eq!(up.to_source(0), 502);
    }

    #[test]
    fn nesting_composes_because_marks_are_absolute() {
        let note = "> > interno\n";
        let outer = Unprefixed::build(note, 0, &Offsets::Linear(0), quote);
        assert_eq!(outer.text, "> interno\n");
        let outer_offsets = Offsets::Mapped {
            map: &outer,
            shift: 0,
        };
        let inner = Unprefixed::build(&outer.text, 0, &outer_offsets, quote);
        assert_eq!(inner.text, "interno\n");
        // `interno` really does start at note byte 4.
        assert_eq!(inner.to_source(0), 4);
        assert_eq!(&note[inner.to_source(0)..], "interno\n");
    }

    #[test]
    fn sub_shifts_without_rebuilding_the_map() {
        let up = Unprefixed::build("> abc\n> def\n", 0, &Offsets::Linear(0), quote);
        let offsets = Offsets::Mapped {
            map: &up,
            shift: 0,
        };
        assert_eq!(offsets.sub(4).at(0), offsets.at(4));
        assert_eq!(offsets.sub(4).at(2), offsets.at(6));
    }
}
