//! What became of a ghost text: the text the editor drew, against the text that stood in its place afterwards.
//!
//! Pure functions on two strings, so the rule is testable without an editor. The outcome keeps the **shape** of
//! what was swapped (`null` for a name, a number for a string), never the text itself: the shape is what a layer
//! can learn from, and the text is the user's code.

use serde::{Deserialize, Serialize};

/// What kind of thing a piece of text is, coarsely.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shape {
    Nothing,
    Null,
    Number,
    Text,
    Name,
    Call,
    Other,
}

/// How the whitespace of a ghost text was changed while its content stood.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Layout {
    /// A line was broken where the ghost had none.
    LineBroken,
    /// Lines the ghost had broken were joined.
    LinesJoined,
    /// The indentation at the start of a line changed.
    Reindented,
    /// A space was added inside a line (`a,b` → `a, b`).
    SpaceAdded,
    /// A space was removed inside a line.
    SpaceRemoved,
}

/// What happened to a ghost text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// Shown and never taken.
    Ignored,
    /// Taken and left as it was.
    Kept,
    /// Taken, then only its whitespace was changed: the content was right and the layout was not the user's. Not a
    /// correction of what was suggested, and kept apart from [`Outcome::Replaced`] for that reason.
    Reformatted { layout: Layout },
    /// Taken, then the tail was deleted: it started right and went on too long.
    Shortened { cut: usize },
    /// Taken, then written on: it was right and too short.
    Extended { added: usize },
    /// Taken, then a middle part was swapped for another.
    Replaced { from: Shape, to: Shape },
    /// Taken, then removed whole.
    Undone,
}

/// Read what stands where the ghost was drawn. `after` is `None` when the ghost was never accepted.
pub fn classify(ghost: &str, after: Option<&str>) -> Outcome {
    let Some(after) = after else { return Outcome::Ignored };
    if after == ghost {
        return Outcome::Kept;
    }
    if after.trim().is_empty() {
        return Outcome::Undone;
    }
    if let Some(layout) = layout_change(ghost, after) {
        return Outcome::Reformatted { layout };
    }
    if let Some(rest) = after.strip_prefix(ghost) {
        return Outcome::Extended { added: rest.len() };
    }
    if let Some(rest) = ghost.strip_prefix(after) {
        return Outcome::Shortened { cut: rest.len() };
    }
    let head = common_prefix(ghost, after);
    let tail = common_suffix(&ghost[head..], &after[head..]);
    let (from, to) = (&ghost[head..ghost.len() - tail], &after[head..after.len() - tail]);
    Outcome::Replaced { from: shape_of(from), to: shape_of(to) }
}

/// The way `after` re-lays out `ghost`, if the two are the same text but for whitespace.
fn layout_change(ghost: &str, after: &str) -> Option<Layout> {
    let bare = |t: &str| t.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    if bare(ghost) != bare(after) {
        return None;
    }
    let (g, a) = (Whitespace::of(ghost), Whitespace::of(after));
    Some(if a.newlines > g.newlines {
        Layout::LineBroken
    } else if a.newlines < g.newlines {
        Layout::LinesJoined
    } else if a.indent != g.indent {
        Layout::Reindented
    } else if a.inner > g.inner {
        Layout::SpaceAdded
    } else {
        Layout::SpaceRemoved
    })
}

/// The whitespace of a text, in the three kinds a style is made of.
struct Whitespace {
    newlines: usize,
    /// Whitespace that starts a line.
    indent: usize,
    /// Whitespace inside a line.
    inner: usize,
}

impl Whitespace {
    fn of(text: &str) -> Self {
        let mut w = Whitespace { newlines: 0, indent: 0, inner: 0 };
        let mut at_line_start = false;
        for c in text.chars() {
            match c {
                '\n' => {
                    w.newlines += 1;
                    at_line_start = true;
                }
                c if c.is_whitespace() => {
                    if at_line_start { w.indent += 1 } else { w.inner += 1 }
                }
                _ => at_line_start = false,
            }
        }
        w
    }
}

/// The coarse kind of `text`.
pub fn shape_of(text: &str) -> Shape {
    let t = text.trim();
    if t.is_empty() {
        return Shape::Nothing;
    }
    if t == "null" {
        return Shape::Null;
    }
    if t.starts_with('"') {
        return Shape::Text;
    }
    if t.parse::<f64>().is_ok() || t.trim_end_matches(['L', 'l', 'f', 'd']).parse::<f64>().is_ok() {
        return Shape::Number;
    }
    if t.contains('(') {
        return Shape::Call;
    }
    if t.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
        return Shape::Name;
    }
    Shape::Other
}

/// Bytes of the longest common prefix, cut on a character boundary.
fn common_prefix(a: &str, b: &str) -> usize {
    a.char_indices()
        .zip(b.chars())
        .take_while(|((_, x), y)| x == y)
        .last()
        .map_or(0, |((i, x), _)| i + x.len_utf8())
}

/// Bytes of the longest common suffix, cut on a character boundary.
fn common_suffix(a: &str, b: &str) -> usize {
    a.chars()
        .rev()
        .zip(b.chars().rev())
        .take_while(|(x, y)| x == y)
        .map(|(x, _)| x.len_utf8())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ghost_never_taken_is_ignored() {
        assert_eq!(classify("foo()", None), Outcome::Ignored);
    }

    #[test]
    fn a_ghost_left_alone_is_kept() {
        assert_eq!(classify("foo()", Some("foo()")), Outcome::Kept);
    }

    #[test]
    fn a_deleted_tail_is_a_shortening_and_a_written_one_an_extension() {
        assert_eq!(classify("foo(a, b)", Some("foo(a")), Outcome::Shortened { cut: 4 });
        assert_eq!(classify("foo(a", Some("foo(a, b)")), Outcome::Extended { added: 4 });
    }

    #[test]
    fn a_swapped_middle_keeps_the_shape_of_both_sides() {
        let out = classify("addAll(items, null)", Some("addAll(items, overrides)"));
        assert_eq!(out, Outcome::Replaced { from: Shape::Null, to: Shape::Name });
    }

    #[test]
    fn nothing_left_of_it_is_an_undo() {
        assert_eq!(classify("foo()", Some("  ")), Outcome::Undone);
    }

    #[test]
    fn a_swap_next_to_a_multibyte_character_cuts_on_a_boundary() {
        let out = classify("\"è\" + a", Some("\"è\" + b"));
        assert_eq!(out, Outcome::Replaced { from: Shape::Name, to: Shape::Name });
    }

    #[test]
    fn a_change_of_whitespace_is_not_a_change_of_content() {
        let reformatted = |l| Outcome::Reformatted { layout: l };
        assert_eq!(classify("foo(a,b)", Some("foo(a, b)")), reformatted(Layout::SpaceAdded));
        assert_eq!(classify("foo(a, b)", Some("foo(a,b)")), reformatted(Layout::SpaceRemoved));
        assert_eq!(classify("foo(a, b)", Some("foo(a,\n    b)")), reformatted(Layout::LineBroken));
        assert_eq!(classify("foo(a,\n    b)", Some("foo(a, b)")), reformatted(Layout::LinesJoined));
        assert_eq!(classify("foo(a,\n    b)", Some("foo(a,\n        b)")), reformatted(Layout::Reindented));
    }

    #[test]
    fn a_reformat_with_a_real_edit_is_still_an_edit() {
        let out = classify("foo(a,b)", Some("foo(a, c)"));
        assert!(matches!(out, Outcome::Replaced { .. }), "{out:?}");
    }

    #[test]
    fn shapes() {
        assert_eq!(shape_of("42L"), Shape::Number);
        assert_eq!(shape_of("\"x\""), Shape::Text);
        assert_eq!(shape_of("List.of(a)"), Shape::Call);
        assert_eq!(shape_of("a.b"), Shape::Name);
        assert_eq!(shape_of("a + b"), Shape::Other);
        assert_eq!(shape_of(" "), Shape::Nothing);
    }
}
