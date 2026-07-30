//! `$` — the row number, inside a `{…}` template.
//!
//! Emmet's numbering, because Emmet is where the `*3{…}` shape comes from and a
//! user who already knows one of them should not have to learn a second dialect of
//! the same idea:
//!
//! | Written | Row 1 of 3 | Row 2 | Row 3 |
//! |---|---|---|---|
//! | `$`     | `1`   | `2`   | `3`   |
//! | `$$$`   | `001` | `002` | `003` |
//! | `$@5`   | `5`   | `6`   | `7`   |
//! | `$@-`   | `3`   | `2`   | `1`   |
//! | `$@-5`  | `7`   | `6`   | `5`   |
//! | `\$`    | `$`   | `$`   | `$`   |
//!
//! The escape is not decoration. `$` is a legal character in an Oracle identifier
//! and a perfectly ordinary character in a string, so a template must be able to
//! contain one that is not a counter — otherwise the feature quietly corrupts the
//! values of anyone whose data contains a dollar sign.

/// Substitute every `$` run in `text` with the number of row `index` (0-based) out
/// of `total`.
pub fn number(text: &str, index: usize, total: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' && chars.get(i + 1) == Some(&'$') {
            out.push('$');
            i += 2;
            continue;
        }
        if chars[i] != '$' {
            out.push(chars[i]);
            i += 1;
            continue;
        }

        // The run of `$` is the zero-padding width.
        let start = i;
        while i < chars.len() && chars[i] == '$' {
            i += 1;
        }
        let width = i - start;

        let (descending, base, consumed) = modifier(&chars[i..]);
        i += consumed;

        let offset = if descending { total.saturating_sub(index + 1) } else { index };
        let value = base.saturating_add(offset as i64);
        let digits = value.abs().to_string();
        if value < 0 {
            out.push('-');
        }
        for _ in digits.len()..width {
            out.push('0');
        }
        out.push_str(&digits);
    }
    out
}

/// `@`, then an optional `-`, then optional digits. Returns whether it counts
/// down, where it starts, and how many characters were used.
///
/// A bare `$` — no `@` at all — is `(false, 1, 0)`: rows are numbered from one,
/// because that is how people count rows.
fn modifier(rest: &[char]) -> (bool, i64, usize) {
    if rest.first() != Some(&'@') {
        return (false, 1, 0);
    }
    let mut used = 1;
    let descending = rest.get(used) == Some(&'-');
    if descending {
        used += 1;
    }
    let digits: String = rest[used..].iter().take_while(|c| c.is_ascii_digit()).collect();
    used += digits.len();
    // `$@-` on its own counts down to 1; `$@-5` counts down to 5.
    let base = digits.parse::<i64>().unwrap_or(1);
    (descending, base, used)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole table in the module doc, asserted.
    fn column(pattern: &str, total: usize) -> Vec<String> {
        (0..total).map(|i| number(pattern, i, total)).collect()
    }

    #[test]
    fn a_bare_dollar_counts_from_one() {
        assert_eq!(column("$", 3), ["1", "2", "3"]);
        assert_eq!(column("RIGA_$", 2), ["RIGA_1", "RIGA_2"]);
    }

    #[test]
    fn repeated_dollars_are_the_padding_width() {
        assert_eq!(column("$$$", 3), ["001", "002", "003"]);
        // Padding is a minimum, never a truncation: row 100 is not `00`.
        assert_eq!(number("$$", 99, 100), "100");
    }

    #[test]
    fn at_sets_where_the_count_starts() {
        assert_eq!(column("$@5", 3), ["5", "6", "7"]);
        assert_eq!(column("$$@10", 2), ["10", "11"]);
    }

    #[test]
    fn a_minus_counts_down() {
        assert_eq!(column("$@-", 3), ["3", "2", "1"]);
        assert_eq!(column("$@-5", 3), ["7", "6", "5"]);
    }

    #[test]
    fn an_escaped_dollar_is_a_dollar() {
        // The case that makes this worth having: Oracle-era names contain `$`,
        // and so does perfectly ordinary text.
        assert_eq!(column("\\$", 2), ["$", "$"]);
        assert_eq!(number("COSTO \\$ $", 0, 1), "COSTO $ 1");
        assert_eq!(number("SYS\\$LOG", 0, 1), "SYS$LOG");
    }

    #[test]
    fn several_counters_in_one_value_all_move_together() {
        assert_eq!(number("$-$$$", 4, 9), "5-005");
        assert_eq!(number("$-$$", 4, 9), "5-05");
    }

    #[test]
    fn text_with_no_counter_comes_back_unchanged() {
        assert_eq!(number("plain", 2, 5), "plain");
        assert_eq!(number("", 0, 1), "");
    }
}
