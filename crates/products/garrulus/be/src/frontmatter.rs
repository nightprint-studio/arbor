//! A pure frontmatter key setter — the one piece of note *syntax* this backend
//! knows, and the smallest one it could get away with.
//!
//! It exists for `apply_type`, which has to write `type: bug` into a note that may
//! or may not already have a YAML block, without disturbing a single other byte.
//! That last clause is the whole design: **frontmatter must round-trip byte-stable
//! when untouched** (`docs/garrulus-design.md` §5.1). Re-emitting the block through
//! a YAML serializer would reformat quoting, key order and indentation, and every
//! note in the vault would become a diff the first time Garrulus opened it — which
//! would make the sync history worthless. So this edits one line and copies the
//! rest verbatim, including the file's own line endings.
//!
//! It is deliberately *not* a YAML parser: it recognises a top-level `key:` at
//! column zero inside the fence and nothing more. A note whose frontmatter needs
//! more than that (a `type` nested under a mapping) is a note this operation
//! declines to guess about.
//!
//! **Where this belongs long-term**: `garrulus-ast` owns the document model and
//! `garrulus-parse` owns the markdown spelling of it, so a frontmatter *mutation*
//! is theirs, not the backend's. It sits here because it is 40 lines against a
//! cross-crate API that does not exist yet; move it the moment a second caller
//! appears.

/// The YAML frontmatter fence, at the very start of the file.
const FENCE: &str = "---";

/// Set (or insert) a top-level frontmatter key, returning the new source.
///
/// Three cases, in the order they are tried:
/// - the key already exists at the top level of an existing block → that one line
///   is replaced;
/// - a block exists without the key → the key is appended as the block's last
///   line;
/// - no block → one is created at the top of the file.
pub fn set_key(source: &str, key: &str, value: &str) -> String {
    let newline = if source.contains("\r\n") { "\r\n" } else { "\n" };
    let line = format!("{key}: {value}");

    let Some((start, end)) = fence_bounds(source) else {
        // No frontmatter: open one. The blank line after the closing fence keeps
        // the body a paragraph rather than a lazy continuation of the block.
        return format!("{FENCE}{newline}{line}{newline}{FENCE}{newline}{newline}{source}");
    };

    let mut lines: Vec<String> = source.lines().map(|l| l.to_string()).collect();
    match lines[start..end].iter().position(|l| is_key_line(l, key)) {
        Some(offset) => lines[start + offset] = line,
        None => lines.insert(end, line),
    }
    let mut out = lines.join(newline);
    // `lines()` drops a trailing newline; put it back so a file that ended with
    // one still does. Anything else would be a one-byte diff on every note.
    if source.ends_with('\n') {
        out.push_str(newline);
    }
    out
}

/// The half-open line range **between** the fences of a leading frontmatter block,
/// or `None` when the source does not open with one.
fn fence_bounds(source: &str) -> Option<(usize, usize)> {
    let mut lines = source.lines();
    if lines.next()?.trim_end() != FENCE {
        return None;
    }
    let end = lines.position(|l| l.trim_end() == FENCE)? + 1;
    Some((1, end))
}

/// Whether a line declares `key` at the top level of the block (column zero, so an
/// indented `key:` nested under something else is left alone).
fn is_key_line(line: &str, key: &str) -> bool {
    line.strip_prefix(key)
        .map(|rest| rest.trim_start().starts_with(':'))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_an_existing_key_and_touches_nothing_else() {
        let src = "---\ntitle: Crash\ntype: nota\ntags: [a, b]\n---\n\n# Crash\n";
        let out = set_key(src, "type", "bug");
        assert_eq!(out, "---\ntitle: Crash\ntype: bug\ntags: [a, b]\n---\n\n# Crash\n");
    }

    #[test]
    fn appends_a_missing_key_at_the_end_of_the_block() {
        let src = "---\ntitle: Crash\n---\n\nbody\n";
        let out = set_key(src, "type", "bug");
        assert_eq!(out, "---\ntitle: Crash\ntype: bug\n---\n\nbody\n");
    }

    #[test]
    fn creates_a_block_when_there_is_none() {
        let out = set_key("# Crash\n", "type", "bug");
        assert_eq!(out, "---\ntype: bug\n---\n\n# Crash\n");
    }

    #[test]
    fn keeps_crlf_line_endings() {
        let src = "---\r\ntitle: Crash\r\n---\r\n\r\nbody\r\n";
        let out = set_key(src, "type", "bug");
        assert_eq!(out, "---\r\ntitle: Crash\r\ntype: bug\r\n---\r\n\r\nbody\r\n");
    }

    #[test]
    fn a_file_without_a_trailing_newline_keeps_not_having_one() {
        let out = set_key("---\ntitle: X\n---\nbody", "type", "bug");
        assert_eq!(out, "---\ntitle: X\ntype: bug\n---\nbody");
    }

    #[test]
    fn an_indented_key_is_not_the_top_level_one() {
        let src = "---\nmeta:\n  type: inner\n---\nbody\n";
        let out = set_key(src, "type", "bug");
        assert_eq!(
            out, "---\nmeta:\n  type: inner\ntype: bug\n---\nbody\n",
            "the nested key is left alone and a real top-level one is added"
        );
    }

    #[test]
    fn a_dashes_line_further_down_is_not_a_fence() {
        // The block must OPEN the file; a horizontal rule mid-note is not one.
        let src = "# Title\n\n---\n\nbody\n";
        let out = set_key(src, "type", "bug");
        assert!(out.starts_with("---\ntype: bug\n---\n\n# Title"));
    }
}
