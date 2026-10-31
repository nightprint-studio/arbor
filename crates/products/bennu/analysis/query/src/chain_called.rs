//! The calls a chain has already made — so a builder does not offer `name(…)` again right after `.name("x")`.
//!
//! A fluent setter returns its own receiver, and in `Device.builder().name("a").type(T).|` three quarters of the
//! list are setters that were just called. Nothing forbids calling one twice; it is just almost never what comes
//! next. The caller moves those rows to the end of the list instead of taking them away.

use std::collections::HashSet;

fn ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Names of the methods called in the chain that ends at the dot at `dot` (`recv.a().b(x).<dot>`), across lines.
pub(crate) fn called_in_chain(source: &str, dot: usize) -> HashSet<String> {
    let b = source.as_bytes();
    let mut called = HashSet::new();
    let mut i = dot.min(b.len());
    let left_of = |mut k: usize| {
        while k > 0 && b[k - 1].is_ascii_whitespace() {
            k -= 1;
        }
        k
    };
    loop {
        i = left_of(i);
        if i == 0 {
            break;
        }
        match b[i - 1] {
            b')' | b']' => {
                // jump back over the balanced brackets
                let mut depth = 0i32;
                while i > 0 {
                    depth += matches!(b[i - 1], b')' | b']') as i32 - matches!(b[i - 1], b'(' | b'[') as i32;
                    i -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                if depth != 0 {
                    break;
                }
                // a call: the identifier before the `(` names it
                let end = left_of(i);
                let mut start = end;
                while start > 0 && ident(b[start - 1]) {
                    start -= 1;
                }
                if start == end {
                    break;
                }
                if left_of(start) > 0 && b[left_of(start) - 1] == b'.' {
                    called.insert(source[start..end].to_string());
                }
                i = start;
            }
            c if ident(c) => {
                while i > 0 && ident(b[i - 1]) {
                    i -= 1;
                }
            }
            b'.' => {
                i -= 1;
                continue;
            }
            _ => break,
        }
        // the chain goes on only through a dot
        let before = left_of(i);
        if before == 0 || b[before - 1] != b'.' {
            break;
        }
    }
    called
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at_dot(src: &str) -> usize {
        src.rfind('.').unwrap()
    }

    #[test]
    fn the_calls_of_a_chain_are_collected() {
        let src = "return Device.builder().name(\"a\").type(T.X).";
        let got = called_in_chain(src, at_dot(src));
        assert_eq!(got, ["builder", "name", "type"].iter().map(|s| s.to_string()).collect::<HashSet<_>>());
    }

    #[test]
    fn a_chain_across_lines_counts_the_same() {
        let src = "Device d = Device.builder()\n    .name(\"a\")\n    .id(compute(1, 2))\n    .";
        let got = called_in_chain(src, at_dot(src));
        assert!(got.contains("name") && got.contains("id") && got.contains("builder"));
        assert!(!got.contains("compute"), "{got:?}");
    }

    #[test]
    fn the_chain_stops_at_the_statement() {
        let src = "other.go();\n        x.keep().";
        let got = called_in_chain(src, at_dot(src));
        assert_eq!(got, ["keep"].iter().map(|s| s.to_string()).collect::<HashSet<_>>());
    }

    #[test]
    fn a_plain_receiver_has_called_nothing() {
        let src = "order.";
        assert!(called_in_chain(src, at_dot(src)).is_empty());
    }
}
