//! `bennu.requires` — what a project needs for a template to be offered at all.
//!
//! ```jinja
//! {# bennu.requires: java >= 16 #}
//! {# bennu.requires: lombok, spring-boot >= 3 #}
//! {# bennu.requires: !lombok #}
//! ```
//!
//! Each item is `java` or a dependency — by `group:artifact` or by artifact alone — optionally with a
//! bound (`>=`, `>`, `<=`, `<`, `=`, `!=`) on its version; `!` in front means *only without*. A template
//! whose requirements the project does not meet is left out wherever templates are offered, and its
//! settings row says why. What is not known yet — a Java level not read, a classpath not resolved — holds
//! nothing back: a template is hidden for what is known to be missing, never for what is not known.

use std::cmp::Ordering;

use crate::facts::ProjectFacts;

/// The first requirement `project` does not meet, in the words a badge uses — `None` when it meets them all.
pub fn unmet(requires: &[String], project: &ProjectFacts) -> Option<String> {
    requires.iter().map(|item| item.trim()).filter(|item| !item.is_empty()).find_map(|item| check(item, project).err())
}

fn check(item: &str, project: &ProjectFacts) -> Result<(), String> {
    let (negated, item) = match item.strip_prefix('!') {
        Some(rest) => (true, rest.trim()),
        None => (false, item),
    };
    let (name, bound) = split_bound(item);
    let is_java = name.eq_ignore_ascii_case("java");
    let known = if is_java { project.java > 0 } else { project.resolved };
    if !known {
        return Ok(());
    }
    let found = match is_java {
        true => Some(project.java.to_string()),
        false => project.version_of(name),
    };
    let met = match (&found, bound) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(version), Some((op, wanted))) => satisfies(version, op, wanted),
    };
    let label = if is_java { "Java" } else { name };
    match (negated, met) {
        (false, true) | (true, false) => Ok(()),
        (false, false) => Err(match bound {
            Some((op, wanted)) => format!("Needs {label} {}", bound_words(op, wanted)),
            None => format!("Needs {label}"),
        }),
        (true, true) => Err(format!("Only without {label}")),
    }
}

/// `spring-boot >= 3` → `spring-boot` and `(">=", "3")`.
fn split_bound(item: &str) -> (&str, Option<(&str, &str)>) {
    let Some(at) = item.find(['<', '>', '=', '!']) else { return (item.trim(), None) };
    let rest = &item[at..];
    let op = ["<=", ">=", "!=", "==", "<", ">", "="].into_iter().find(|op| rest.starts_with(op)).unwrap_or("=");
    (item[..at].trim(), Some((op, rest[op.len()..].trim())))
}

fn satisfies(version: &str, op: &str, wanted: &str) -> bool {
    let ordering = compare(version, wanted);
    match op {
        ">=" => ordering != Ordering::Less,
        ">" => ordering == Ordering::Greater,
        "<=" => ordering != Ordering::Greater,
        "<" => ordering == Ordering::Less,
        "!=" => ordering != Ordering::Equal,
        _ => ordering == Ordering::Equal,
    }
}

/// Versions compared number by number, a missing number counting as zero: `3` is `3.0.0`, `1.18.30` is
/// later than `1.18.4`.
fn compare(a: &str, b: &str) -> Ordering {
    let numbers = |v: &str| -> Vec<u64> { v.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse().ok()).collect() };
    let (a, b) = (numbers(a), numbers(b));
    (0..a.len().max(b.len()))
        .map(|i| a.get(i).unwrap_or(&0).cmp(b.get(i).unwrap_or(&0)))
        .find(|o| *o != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}

fn bound_words(op: &str, wanted: &str) -> String {
    match op {
        ">=" => format!("{wanted} or later"),
        ">" => format!("later than {wanted}"),
        "<=" => format!("{wanted} or earlier"),
        "<" => format!("before {wanted}"),
        "!=" => format!("other than {wanted}"),
        _ => wanted.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(java: u32, deps: &[(&str, &str, &str)]) -> ProjectFacts {
        ProjectFacts::new(java, deps.iter().map(|(g, a, v)| (g.to_string(), a.to_string(), v.to_string())), true)
    }

    fn items(text: &[&str]) -> Vec<String> {
        text.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn java_and_a_dependency_are_checked_with_their_bounds() {
        let p = project(17, &[("org.springframework.boot", "spring-boot", "3.2.1"), ("org.projectlombok", "lombok", "1.18.30")]);
        assert_eq!(unmet(&items(&["java >= 16", "lombok", "spring-boot >= 3"]), &p), None);
        assert_eq!(unmet(&items(&["java >= 21"]), &p).as_deref(), Some("Needs Java 21 or later"));
        assert_eq!(unmet(&items(&["org.mapstruct:mapstruct"]), &p).as_deref(), Some("Needs org.mapstruct:mapstruct"));
        assert_eq!(unmet(&items(&["lombok >= 1.18.4"]), &p), None, "30 is later than 4");
        assert_eq!(unmet(&items(&["!lombok"]), &p).as_deref(), Some("Only without lombok"));
        assert_eq!(unmet(&items(&["!mapstruct"]), &p), None);
    }

    /// Hidden for what is known to be missing, never for what is not known.
    #[test]
    fn what_is_not_known_holds_nothing_back() {
        let unresolved = ProjectFacts { java: 0, resolved: false, ..ProjectFacts::default() };
        assert_eq!(unmet(&items(&["java >= 16", "lombok", "!lombok"]), &unresolved), None);
    }
}
