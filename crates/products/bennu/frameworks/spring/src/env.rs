//! Turning a configuration key into the environment variable that overrides it.
//!
//! ## Why it needs an implementation rather than a rule of thumb
//!
//! "Uppercase it and swap the dots" is nearly right, and nearly right is how
//! `SPRING_JPA_SHOW_SQL` gets into a deployment descriptor and silently does nothing. The
//! actual rule Spring Boot documents for binding from the environment is three steps, and
//! the middle one is the one everybody forgets:
//!
//! 1. `.` becomes `_`
//! 2. `-` is **removed** — not replaced
//! 3. the result is uppercased
//!
//! So `spring.jpa.show-sql` is `SPRING_JPA_SHOWSQL`. Indexed keys take a fourth rule:
//! `my.service[0].other` becomes `MY_SERVICE_0_OTHER`, brackets turning into separators.
//!
//! The key is canonicalised first ([`canonical_key`]), so it does not matter whether the
//! yaml spells it `showSql`, `show-sql` or `show_sql` — all three produce the same variable,
//! which is exactly the property that makes this worth computing instead of typing.
//!
//! ## What it produces
//!
//! Not just the name: the *lines you paste somewhere*. A name on its own still leaves the
//! quoting to be got right by hand, and the four places this ends up (a `.env`, a shell, a
//! `docker run`, a compose file) quote differently.

use crate::usages::canonical_key;

/// A key rendered as an environment override, in each of the forms it gets pasted into.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvVar {
    /// The property key as written in the file.
    pub key: String,
    /// The value as written, possibly empty.
    pub value: String,
    /// The environment variable name (`SPRING_JPA_SHOWSQL`).
    pub name: String,
    /// `(label, text)` pairs — one per place this gets pasted.
    pub forms: Vec<(String, String)>,
}

/// The environment variable name Spring Boot binds `key` from.
pub fn env_var_name(key: &str) -> String {
    let canonical = canonical_key(key);
    let mut out = String::with_capacity(canonical.len());
    for c in canonical.chars() {
        match c {
            // Dashes vanish: `show-sql` is `SHOWSQL`, not `SHOW_SQL`. The single most
            // common way a hand-written override misses.
            '-' => {}
            '.' | '[' | ']' => {
                // Never two separators in a row, and never one leading.
                if !out.is_empty() && !out.ends_with('_') {
                    out.push('_');
                }
            }
            _ => out.extend(c.to_uppercase()),
        }
    }
    out.trim_end_matches('_').to_string()
}

/// The full view for the UI: the name plus every paste-ready form of it.
pub fn env_var(key: &str, value: &str) -> EnvVar {
    let name = env_var_name(key);
    let quoted = shell_quote(value);
    EnvVar {
        forms: vec![
            ("Name".to_string(), name.clone()),
            (".env".to_string(), format!("{name}={value}")),
            ("Shell".to_string(), format!("export {name}={quoted}")),
            ("docker run".to_string(), format!("-e {name}={quoted}")),
            ("compose".to_string(), format!("{name}: \"{}\"", value.replace('"', "\\\""))),
        ],
        key: key.to_string(),
        value: value.to_string(),
        name,
    }
}

/// A value safe to paste into a shell. Single quotes unless it is plainly a bare word —
/// erring toward quoting, because an unquoted `${…}` or a space is a bug at paste time.
fn shell_quote(value: &str) -> String {
    let bare = !value.is_empty()
        && value.chars().all(|c| c.is_ascii_alphanumeric() || "._-/:@".contains(c));
    if bare {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashes_are_removed_and_dots_become_underscores() {
        assert_eq!(env_var_name("spring.jpa.show-sql"), "SPRING_JPA_SHOWSQL");
        assert_eq!(env_var_name("server.port"), "SERVER_PORT");
        assert_eq!(env_var_name("server.servlet.context-path"), "SERVER_SERVLET_CONTEXTPATH");
    }

    /// The whole reason to compute this rather than type it: three spellings of one key must
    /// produce one variable, and they do only because the key is canonicalised first.
    #[test]
    fn every_relaxed_spelling_yields_the_same_variable() {
        let expected = "SPRING_JPA_SHOWSQL";
        assert_eq!(env_var_name("spring.jpa.showSql"), expected);
        assert_eq!(env_var_name("spring.jpa.show_sql"), expected);
        assert_eq!(env_var_name("spring.jpa.show-sql"), expected);
    }

    #[test]
    fn indexed_keys_turn_their_brackets_into_separators() {
        assert_eq!(env_var_name("my.service[0].other"), "MY_SERVICE_0_OTHER");
        assert_eq!(env_var_name("a.b[10]"), "A_B_10", "no trailing separator");
    }

    #[test]
    fn a_value_that_needs_quoting_gets_it_in_the_forms_that_need_it() {
        let v = env_var("app.greeting", "hello world");
        assert_eq!(v.name, "APP_GREETING");
        let form = |label: &str| {
            v.forms.iter().find(|(l, _)| l == label).map(|(_, t)| t.clone()).unwrap()
        };
        // A `.env` file is not a shell: quotes there would become part of the value.
        assert_eq!(form(".env"), "APP_GREETING=hello world");
        assert_eq!(form("Shell"), "export APP_GREETING='hello world'");
        assert_eq!(form("docker run"), "-e APP_GREETING='hello world'");
        assert_eq!(form("compose"), "APP_GREETING: \"hello world\"");
    }

    #[test]
    fn a_bare_word_is_left_unquoted_and_a_placeholder_is_not() {
        assert_eq!(shell_quote("jdbc:postgresql://db/app"), "jdbc:postgresql://db/app");
        assert_eq!(shell_quote("${DB_URL}"), "'${DB_URL}'", "would expand in the shell");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn an_empty_key_does_not_produce_a_stray_underscore() {
        assert_eq!(env_var_name(""), "");
        assert_eq!(env_var_name("."), "");
    }
}
