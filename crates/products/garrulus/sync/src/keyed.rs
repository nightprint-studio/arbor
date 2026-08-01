//! The key algebra every keyed three-way merge in this crate shares.
//!
//! Two of the formats the sync engine merges are *maps of verbatim chunks*: a
//! note's YAML frontmatter ([`crate::frontmatter`]) and a `.arbor/garrulus/`
//! metadata file ([`crate::metadata`]). Neither is parsed into values — both are
//! cut into `key` + `raw` chunks and merged key by key, so whatever the splitter
//! did not understand round-trips byte for byte.
//!
//! Exactly one decision differs between them, which is why the algebra lives
//! here once and the decision travels as a parameter: what to do when both
//! machines changed the same key. A note's frontmatter reports it and the user
//! arbitrates (`docs/garrulus-design.md` §4.4.1); a settings file settles it
//! (§4.4.4), because a conflict in a settings file is pure noise.

/// A keyed chunk of a structured file, kept verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// The key, as written before the separator.
    pub key: String,
    /// The whole chunk (the key's line plus its continuation lines), without a
    /// trailing newline.
    pub raw: String,
}

/// What to do with a key both machines changed differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clash {
    /// Report it: the merge fails and the caller hands the file to the user.
    /// What a note's frontmatter gets.
    Report,
    /// Keep the local side and move on. What `.arbor/garrulus/` metadata gets:
    /// "last writer wins", and the last writer is the machine the user is
    /// sitting in front of — the one that just made the edit and would notice
    /// it being silently reverted. The remote value is not lost either: it stays
    /// in the merge's history, which is where a settings value belongs.
    KeepLocal,
}

/// Does a chunk only one side still has survive the merge?
///
/// A *genuine delete* — the surviving side's chunk is byte-identical to the
/// base, so that side never touched it and the other side removed it on purpose
/// — drops it. Everything else keeps it: re-deleting a key is one keystroke,
/// getting a silently deleted one back is archaeology.
pub fn keeps_one_sided(base: Option<&str>, side: &str) -> bool {
    base != Some(side)
}

/// Merge three lists of keyed chunks, key by key.
///
/// Local order is the spine and remote-only keys are appended, so a key the
/// other machine added lands at the end rather than reshuffling the file.
///
/// `None` is only ever returned under [`Clash::Report`].
pub fn merge_keyed(
    base: &[Field],
    local: &[Field],
    remote: &[Field],
    clash: Clash,
) -> Option<Vec<Field>> {
    let find = |fields: &[Field], key: &str| -> Option<String> {
        fields.iter().find(|f| f.key == key).map(|f| f.raw.clone())
    };

    let mut keys: Vec<String> = local.iter().map(|f| f.key.clone()).collect();
    for f in remote {
        if !keys.contains(&f.key) {
            keys.push(f.key.clone());
        }
    }

    let mut merged: Vec<Field> = Vec::new();
    for key in keys {
        let b = find(base, &key);
        let l = find(local, &key);
        let r = find(remote, &key);
        let raw = match (l, r) {
            (Some(l), Some(r)) if l == r => Some(l),
            (Some(l), Some(r)) => {
                if b.as_deref() == Some(l.as_str()) {
                    Some(r)
                } else if b.as_deref() == Some(r.as_str()) {
                    Some(l)
                } else {
                    match clash {
                        Clash::Report => return None,
                        Clash::KeepLocal => Some(l),
                    }
                }
            }
            (Some(l), None) => keeps_one_sided(b.as_deref(), &l).then_some(l),
            (None, Some(r)) => keeps_one_sided(b.as_deref(), &r).then_some(r),
            (None, None) => None,
        };
        if let Some(raw) = raw {
            merged.push(Field { key, raw });
        }
    }
    Some(merged)
}

/// Render chunks back into a block, in the order given.
pub fn render_fields(fields: &[Field]) -> String {
    fields.iter().map(|f| f.raw.as_str()).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(key: &str, raw: &str) -> Field {
        Field { key: key.to_string(), raw: raw.to_string() }
    }

    #[test]
    fn two_sides_touching_two_keys_merge_under_either_policy() {
        let base = [f("a", "a = 1"), f("b", "b = 1")];
        let local = [f("a", "a = 2"), f("b", "b = 1")];
        let remote = [f("a", "a = 1"), f("b", "b = 2")];
        for clash in [Clash::Report, Clash::KeepLocal] {
            let merged = merge_keyed(&base, &local, &remote, clash).unwrap();
            assert_eq!(merged, vec![f("a", "a = 2"), f("b", "b = 2")]);
        }
    }

    #[test]
    fn the_clash_policy_is_the_only_difference() {
        let base = [f("a", "a = 1")];
        let local = [f("a", "a = 2")];
        let remote = [f("a", "a = 3")];
        assert!(merge_keyed(&base, &local, &remote, Clash::Report).is_none());
        assert_eq!(
            merge_keyed(&base, &local, &remote, Clash::KeepLocal).unwrap(),
            vec![f("a", "a = 2")]
        );
    }

    #[test]
    fn a_remote_only_key_lands_at_the_end() {
        let merged =
            merge_keyed(&[], &[f("a", "a = 1")], &[f("z", "z = 1")], Clash::KeepLocal).unwrap();
        assert_eq!(merged, vec![f("a", "a = 1"), f("z", "z = 1")]);
    }

    #[test]
    fn only_an_untouched_key_is_deleted() {
        // Remote dropped `b` and local never touched it -> gone.
        let merged = merge_keyed(
            &[f("b", "b = 1")],
            &[f("b", "b = 1")],
            &[],
            Clash::KeepLocal,
        )
        .unwrap();
        assert!(merged.is_empty());
        // Remote dropped `b` while local edited it -> the edit survives.
        let merged = merge_keyed(
            &[f("b", "b = 1")],
            &[f("b", "b = 2")],
            &[],
            Clash::KeepLocal,
        )
        .unwrap();
        assert_eq!(merged, vec![f("b", "b = 2")]);
    }

    #[test]
    fn one_sided_survival_needs_a_base_that_matches() {
        assert!(!keeps_one_sided(Some("a = 1"), "a = 1"));
        assert!(keeps_one_sided(Some("a = 1"), "a = 2"));
        assert!(keeps_one_sided(None, "a = 1"));
    }
}
