//! Project-relative paths, in one place.
//!
//! Every path in the script half is relative to the repository root with POSIX
//! separators, including on Windows — that is the identity of a file and of a
//! folder everywhere in Picus. The three questions asked of one are asked from
//! four modules, so they are answered here rather than four times.
//!
//! The empty string is the root itself, and it is a legitimate path: a
//! declaration on `""` applies to the whole repository.

/// Everything before the last separator. `""` for a top-level path, and for the
/// root itself.
pub fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(index) => &path[..index],
        None => "",
    }
}

/// The last segment — what a folder is called, and what inference reads.
pub fn last_segment(path: &str) -> &str {
    match path.rfind('/') {
        Some(index) => &path[index + 1..],
        None => path,
    }
}

/// A path and every folder above it, nearest first, ending at the root `""`.
///
/// `"A/B/C"` → `["A/B/C", "A/B", "A", ""]`. This is the order a lookup with
/// inheritance wants: the first ancestor that declares something wins.
pub fn self_and_ancestors(path: &str) -> Vec<&str> {
    let mut out = vec![path];
    let mut current = path;
    while !current.is_empty() {
        current = parent_of(current);
        out.push(current);
    }
    out
}

/// Is `folder` the path of a folder that contains `path`, at any depth?
pub fn contains(folder: &str, path: &str) -> bool {
    if folder.is_empty() {
        return true;
    }
    path.len() > folder.len()
        && path.starts_with(folder)
        && path.as_bytes()[folder.len()] == b'/'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_walks_up_to_the_root() {
        assert_eq!(self_and_ancestors("A/B/C"), ["A/B/C", "A/B", "A", ""]);
        assert_eq!(self_and_ancestors("A"), ["A", ""]);
        assert_eq!(self_and_ancestors(""), [""]);
    }

    #[test]
    fn the_root_contains_everything_and_a_prefix_is_not_a_parent() {
        assert!(contains("", "ORACLE/x.sql"));
        assert!(contains("ORACLE", "ORACLE/AGGIORNAMENTO/x.sql"));
        // `ORACLE_OLD` starts with `ORACLE` and is a different folder.
        assert!(!contains("ORACLE", "ORACLE_OLD/x.sql"));
        assert!(!contains("ORACLE", "ORACLE"));
    }

    #[test]
    fn the_last_segment_is_what_inference_reads() {
        assert_eq!(last_segment("AGGIORNAMENTO/2024/ORA"), "ORA");
        assert_eq!(last_segment("ORACLE"), "ORACLE");
        assert_eq!(last_segment(""), "");
        assert_eq!(parent_of("AGGIORNAMENTO/2024/ORA"), "AGGIORNAMENTO/2024");
        assert_eq!(parent_of("ORACLE"), "");
    }
}
