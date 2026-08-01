//! Three-way line merge — the body half of a note merge.
//!
//! Classic diff3: match the base against each side, keep the lines both sides
//! agree on as anchors, and reconcile each unstable chunk between two anchors.
//! A chunk only one side changed takes that side; a chunk both sides changed the
//! same way takes it once; a chunk both sides changed differently is a conflict
//! and the *caller* decides what to do with it — this module never invents
//! merge markers (`docs/garrulus-design.md` §4.4.3).
//!
//! Written here rather than pulled in as a dependency because it is eighty lines
//! of well-understood algorithm and because the failure mode we care about
//! (silently losing a line) is exactly the thing to own and test.

/// Above this many `base × side` cells the LCS table stops being worth its
/// memory. A note that large is not prose any more, and reporting a conflict on
/// it is honest: nothing is lost, the user arbitrates.
const MAX_LCS_CELLS: usize = 1_000_000;

/// Merge three texts line-wise. `None` means the sides changed the same region
/// differently.
///
/// The trailing newline follows `local`: whichever side the user is sitting in
/// front of decides the shape of the file they are looking at.
pub fn merge_text3(base: Option<&str>, local: &str, remote: &str) -> Option<String> {
    if local == remote {
        return Some(local.to_string());
    }
    let base = base?;
    if base == local {
        return Some(remote.to_string());
    }
    if base == remote {
        return Some(local.to_string());
    }
    let (b, l, r) = (lines(base), lines(local), lines(remote));
    let merged = merge_lines3(&b, &l, &r)?;
    let mut out = merged.join("\n");
    if local.ends_with('\n') && !out.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

/// Split into lines, dropping the artefact empty line a trailing newline
/// produces so that "same text plus a final newline" is not a whole-file diff.
fn lines(s: &str) -> Vec<&str> {
    let body = s.strip_suffix('\n').unwrap_or(s);
    if body.is_empty() && s.is_empty() {
        return Vec::new();
    }
    body.split('\n').collect()
}

/// The diff3 core. `None` on a genuine conflict.
pub fn merge_lines3(base: &[&str], local: &[&str], remote: &[&str]) -> Option<Vec<String>> {
    if base.len().saturating_mul(local.len()) > MAX_LCS_CELLS
        || base.len().saturating_mul(remote.len()) > MAX_LCS_CELLS
    {
        return None;
    }
    let ml = lcs_matches(base, local);
    let mr = lcs_matches(base, remote);

    // Anchors: base lines matched on BOTH sides, in increasing order on all
    // three. Those are the lines nobody touched, and they frame the chunks.
    let mut anchors: Vec<(usize, usize, usize)> = Vec::new();
    let mut ri = 0usize;
    for &(bi, li) in &ml {
        while ri < mr.len() && mr[ri].0 < bi {
            ri += 1;
        }
        if ri < mr.len() && mr[ri].0 == bi {
            anchors.push((bi, li, mr[ri].1));
        }
    }

    let mut out: Vec<String> = Vec::new();
    let (mut bp, mut lp, mut rp) = (0usize, 0usize, 0usize);
    for (bi, li, rix) in anchors {
        let chunk = reconcile(&base[bp..bi], &local[lp..li], &remote[rp..rix])?;
        out.extend(chunk);
        out.push(base[bi].to_string());
        bp = bi + 1;
        lp = li + 1;
        rp = rix + 1;
    }
    let tail = reconcile(&base[bp..], &local[lp..], &remote[rp..])?;
    out.extend(tail);
    Some(out)
}

/// Resolve one unstable chunk.
fn reconcile(base: &[&str], local: &[&str], remote: &[&str]) -> Option<Vec<String>> {
    let take = |side: &[&str]| side.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    if local == remote {
        return Some(take(local));
    }
    if local == base {
        return Some(take(remote));
    }
    if remote == base {
        return Some(take(local));
    }
    None
}

/// Longest common subsequence as `(index_in_a, index_in_b)` pairs.
fn lcs_matches(a: &[&str], b: &[&str]) -> Vec<(usize, usize)> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let (n, m) = (a.len(), b.len());
    // table[i][j] = LCS length of a[i..] and b[j..], flattened.
    let mut table = vec![0u32; (n + 1) * (m + 1)];
    let idx = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[idx(i, j)] = if a[i] == b[j] {
                table[idx(i + 1, j + 1)] + 1
            } else {
                table[idx(i + 1, j)].max(table[idx(i, j + 1)])
            };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if table[idx(i + 1, j)] >= table[idx(i, j + 1)] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_sides_need_no_base() {
        assert_eq!(merge_text3(None, "uguale\n", "uguale\n").as_deref(), Some("uguale\n"));
        assert_eq!(merge_text3(None, "a\n", "b\n"), None);
    }

    #[test]
    fn one_side_untouched_takes_the_other() {
        let base = "riga 1\nriga 2\n";
        assert_eq!(merge_text3(Some(base), base, "riga 1\nriga 2 mod\n").as_deref(),
                   Some("riga 1\nriga 2 mod\n"));
    }

    #[test]
    fn edits_in_different_places_merge() {
        let base = "# Nota\n\nprimo\nsecondo\nterzo\n";
        let local = "# Nota modificata\n\nprimo\nsecondo\nterzo\n";
        let remote = "# Nota\n\nprimo\nsecondo\nterzo\nquarto\n";
        assert_eq!(
            merge_text3(Some(base), local, remote).as_deref(),
            Some("# Nota modificata\n\nprimo\nsecondo\nterzo\nquarto\n")
        );
    }

    #[test]
    fn both_inserting_in_different_places_merges() {
        let base = "a\nb\nc\n";
        let local = "a\nlocale\nb\nc\n";
        let remote = "a\nb\nc\nremoto\n";
        assert_eq!(
            merge_text3(Some(base), local, remote).as_deref(),
            Some("a\nlocale\nb\nc\nremoto\n")
        );
    }

    #[test]
    fn the_same_line_edited_twice_conflicts() {
        let base = "titolo\n";
        assert_eq!(merge_text3(Some(base), "titolo mio\n", "titolo suo\n"), None);
    }

    #[test]
    fn both_sides_making_the_same_edit_apply_it_once() {
        let base = "a\nb\n";
        let same = "a\nb\nc\n";
        assert_eq!(merge_text3(Some(base), same, same).as_deref(), Some(same));
    }

    #[test]
    fn a_deletion_on_one_side_is_kept() {
        let base = "a\nb\nc\n";
        let local = "a\nc\n";
        let remote = "a\nb\nc\nd\n";
        assert_eq!(merge_text3(Some(base), local, remote).as_deref(), Some("a\nc\nd\n"));
    }

    #[test]
    fn lcs_finds_the_common_spine() {
        let a = ["a", "b", "c"];
        let b = ["a", "x", "c"];
        assert_eq!(lcs_matches(&a, &b), vec![(0, 0), (2, 2)]);
    }
}
