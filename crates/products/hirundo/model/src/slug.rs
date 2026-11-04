//! File-name slugs: what a request called "Lista bandi (v2)" is called on disk.

/// Lower-case ASCII letters and digits, words joined by `-`. Accents fold to their base letter where the
/// language needs it (è → e), anything else drops. Never empty: a name with nothing usable is `item`.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for c in name.chars() {
        let folded = match c {
            'à' | 'á' | 'â' | 'ä' | 'À' | 'Á' | 'Â' | 'Ä' => Some('a'),
            'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => Some('e'),
            'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => Some('i'),
            'ò' | 'ó' | 'ô' | 'ö' | 'Ò' | 'Ó' | 'Ô' | 'Ö' => Some('o'),
            'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => Some('u'),
            c if c.is_ascii_alphanumeric() => Some(c.to_ascii_lowercase()),
            _ => None,
        };
        match folded {
            Some(f) => {
                if pending_dash && !out.is_empty() {
                    out.push('-');
                }
                pending_dash = false;
                out.push(f);
            }
            None => pending_dash = true,
        }
    }
    if out.is_empty() { "item".into() } else { out }
}

/// `slug`, or `slug-2`, `slug-3`… — the first that `taken` does not claim.
pub fn unique(slug: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(slug) {
        return slug.to_string();
    }
    (2..).map(|n| format!("{slug}-{n}")).find(|c| !taken(c)).expect("an unbounded range")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_become_plain_file_names() {
        assert_eq!(slugify("Lista bandi"), "lista-bandi");
        assert_eq!(slugify("  Crea bando (v2) / test!  "), "crea-bando-v2-test");
        assert_eq!(slugify("Perché è così"), "perche-e-cosi");
        assert_eq!(slugify("***"), "item");
        assert_eq!(slugify(""), "item");
        assert_eq!(slugify("A--B"), "a-b");
    }

    #[test]
    fn a_taken_slug_gets_a_number() {
        let taken = ["a", "a-2"];
        assert_eq!(unique("a", |s| taken.contains(&s)), "a-3");
        assert_eq!(unique("b", |s| taken.contains(&s)), "b");
    }
}
