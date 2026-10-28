//! Finding the layout a project's logs are written in.
//!
//! A project that configures Logback or Log4j has *written down* what its console lines look like
//! — `%d{…} | %-5level | %logger{1} | …`. Reading that turns the run console's guesses into
//! facts (see `arbor_logscan::layout`): which column is the class, which values are MDC.
//!
//! Where it looks, in this order, in the project root and in the `src/main/resources` of the root
//! and of each module one level down:
//!
//! 1. `logback-spring.xml`, `logback.xml` — the pattern of the console appender (the first
//!    appender when none is a console one), `<property>` substitutions applied;
//! 2. `log4j2-spring.xml`, `log4j2.xml` — the `PatternLayout` of the `Console` appender;
//! 3. `application.properties`, `application.yml`, `application.yaml` — `logging.pattern.console`.
//!
//! A pattern that cannot be fully resolved (an unknown `${property}`) or that the layout parser
//! does not model is simply not used: the console keeps the shape rules, which is what it had.
//! Reading is a handful of small files at the start of a run, never on the output path.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use arbor_logscan::prelude::PatternLayout;

/// The most a config file is read for. A real one is a few kilobytes.
const MAX_CONFIG_BYTES: u64 = 512 * 1024;

/// The layout the project at `root` logs in, if its configuration says one this understands.
pub(crate) fn detect(root: &Path) -> Option<PatternLayout> {
    for dir in resource_dirs(root) {
        for (name, read) in SOURCES {
            let Some(text) = read_small(&dir.join(name)) else { continue };
            for pattern in read(&text) {
                if let Some(layout) = PatternLayout::parse(&pattern) {
                    return Some(layout);
                }
            }
        }
    }
    None
}

type Reader = fn(&str) -> Vec<String>;

/// Files in the order they are consulted, each with what extracts its candidate patterns.
const SOURCES: &[(&str, Reader)] = &[
    ("logback-spring.xml", patterns_from_logback),
    ("logback.xml", patterns_from_logback),
    ("log4j2-spring.xml", patterns_from_log4j2),
    ("log4j2.xml", patterns_from_log4j2),
    ("application.properties", patterns_from_properties),
    ("application.yml", patterns_from_yaml),
    ("application.yaml", patterns_from_yaml),
];

/// `root` and `root/*` (modules), each with its `src/main/resources`.
fn resource_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![root.join("src/main/resources"), root.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut modules: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir() && !is_skipped(p))
            .collect();
        modules.sort();
        for module in modules {
            dirs.push(module.join("src/main/resources"));
        }
    }
    dirs
}

fn is_skipped(dir: &Path) -> bool {
    dir.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.') || matches!(n, "target" | "node_modules" | "build" | "out" | "src"))
}

fn read_small(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_CONFIG_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

// ── Logback ─────────────────────────────────────────────────────────────────────

/// Every `<pattern>` of the appenders, console appenders first, with `<property>` values
/// substituted. A pattern with a `${…}` that has no value is dropped.
pub(crate) fn patterns_from_logback(xml: &str) -> Vec<String> {
    let props = logback_properties(xml);
    let mut console: Vec<String> = Vec::new();
    let mut other: Vec<String> = Vec::new();
    for block in blocks(xml, "<appender", "</appender>") {
        let is_console = block.to_ascii_lowercase().contains("consoleappender");
        for raw in elements(block, "pattern") {
            if let Some(pattern) = substitute(&unescape(raw.trim()), &props) {
                if is_console { console.push(pattern) } else { other.push(pattern) }
            }
        }
    }
    console.extend(other);
    console
}

/// `<property name="LOG_PATTERN" value="…"/>` → name → value.
fn logback_properties(xml: &str) -> HashMap<String, String> {
    let mut props = HashMap::new();
    for tag in blocks(xml, "<property", ">") {
        if let (Some(name), Some(value)) = (attribute(tag, "name"), attribute(tag, "value")) {
            props.insert(name, unescape(&value));
        }
    }
    props
}

/// Replace `${NAME}` and `${NAME:-default}`; `None` when one has neither a value nor a default.
fn substitute(pattern: &str, props: &HashMap<String, String>) -> Option<String> {
    let mut out = String::with_capacity(pattern.len());
    let mut rest = pattern;
    while let Some(open) = rest.find("${") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        let close = after.find('}')?;
        let inner = &after[..close];
        let (key, default) = match inner.split_once(":-") {
            Some((k, d)) => (k, Some(d)),
            None => (inner, None),
        };
        let value = props.get(key).map(String::as_str).or(default)?;
        out.push_str(value);
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    Some(out)
}

// ── Log4j2 ──────────────────────────────────────────────────────────────────────

/// The `pattern` attribute of every `PatternLayout`, those inside a `Console` first.
pub(crate) fn patterns_from_log4j2(xml: &str) -> Vec<String> {
    let mut console: Vec<String> = Vec::new();
    let mut other: Vec<String> = Vec::new();
    let lower = xml.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find("<patternlayout") {
        let start = from + at;
        let end = xml[start..].find('>').map_or(xml.len(), |i| start + i);
        from = end;
        let Some(pattern) = attribute(&xml[start..end], "pattern") else { continue };
        // The nearest opening appender tag above says what kind of appender this belongs to.
        let owner = lower[..start].rfind("<console").zip(lower[..start].rfind("</")).map(|(c, close)| c > close);
        let is_console = owner.unwrap_or_else(|| lower[..start].rfind("<console").is_some());
        let pattern = unescape(&pattern);
        if is_console { console.push(pattern) } else { other.push(pattern) }
    }
    console.extend(other);
    console
}

// ── Spring Boot properties ──────────────────────────────────────────────────────

pub(crate) fn patterns_from_properties(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (key, value) = line.split_once('=').or_else(|| line.split_once(':'))?;
            (key.trim() == "logging.pattern.console").then(|| value.trim().to_string())
        })
        .collect()
}

/// `logging.pattern.console` as a flat key or nested under `logging:` → `pattern:`.
pub(crate) fn patterns_from_yaml(text: &str) -> Vec<String> {
    let mut path: Vec<(usize, String)> = Vec::new();
    let mut found = Vec::new();
    for line in text.lines() {
        let content = line.trim_start();
        if content.is_empty() || content.starts_with('#') {
            continue;
        }
        let indent = line.len() - content.len();
        let Some((key, value)) = content.split_once(':') else { continue };
        while path.last().is_some_and(|(i, _)| *i >= indent) {
            path.pop();
        }
        let full = path.iter().map(|(_, k)| k.as_str()).chain(std::iter::once(key.trim())).collect::<Vec<_>>().join(".");
        let value = value.trim();
        if value.is_empty() {
            path.push((indent, key.trim().to_string()));
        } else if full == "logging.pattern.console" {
            found.push(unquote(value));
        }
    }
    found
}

fn unquote(value: &str) -> String {
    let v = value.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = v.strip_prefix(quote).and_then(|s| s.strip_suffix(quote)) {
            return inner.to_string();
        }
    }
    v.to_string()
}

// ── tiny XML helpers ────────────────────────────────────────────────────────────

/// The text of each `<tag>…</tag>` in `hay`, tag matched without regard to case.
fn elements<'a>(hay: &'a str, tag: &str) -> Vec<&'a str> {
    let lower = hay.to_ascii_lowercase();
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = lower[from..].find(&open) {
        let body = from + at + open.len();
        let Some(len) = lower[body..].find(&close) else { break };
        out.push(&hay[body..body + len]);
        from = body + len + close.len();
    }
    out
}

/// Each stretch of `hay` from `open` to `close` (the close marker included), case-insensitively.
fn blocks<'a>(hay: &'a str, open: &str, close: &str) -> Vec<&'a str> {
    let lower = hay.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = lower[from..].find(open) {
        let start = from + at;
        let end = lower[start..].find(close).map_or(hay.len(), |i| start + i + close.len());
        out.push(&hay[start..end]);
        from = end;
    }
    out
}

/// The value of `name="…"` (or `'…'`) in a tag's text.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find(name) {
        let start = from + at;
        from = start + name.len();
        let before_ok = start == 0 || !lower.as_bytes()[start - 1].is_ascii_alphanumeric();
        let rest = tag[start + name.len()..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        let rest = rest.trim_start();
        let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        if !before_ok {
            continue;
        }
        let inner = &rest[1..];
        return inner.find(quote).map(|end| inner[..end].to_string());
    }
    None
}

fn unescape(text: &str) -> String {
    // `<pattern><![CDATA[…]]></pattern>` is how a pattern with `<` or `&` in it is usually written.
    let text = text.trim();
    let text = text
        .strip_prefix("<![CDATA[")
        .and_then(|t| t.strip_suffix("]]>"))
        .unwrap_or(text);
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = "%d{yyyy-MM-dd HH:mm:ss} | %-5level | %logger{1} | MDC:[tenant=%X{tenant.id}] | %msg%n";

    #[test]
    fn a_logback_console_appender_pattern_is_found() {
        let xml = format!(
            "<configuration>\n<appender name=\"FILE\" class=\"ch.qos.logback.core.FileAppender\"><encoder><pattern>%d %msg</pattern></encoder></appender>\n\
             <appender name=\"STDOUT\" class=\"ch.qos.logback.core.ConsoleAppender\"><encoder>\n  <pattern>{EXAMPLE}</pattern>\n</encoder></appender></configuration>"
        );
        let got = patterns_from_logback(&xml);
        assert_eq!(got[0], EXAMPLE, "the console appender comes first");
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn xml_entities_in_a_pattern_are_unescaped() {
        let xml = "<appender class=\"x.ConsoleAppender\"><pattern>%d &lt;%level&gt; &amp; %msg</pattern></appender>";
        assert_eq!(patterns_from_logback(xml), ["%d <%level> & %msg"]);
    }

    #[test]
    fn a_cdata_pattern_is_unwrapped() {
        let xml = "<appender class=\"x.ConsoleAppender\"><pattern><![CDATA[%d | %level | %msg]]></pattern></appender>";
        assert_eq!(patterns_from_logback(xml), ["%d | %level | %msg"]);
    }

    #[test]
    fn a_property_is_substituted_and_a_default_is_used() {
        let xml = "<configuration><property name=\"LOG_PATTERN\" value=\"%d{HH:mm:ss} %level %msg\"/>\
                   <appender class=\"x.ConsoleAppender\"><encoder><pattern>${LOG_PATTERN}</pattern></encoder></appender>\
                   <appender class=\"x.ConsoleAppender\"><encoder><pattern>${MISSING:-%level %msg}</pattern></encoder></appender></configuration>";
        assert_eq!(patterns_from_logback(xml), ["%d{HH:mm:ss} %level %msg", "%level %msg"]);
    }

    /// Spring Boot's own `${CONSOLE_LOG_PATTERN}` is defined by a file we do not read; guessing it
    /// would colour lines in a layout the project does not use.
    #[test]
    fn an_unresolvable_property_drops_the_pattern() {
        let xml = "<appender class=\"x.ConsoleAppender\"><pattern>${CONSOLE_LOG_PATTERN}</pattern></appender>";
        assert!(patterns_from_logback(xml).is_empty());
    }

    #[test]
    fn a_log4j2_console_pattern_comes_first() {
        let xml = format!(
            "<Configuration><Appenders><File name=\"F\"><PatternLayout pattern=\"%d %m\"/></File>\
             <Console name=\"C\"><PatternLayout pattern=\"{EXAMPLE}\"/></Console></Appenders></Configuration>"
        );
        let got = patterns_from_log4j2(&xml);
        assert_eq!(got[0], EXAMPLE);
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn spring_properties_name_the_console_pattern() {
        let text = "server.port=8080\n# a comment\nlogging.pattern.console = %d{HH:mm:ss} %level %msg%n\nlogging.pattern.file=%d %msg\n";
        assert_eq!(patterns_from_properties(text), ["%d{HH:mm:ss} %level %msg%n"]);
    }

    #[test]
    fn yaml_finds_the_flat_and_the_nested_form() {
        let flat = "logging.pattern.console: \"%d{HH:mm:ss} %level %msg%n\"\n";
        assert_eq!(patterns_from_yaml(flat), ["%d{HH:mm:ss} %level %msg%n"]);
        let nested = "server:\n  port: 8080\nlogging:\n  level:\n    root: INFO\n  pattern:\n    file: '%d %m'\n    console: '%d{HH:mm:ss} %level %msg%n'\n";
        assert_eq!(patterns_from_yaml(nested), ["%d{HH:mm:ss} %level %msg%n"]);
    }

    #[test]
    fn an_example_pattern_from_each_source_parses_as_a_layout() {
        assert!(PatternLayout::parse(EXAMPLE).is_some());
    }

    #[test]
    fn detect_reads_the_resources_of_the_root_and_of_a_module() {
        let dir = std::env::temp_dir().join(format!("bennu-log-pattern-{}", std::process::id()));
        let module = dir.join("web/src/main/resources");
        std::fs::create_dir_all(&module).unwrap();
        std::fs::write(
            module.join("logback.xml"),
            format!("<configuration><appender class=\"x.ConsoleAppender\"><pattern>{EXAMPLE}</pattern></appender></configuration>"),
        )
        .unwrap();
        assert!(detect(&dir).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_project_without_configuration_has_no_layout() {
        let dir = std::env::temp_dir().join(format!("bennu-log-pattern-none-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(detect(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn attributes_are_read_in_either_quote_and_not_from_a_longer_name() {
        assert_eq!(attribute("<p name='a' value=\"b c\"/>", "value").as_deref(), Some("b c"));
        assert_eq!(attribute("<p classname=\"x\" name=\"y\"/>", "name").as_deref(), Some("y"));
        assert_eq!(attribute("<p other=\"x\"/>", "name"), None);
    }
}
