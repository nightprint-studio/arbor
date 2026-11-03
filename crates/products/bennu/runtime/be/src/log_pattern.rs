//! Finding the layout a project's logs are written in.
//!
//! A project that configures Logback or Log4j has *written down* what its console lines look like
//! — `%d{…} | %-5level | %logger{1} | …`. Reading that turns the run console's guesses into
//! facts (see `arbor_logscan::layout`): which column is the class, which values are MDC.
//!
//! Where it looks, in the project root and in the resources of the root and of each module one level
//! down (`src/main/resources`, `WEB-INF/classes`, `WEB-INF`), in this order:
//!
//! 1. `logback-spring.xml`, `logback.xml` — the pattern of the console appender (the first
//!    appender when none is a console one), `<property>` substitutions applied;
//! 2. `log4j2-spring.xml`, `log4j2.xml`, `log4j2.properties` — the `PatternLayout` of the `Console`
//!    appender, `<Property>` substitutions applied;
//! 3. `log4j.xml`, `log4j.properties` — Log4j 1.x's `ConversionPattern`;
//! 4. `application.properties`, `application.yml`, `application.yaml` — `logging.pattern.console`.
//!
//! A project with several of those (a module each) gets a list of them; the first that yields a
//! layout is the one used. A pattern that cannot be fully resolved (an unknown `${property}`) or
//! that the layout parser does not model is simply not used: the console keeps the shape rules,
//! which is what it had. Reading is a handful of small files at the start of a run, never on the
//! output path.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use arbor_logscan::prelude::PatternLayout;
use serde::Serialize;

/// The most a config file is read for. A real one is a few kilobytes.
const MAX_CONFIG_BYTES: u64 = 512 * 1024;

/// One logging configuration that names a console pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Candidate {
    /// Relative to the project root, with `/`.
    pub file: String,
    /// `logback` | `log4j2` | `log4j` | `spring`.
    pub kind: &'static str,
    pub pattern: String,
    /// The layout parser models it, so the console can use it.
    pub usable: bool,
}

/// The layout the project at `root` logs in, if its configuration says one this understands.
pub(crate) fn detect(root: &Path) -> Option<PatternLayout> {
    candidates(root).iter().find(|c| c.usable).and_then(|c| PatternLayout::parse(&c.pattern))
}

/// Every logging configuration of the project that names a pattern, in the order they are consulted.
pub(crate) fn candidates(root: &Path) -> Vec<Candidate> {
    let mut found = Vec::new();
    for dir in resource_dirs(root) {
        for (name, kind, read) in SOURCES {
            let path = dir.join(name);
            let Some(text) = read_small(&path) else { continue };
            let patterns = read(&text);
            let Some(first) = patterns.first() else { continue };
            let usable = patterns.iter().find(|p| PatternLayout::parse(p).is_some());
            found.push(Candidate {
                file: relative(root, &path),
                kind,
                pattern: usable.unwrap_or(first).clone(),
                usable: usable.is_some(),
            });
        }
    }
    found
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

type Reader = fn(&str) -> Vec<String>;

/// Files in the order they are consulted, each with its kind and what extracts its candidate patterns.
const SOURCES: &[(&str, &str, Reader)] = &[
    ("logback-spring.xml", "logback", patterns_from_logback),
    ("logback.xml", "logback", patterns_from_logback),
    ("log4j2-spring.xml", "log4j2", patterns_from_log4j2),
    ("log4j2.xml", "log4j2", patterns_from_log4j2),
    ("log4j2.properties", "log4j2", patterns_from_log4j2_properties),
    ("log4j.xml", "log4j", patterns_from_log4j1_xml),
    ("log4j.properties", "log4j", patterns_from_log4j1_properties),
    ("application.properties", "spring", patterns_from_properties),
    ("application.yml", "spring", patterns_from_yaml),
    ("application.yaml", "spring", patterns_from_yaml),
];

/// `root` and `root/*` (modules), each with the places a web module keeps its resources.
fn resource_dirs(root: &Path) -> Vec<PathBuf> {
    let places = |base: &Path| {
        [
            base.join("src/main/resources"),
            base.join("src/main/webapp/WEB-INF/classes"),
            base.join("src/main/webapp/WEB-INF"),
        ]
    };
    let mut dirs: Vec<PathBuf> = places(root).into();
    dirs.push(root.to_path_buf());
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut modules: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir() && !is_skipped(p))
            .collect();
        modules.sort();
        for module in modules {
            dirs.extend(places(&module));
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

/// The `pattern` attribute of every `PatternLayout`, those inside a `Console` first, with the
/// `<Property name="…">value</Property>` of the file substituted. One that names a property it
/// does not define (`${sys:…}`) is dropped.
pub(crate) fn patterns_from_log4j2(xml: &str) -> Vec<String> {
    let props = log4j2_properties(xml);
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
        let Some(pattern) = substitute(&unescape(&pattern), &props) else { continue };
        if is_console { console.push(pattern) } else { other.push(pattern) }
    }
    console.extend(other);
    console
}

/// `<Property name="LOG_PATTERN">%d … %msg%n</Property>` → name → text.
fn log4j2_properties(xml: &str) -> HashMap<String, String> {
    let mut props = HashMap::new();
    for block in blocks(xml, "<property ", "</property>") {
        let Some(name) = attribute(block, "name") else { continue };
        let Some(body) = block.find('>').map(|i| &block[i + 1..]) else { continue };
        let value = body.strip_suffix("</Property>").or_else(|| body.strip_suffix("</property>")).unwrap_or(body);
        props.insert(name, unescape(value));
    }
    props
}

/// `appender.console.layout.pattern = …` of a `.properties` file, the `Console` appenders first,
/// with `property.NAME = …` substituted.
pub(crate) fn patterns_from_log4j2_properties(text: &str) -> Vec<String> {
    let entries = key_values(text);
    let props: HashMap<String, String> =
        entries.iter().filter_map(|(k, v)| k.strip_prefix("property.").map(|n| (n.to_string(), v.clone()))).collect();
    let is_console = |name: &str| {
        entries.iter().any(|(k, v)| *k == format!("appender.{name}.type") && v.eq_ignore_ascii_case("console"))
    };
    appender_patterns(&entries, "appender.", ".layout.pattern", is_console, |p| substitute(p, &props))
}

// ── Log4j 1.x ───────────────────────────────────────────────────────────────────

/// `<param name="ConversionPattern" value="…"/>` of each appender, the `ConsoleAppender` first.
pub(crate) fn patterns_from_log4j1_xml(xml: &str) -> Vec<String> {
    let mut console: Vec<String> = Vec::new();
    let mut other: Vec<String> = Vec::new();
    for block in blocks(xml, "<appender", "</appender>") {
        let is_console = block.to_ascii_lowercase().contains("consoleappender");
        for tag in blocks(block, "<param", ">") {
            if attribute(tag, "name").is_some_and(|n| n.eq_ignore_ascii_case("conversionpattern")) {
                if let Some(value) = attribute(tag, "value") {
                    if is_console { console.push(unescape(&value)) } else { other.push(unescape(&value)) }
                }
            }
        }
    }
    console.extend(other);
    console
}

/// `log4j.appender.STDOUT.layout.ConversionPattern=…`, the appenders that are a `ConsoleAppender` first.
pub(crate) fn patterns_from_log4j1_properties(text: &str) -> Vec<String> {
    let entries = key_values(text);
    let is_console = |name: &str| {
        entries
            .iter()
            .any(|(k, v)| *k == format!("log4j.appender.{name}") && v.to_ascii_lowercase().contains("consoleappender"))
    };
    appender_patterns(&entries, "log4j.appender.", ".layout.ConversionPattern", is_console, |p| Some(p.to_string()))
}

/// The patterns of every `<prefix><name><suffix>` entry, those whose appender `is_console` first, each
/// run through `resolve` (which may drop it).
fn appender_patterns(
    entries: &[(String, String)],
    prefix: &str,
    suffix: &str,
    is_console: impl Fn(&str) -> bool,
    resolve: impl Fn(&str) -> Option<String>,
) -> Vec<String> {
    let mut console: Vec<String> = Vec::new();
    let mut other: Vec<String> = Vec::new();
    for (key, value) in entries {
        let Some(name) = key.strip_prefix(prefix).and_then(|k| k.strip_suffix(suffix)) else { continue };
        let Some(pattern) = resolve(value) else { continue };
        if is_console(name) { console.push(pattern) } else { other.push(pattern) }
    }
    console.extend(other);
    console
}

/// `key = value` (or `key: value`) lines of a properties file, without comments.
fn key_values(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('!'))
        .filter_map(|l| l.split_once('=').or_else(|| l.split_once(':')))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
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

// ── the files the configuration writes to ───────────────────────────────────────

/// The logging configurations that can name a file the program writes its log to.
const FILE_SOURCES: &[&str] = &[
    "logback-spring.xml",
    "logback.xml",
    "log4j2-spring.xml",
    "log4j2.xml",
    "log4j2.properties",
    "log4j.xml",
    "log4j.properties",
];

/// The log files the project's logging configuration writes to, that exist: Log4j2's `fileName`,
/// Logback's `<file>`, Log4j 1.x's `File`. A name that needs a variable (`${sys:…}`) is skipped — it
/// cannot be known without running — and a relative one is taken from the project root, where the
/// program runs from.
pub(crate) fn declared_files(root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for dir in resource_dirs(root) {
        for name in FILE_SOURCES {
            let Some(text) = read_small(&dir.join(name)) else { continue };
            for value in declared_names(&text) {
                if value.is_empty() || value.contains("${") {
                    continue;
                }
                let path = Path::new(&value);
                let path = if path.is_absolute() { path.to_path_buf() } else { root.join(path) };
                if path.is_file() && !out.contains(&path) {
                    out.push(path);
                }
            }
        }
    }
    out
}

fn declared_names(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for tag in blocks(text, "<", ">") {
        names.extend(attribute(tag, "filename")); // `attribute` matches case-insensitively and wants the name lowercase
        if attribute(tag, "name").is_some_and(|n| n.eq_ignore_ascii_case("file")) {
            names.extend(attribute(tag, "value"));
        }
    }
    names.extend(elements(text, "file").into_iter().map(|f| unescape(f.trim())));
    names.extend(
        key_values(text)
            .into_iter()
            .filter(|(k, _)| k.ends_with(".fileName") || k.ends_with(".File") || k.ends_with(".file"))
            .map(|(_, v)| v),
    );
    names
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

    /// The usual Log4j2 shape: the pattern defined once in `<Properties>` and named in the layout.
    #[test]
    fn a_log4j2_property_is_substituted() {
        let xml = "<Configuration><Properties><Property name=\"LOG_PATTERN\">%d{HH:mm:ss.SSS} [%t] %-5level %logger{36} - %msg%n</Property></Properties>\
                   <Appenders><Console name=\"C\"><PatternLayout pattern=\"${LOG_PATTERN}\"/></Console></Appenders></Configuration>";
        assert_eq!(patterns_from_log4j2(xml), ["%d{HH:mm:ss.SSS} [%t] %-5level %logger{36} - %msg%n"]);
    }

    #[test]
    fn a_log4j2_system_property_drops_the_pattern() {
        let xml = "<Configuration><Appenders><Console name=\"C\"><PatternLayout pattern=\"${sys:LOG_PATTERN}\"/></Console></Appenders></Configuration>";
        assert!(patterns_from_log4j2(xml).is_empty());
    }

    #[test]
    fn log4j2_properties_name_the_console_pattern() {
        let text = "property.p = %d{HH:mm:ss} %level %msg%n\n\
                    appender.file.type = File\nappender.file.layout.pattern = %d %m\n\
                    appender.console.type = Console\nappender.console.layout.pattern = ${p}\n";
        assert_eq!(patterns_from_log4j2_properties(text), ["%d{HH:mm:ss} %level %msg%n", "%d %m"]);
    }

    #[test]
    fn log4j1_xml_reads_the_conversion_pattern() {
        let xml = "<log4j:configuration>\
                   <appender name=\"F\" class=\"org.apache.log4j.FileAppender\"><layout class=\"org.apache.log4j.PatternLayout\"><param name=\"ConversionPattern\" value=\"%d %m\"/></layout></appender>\
                   <appender name=\"C\" class=\"org.apache.log4j.ConsoleAppender\"><layout class=\"org.apache.log4j.PatternLayout\"><param name=\"ConversionPattern\" value=\"%d{HH:mm:ss} %-5p [%c] %m%n\"/></layout></appender>\
                   </log4j:configuration>";
        assert_eq!(patterns_from_log4j1_xml(xml), ["%d{HH:mm:ss} %-5p [%c] %m%n", "%d %m"]);
    }

    #[test]
    fn log4j1_properties_read_the_console_appender_first() {
        let text = "log4j.rootLogger=INFO, F, C\n\
                    log4j.appender.F=org.apache.log4j.FileAppender\nlog4j.appender.F.layout.ConversionPattern=%d %m\n\
                    log4j.appender.C=org.apache.log4j.ConsoleAppender\nlog4j.appender.C.layout.ConversionPattern=%d{HH:mm:ss} %-5p %c - %m%n\n";
        assert_eq!(patterns_from_log4j1_properties(text), ["%d{HH:mm:ss} %-5p %c - %m%n", "%d %m"]);
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
        let found = candidates(&dir);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file, "web/src/main/resources/logback.xml");
        assert!(found[0].usable);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_web_modules_classes_folder_is_searched() {
        let dir = std::env::temp_dir().join(format!("bennu-log-pattern-web-{}", std::process::id()));
        let classes = dir.join("Webapp/src/main/webapp/WEB-INF/classes");
        std::fs::create_dir_all(&classes).unwrap();
        std::fs::write(
            classes.join("log4j2.xml"),
            "<Configuration><Appenders><Console name=\"C\"><PatternLayout pattern=\"%d{HH:mm:ss} %-5level %logger{36} - %msg%n\"/></Console></Appenders></Configuration>",
        )
        .unwrap();
        assert!(detect(&dir).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pattern_the_parser_does_not_model_is_listed_but_not_used() {
        let dir = std::env::temp_dir().join(format!("bennu-log-pattern-odd-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src/main/resources")).unwrap();
        std::fs::write(
            dir.join("src/main/resources/logback.xml"),
            "<configuration><appender class=\"x.ConsoleAppender\"><pattern>%replace(%msg){'a','b'}%n</pattern></appender></configuration>",
        )
        .unwrap();
        let found = candidates(&dir);
        assert_eq!(found.len(), 1);
        assert!(!found[0].usable);
        assert!(detect(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_files_a_configuration_writes_to_are_named() {
        let log4j2 = "<Configuration><Appenders><File name=\"F\" fileName=\"logs/app.log\"/><File name=\"G\" fileName=\"${sys:dir}/x.log\"/></Appenders></Configuration>";
        assert_eq!(declared_names(log4j2), ["logs/app.log", "${sys:dir}/x.log"]);
        let logback = "<configuration><appender class=\"x.FileAppender\"><file>logs/web.log</file></appender></configuration>";
        assert_eq!(declared_names(logback), ["logs/web.log"]);
        let log4j1 = "<appender class=\"org.apache.log4j.FileAppender\"><param name=\"File\" value=\"logs/old.log\"/></appender>";
        assert_eq!(declared_names(log4j1), ["logs/old.log"]);
        assert_eq!(declared_names("log4j.appender.F.File=logs/p.log
log4j.appender.F.layout=x
"), ["logs/p.log"]);
    }

    #[test]
    fn a_project_without_configuration_has_no_layout() {
        let dir = std::env::temp_dir().join(format!("bennu-log-pattern-none-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(detect(&dir).is_none());
        assert!(candidates(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn attributes_are_read_in_either_quote_and_not_from_a_longer_name() {
        assert_eq!(attribute("<p name='a' value=\"b c\"/>", "value").as_deref(), Some("b c"));
        assert_eq!(attribute("<p classname=\"x\" name=\"y\"/>", "name").as_deref(), Some("y"));
        assert_eq!(attribute("<p other=\"x\"/>", "name"), None);
    }
}
