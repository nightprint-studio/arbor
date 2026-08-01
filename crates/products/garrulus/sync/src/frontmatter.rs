//! The frontmatter half of a merge: split it off, cut it into top-level fields,
//! and merge those fields *field-wise* rather than line-wise.
//!
//! Why field-wise (`docs/garrulus-design.md` §4.4.1): frontmatter is structured
//! data. Two machines that touched two different fields of the same note have
//! not conflicted, and telling the user they have — because the lines happen to
//! be adjacent — is the kind of noise that makes people stop trusting the sync.
//!
//! Deliberately *not* a YAML parser. This is a conservative, byte-preserving
//! field splitter: a field is a top-level `key:` line plus every indented or
//! list line under it, kept verbatim. Anything it cannot make sense of is left
//! as one opaque chunk, which merges as a unit — worst case a conflict, never a
//! rewrite. That is the price of the hard invariant that untouched frontmatter
//! round-trips byte-stable (§5.1).
//!
//! Splitting is all this module owns: once the block is a list of [`Field`]s,
//! the merge itself is [`crate::keyed`], shared with the metadata merger.

use crate::keyed::{merge_keyed, render_fields, Clash, Field};

/// The `---` fence, as written and as recognised.
const FENCE: &str = "---";

/// Split a note into `(frontmatter_inner, body)`.
///
/// `frontmatter_inner` is the text *between* the fences, fences excluded and
/// without the trailing newline; `None` when the note has no frontmatter. The
/// body is everything after the closing fence's newline.
pub fn split_front(src: &str) -> (Option<&str>, &str) {
    let after_open = match strip_fence_line(src) {
        Some(rest) => rest,
        None => return (None, src),
    };
    // The opening fence must be the very first line; the closing one is the
    // next line that is exactly `---`.
    let mut offset = 0usize;
    for line in after_open.split_inclusive('\n') {
        let bare = line.trim_end_matches('\n').trim_end_matches('\r');
        if bare == FENCE {
            let inner = &after_open[..offset];
            let body_start = offset + line.len();
            let inner = inner.strip_suffix('\n').unwrap_or(inner);
            let inner = inner.strip_suffix('\r').unwrap_or(inner);
            return (Some(inner), &after_open[body_start..]);
        }
        offset += line.len();
    }
    // Unterminated fence: not frontmatter, it is just a document that starts
    // with a horizontal rule. Never guess.
    (None, src)
}

/// Consume a leading `---` line, returning what follows it.
fn strip_fence_line(src: &str) -> Option<&str> {
    let first = src.split_inclusive('\n').next()?;
    let bare = first.trim_end_matches('\n').trim_end_matches('\r');
    if bare == FENCE {
        Some(&src[first.len()..])
    } else {
        None
    }
}

/// Put a note back together from an optional frontmatter block and a body.
pub fn join(front: Option<&str>, body: &str) -> String {
    match front {
        // An empty block is `---\n---`, not `---\n\n---`: the round-trip has to
        // be byte-stable for a note nobody touched.
        Some(f) if f.is_empty() => format!("{FENCE}\n{FENCE}\n{body}"),
        Some(f) => format!("{FENCE}\n{f}\n{FENCE}\n{body}"),
        None => body.to_string(),
    }
}

/// Cut a frontmatter block into its top-level fields, in source order.
///
/// A line that starts at column zero and contains a `:` opens a field;
/// everything indented, every `- ` list line and every blank line belongs to the
/// field above it. Leading junk before the first key (a stray comment) becomes a
/// field with an empty key so it is never dropped.
pub fn parse_fields(front: &str) -> Vec<Field> {
    let mut fields: Vec<Field> = Vec::new();
    for line in front.split('\n') {
        let line = line.trim_end_matches('\r');
        if let Some(key) = top_level_key(line) {
            fields.push(Field { key, raw: line.to_string() });
        } else if let Some(last) = fields.last_mut() {
            last.raw.push('\n');
            last.raw.push_str(line);
        } else {
            fields.push(Field { key: String::new(), raw: line.to_string() });
        }
    }
    fields
}

/// The key of a `key: value` line at column zero, if this is one.
fn top_level_key(line: &str) -> Option<String> {
    if line.is_empty() || line.starts_with(char::is_whitespace) || line.starts_with('-') {
        return None;
    }
    if line.starts_with('#') {
        return None; // a YAML comment belongs to the field above it
    }
    let idx = line.find(':')?;
    let key = line[..idx].trim();
    if key.is_empty() {
        None
    } else {
        Some(key.to_string())
    }
}

/// Merge frontmatter field-wise. `None` means *the same field changed on both
/// sides differently* — the one case the user has to arbitrate.
///
/// `base` is the last common ancestor when there is one (git has it, a folder
/// mirror does not). Without a base, only identical sides can merge: guessing
/// which side is newer is exactly how text gets lost.
pub fn merge_frontmatter(
    base: Option<&str>,
    local: Option<&str>,
    remote: Option<&str>,
) -> Option<Option<String>> {
    if local == remote {
        return Some(local.map(str::to_string));
    }
    if base.is_some() && base == local {
        return Some(remote.map(str::to_string));
    }
    if base.is_some() && base == remote {
        return Some(local.map(str::to_string));
    }
    let (local, remote) = match (local, remote) {
        (Some(l), Some(r)) => (l, r),
        // One side added or dropped the whole block and the other edited it:
        // structural, and not something to resolve behind the user's back.
        _ => return None,
    };
    let base_fields = base.map(parse_fields).unwrap_or_default();
    let local_fields = parse_fields(local);
    let remote_fields = parse_fields(remote);

    // `Clash::Report`: the same field changed on both sides is the one thing a
    // note's frontmatter does not decide behind the user's back (§4.4.1).
    let merged = merge_keyed(&base_fields, &local_fields, &remote_fields, Clash::Report)?;
    Some(Some(render_fields(&merged)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_fenced_block() {
        let src = "---\ntitle: Nota\ntags:\n  - bug\n---\n# Titolo\n\ntesto\n";
        let (front, body) = split_front(src);
        assert_eq!(front, Some("title: Nota\ntags:\n  - bug"));
        assert_eq!(body, "# Titolo\n\ntesto\n");
    }

    #[test]
    fn a_leading_rule_is_not_frontmatter() {
        let src = "---\nsolo una riga\n";
        assert_eq!(split_front(src), (None, src));
        let src2 = "# Titolo\n---\n";
        assert_eq!(split_front(src2), (None, src2));
    }

    #[test]
    fn split_then_join_round_trips_byte_stable() {
        let src = "---\ntitle: Nota\n---\ncorpo\n";
        let (front, body) = split_front(src);
        assert_eq!(join(front, body), src);
        let plain = "solo corpo\n";
        let (front, body) = split_front(plain);
        assert_eq!(join(front, body), plain);
    }

    #[test]
    fn fields_keep_their_continuation_lines() {
        let fields = parse_fields("title: Nota\ntags:\n  - bug\n  - ui\nstatus: aperto");
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[1].key, "tags");
        assert_eq!(fields[1].raw, "tags:\n  - bug\n  - ui");
        assert_eq!(render_fields(&fields), "title: Nota\ntags:\n  - bug\n  - ui\nstatus: aperto");
    }

    #[test]
    fn two_sides_touching_two_fields_merge() {
        let base = "title: Nota\nstatus: aperto\nseverity: major";
        let local = "title: Nota\nstatus: in corso\nseverity: major";
        let remote = "title: Nota\nstatus: aperto\nseverity: blocker";
        let merged = merge_frontmatter(Some(base), Some(local), Some(remote)).unwrap();
        assert_eq!(
            merged.as_deref(),
            Some("title: Nota\nstatus: in corso\nseverity: blocker")
        );
    }

    #[test]
    fn the_same_field_changed_twice_is_a_conflict() {
        let base = "status: aperto";
        assert!(merge_frontmatter(Some(base), Some("status: risolto"), Some("status: in corso"))
            .is_none());
    }

    #[test]
    fn a_field_added_remotely_lands_at_the_end() {
        let base = "title: Nota";
        let local = "title: Nota\nstatus: aperto";
        let remote = "title: Nota\napp: corvus";
        let merged = merge_frontmatter(Some(base), Some(local), Some(remote)).unwrap();
        assert_eq!(merged.as_deref(), Some("title: Nota\nstatus: aperto\napp: corvus"));
    }

    #[test]
    fn an_untouched_field_deleted_on_one_side_goes_away() {
        let base = "title: Nota\nbozza: true";
        let local = "title: Nota";
        let remote = "title: Nota\nbozza: true\napp: corvus";
        let merged = merge_frontmatter(Some(base), Some(local), Some(remote)).unwrap();
        assert_eq!(merged.as_deref(), Some("title: Nota\napp: corvus"));
    }

    #[test]
    fn identical_sides_are_returned_verbatim() {
        let same = "title:   Nota\n\n# commento";
        let merged = merge_frontmatter(None, Some(same), Some(same)).unwrap();
        assert_eq!(merged.as_deref(), Some(same));
    }

    #[test]
    fn without_a_base_differing_sides_do_not_merge() {
        assert!(merge_frontmatter(None, Some("a: 1"), Some("a: 2")).is_none());
    }

    #[test]
    fn an_absent_block_on_one_side_only_merges_when_untouched() {
        // Local dropped the block, remote left it alone -> take local.
        let merged = merge_frontmatter(Some("a: 1"), None, Some("a: 1")).unwrap();
        assert_eq!(merged, None);
        // Local dropped it and remote edited it -> the user decides.
        assert!(merge_frontmatter(Some("a: 1"), None, Some("a: 2")).is_none());
    }
}
