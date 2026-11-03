//! Built-in grammars for the two logging configurations every Java project has.
//!
//! `log4j2.xml` and `logback.xml` are the files a developer edits most often after the POM, and —
//! like the POM — nothing on the build path ships a schema for them: Log4j2's `Log4j-config.xsd` is a
//! release artifact and not in the jar, and Logback has no schema at all. So, like [`crate::builtin`],
//! this is a curated vocabulary: what the documentation says may go where, with the attributes each
//! element takes and the values the closed ones accept.
//!
//! ## Completion and hover, never accusations
//!
//! Both are **plugin-based**: a Log4j2 appender, layout or filter can be one the project wrote, and a
//! Logback `class=` names any appender. A table of the ones that ship can therefore never be complete,
//! and reporting a custom appender as an unknown element would be the one thing worse than having no
//! table. So the grammars are marked [`Grammar::approximate`] — the flag that turns every check off
//! while leaving completion, ghost text and hover on, which is exactly the contract wanted here.
//!
//! ## How a row reads
//!
//! `(element, children, attributes, documentation)`. Children are space-separated. Attributes are
//! space-separated too: `name` is optional, `name!` required, and `name=A|B|C` a closed set of values,
//! where `$L` stands for the level names, `$A` for the filter answers and `$B` for `true|false` — written
//! once instead of forty times.

use crate::grammar::{Attribute, Child, Element, Grammar, GrammarKind};
use crate::scan::{Scan, TagKind};

type Row = (&'static str, &'static str, &'static str, &'static str);

const LOG4J2_LEVELS: &str = "ALL|TRACE|DEBUG|INFO|WARN|ERROR|FATAL|OFF";
const LOGBACK_LEVELS: &str = "ALL|TRACE|DEBUG|INFO|WARN|ERROR|OFF|INHERITED|NULL";
const FILTER_ANSWERS: &str = "ACCEPT|DENY|NEUTRAL";
const BOOLEAN: &str = "true|false";

const FILTERS: &str = "Filters ThresholdFilter LevelRangeFilter MarkerFilter RegexFilter BurstFilter TimeFilter \
                       DynamicThresholdFilter ContextMapFilter ThreadContextMapFilter MapFilter DenyAllFilter \
                       NoMarkerFilter ScriptFilter";

// ── Log4j2 ──────────────────────────────────────────────────────────────────────

/// Log4j2's configuration. The appenders and filters share a row each; what they share (the layouts
/// and filters inside) is spelled out in the children column of the appenders.
const LOG4J2: &[Row] = &[
    (
        "Configuration",
        "Properties CustomLevels Scripts Filters Appenders Loggers",
        "status=$L|trace|debug|info|warn|error|fatal monitorInterval name packages strict schema shutdownHook=enable|disable \
         shutdownTimeout verbose advertiser dest",
        "The root of a Log4j2 configuration. `status` is the level of Log4j's own internal messages; \
         `monitorInterval` is how many seconds between checks for a changed file.",
    ),
    ("Properties", "Property", "", "Values that can be named as ${name} anywhere in the file."),
    ("Property", "", "name!", "One property. Its text is the value."),
    ("CustomLevels", "CustomLevel", "", ""),
    ("CustomLevel", "", "name! intLevel!", "A level of your own, ordered by its integer."),
    ("Scripts", "Script ScriptFile", "", ""),
    ("Script", "", "name language", ""),
    ("ScriptFile", "", "name language path charset", ""),
    ("Appenders", "Console File RollingFile RandomAccessFile RollingRandomAccessFile MemoryMappedFile Async Syslog Socket SMTP JDBC Routing Rewrite Failover Kafka Null", "", "Where log events go."),
    (
        "Console",
        "PatternLayout JsonLayout XMLLayout Layout Filters ThresholdFilter",
        "name! target=SYSTEM_OUT|SYSTEM_ERR follow direct ignoreExceptions=$B bufferedIo=$B bufferSize",
        "Writes to the console. `follow` keeps tracking a changed System.out.",
    ),
    (
        "File",
        "PatternLayout JsonLayout XMLLayout YamlLayout CsvLayout HtmlLayout Layout Filters ThresholdFilter",
        "name! fileName! append=$B immediateFlush=$B bufferedIo=$B bufferSize createOnDemand=$B locking=$B ignoreExceptions=$B \
         filePermissions fileOwner fileGroup",
        "Writes to one file.",
    ),
    (
        "RollingFile",
        "PatternLayout JsonLayout XMLLayout YamlLayout CsvLayout Layout Filters ThresholdFilter Policies DefaultRolloverStrategy DirectWriteRolloverStrategy",
        "name! fileName filePattern! append=$B immediateFlush=$B bufferedIo=$B bufferSize createOnDemand=$B ignoreExceptions=$B \
         filePermissions fileOwner fileGroup",
        "Writes to a file and starts a new one by size or time. `filePattern` names the archives; a `.gz` or `.zip` ending compresses them.",
    ),
    (
        "RandomAccessFile",
        "PatternLayout JsonLayout XMLLayout Layout Filters",
        "name! fileName! append=$B immediateFlush=$B bufferSize ignoreExceptions=$B",
        "A faster File that always buffers.",
    ),
    (
        "RollingRandomAccessFile",
        "PatternLayout JsonLayout XMLLayout Layout Filters Policies DefaultRolloverStrategy",
        "name! fileName! filePattern! append=$B immediateFlush=$B bufferSize ignoreExceptions=$B",
        "A faster RollingFile that always buffers.",
    ),
    ("MemoryMappedFile", "PatternLayout Layout Filters", "name! fileName! append=$B immediateFlush=$B regionLength ignoreExceptions=$B", ""),
    (
        "Async",
        "AppenderRef Filters BlockingQueueFactory",
        "name! bufferSize blocking=$B shutdownTimeout includeLocation=$B ignoreExceptions=$B errorRef",
        "Hands events to another thread, so the caller does not wait for the appender it wraps.",
    ),
    ("BlockingQueueFactory", "", "", ""),
    ("Syslog", "PatternLayout RFC5424Layout Layout Filters", "name! host! port! protocol=TCP|UDP facility format appName", ""),
    ("Socket", "PatternLayout JsonLayout XMLLayout Layout Filters", "name! host! port! protocol=TCP|UDP|SSL reconnectionDelayMillis immediateFail", ""),
    ("SMTP", "PatternLayout HtmlLayout Layout Filters", "name! to from subject smtpHost smtpPort smtpUsername smtpPassword bufferSize", ""),
    ("JDBC", "ColumnMapping ConnectionFactory Filters", "name! tableName bufferSize ignoreExceptions=$B", ""),
    ("ColumnMapping", "", "name! pattern literal type", ""),
    ("ConnectionFactory", "", "class method", ""),
    ("Routing", "Routes Filters", "name! ignoreExceptions=$B", ""),
    ("Routes", "Route", "pattern", ""),
    ("Route", "Console File RollingFile", "key ref", ""),
    ("Rewrite", "AppenderRef Filters", "name! ignoreExceptions=$B", ""),
    ("Failover", "Failovers", "name! primary! retryIntervalSeconds ignoreExceptions=$B", ""),
    ("Failovers", "AppenderRef", "", ""),
    ("Kafka", "PatternLayout JsonLayout Property Filters", "name! topic! syncSend ignoreExceptions=$B", ""),
    ("Null", "", "name!", "Discards everything."),
    (
        "PatternLayout",
        "",
        "pattern charset alwaysWriteExceptions=$B noConsoleNoAnsi=$B disableAnsi=$B header footer replace",
        "Formats an event from a conversion pattern: `%d` the date, `%-5level` the level, `%logger{36}` the logger, `%t` the thread, `%msg` the message, `%n` a newline.",
    ),
    ("Layout", "", "type class", ""),
    (
        "JsonLayout",
        "KeyValuePair",
        "compact=$B eventEol=$B complete=$B properties=$B propertiesAsList=$B locationInfo=$B includeStacktrace=$B stacktraceAsString=$B charset",
        "One JSON object per event.",
    ),
    ("XMLLayout", "KeyValuePair", "compact=$B complete=$B properties=$B locationInfo=$B charset", ""),
    ("YamlLayout", "KeyValuePair", "properties=$B locationInfo=$B charset", ""),
    ("CsvLayout", "", "format delimiter escape quote quoteMode charset header footer", ""),
    ("HtmlLayout", "", "title contentType charset locationInfo=$B fontName fontSize=smaller|small|medium|large|larger", ""),
    ("RFC5424Layout", "", "enterpriseNumber facility id includeMDC=$B mdcId", ""),
    ("GelfLayout", "KeyValuePair", "host compressionType=GZIP|ZLIB|OFF compressionThreshold includeStacktrace=$B", ""),
    ("KeyValuePair", "", "key! value!", ""),
    (
        "Policies",
        "SizeBasedTriggeringPolicy TimeBasedTriggeringPolicy CronTriggeringPolicy OnStartupTriggeringPolicy",
        "",
        "When a RollingFile starts a new file. Any one of them is enough.",
    ),
    ("SizeBasedTriggeringPolicy", "", "size!", "Roll over when the file reaches this size: `10 MB`, `500 KB`."),
    ("TimeBasedTriggeringPolicy", "", "interval modulate=$B maxRandomDelay", "Roll over by the date pattern in `filePattern`; `interval` counts its smallest unit."),
    ("CronTriggeringPolicy", "", "schedule! evaluateOnStartup=$B", ""),
    ("OnStartupTriggeringPolicy", "", "minSize", ""),
    ("DefaultRolloverStrategy", "Delete", "max min fileIndex=min|max compressionLevel tempCompressedFilePattern stopCustomActionsOnError=$B", "How many archives to keep, and what to do with older ones."),
    ("DirectWriteRolloverStrategy", "Delete", "maxFiles compressionLevel tempCompressedFilePattern", ""),
    ("Delete", "IfFileName IfLastModified IfAccumulatedFileCount IfAccumulatedFileSize IfAny IfAll IfNot", "basePath! maxDepth followLinks=$B testMode=$B pathSorter", "Removes old archives when a rollover happens."),
    ("IfFileName", "IfLastModified IfAccumulatedFileCount IfAccumulatedFileSize", "glob regex", ""),
    ("IfLastModified", "", "age!", "A duration: `30d`, `12H`."),
    ("IfAccumulatedFileCount", "", "exceeds!", ""),
    ("IfAccumulatedFileSize", "", "exceeds!", ""),
    ("IfAny", "IfFileName IfLastModified IfAccumulatedFileCount IfAccumulatedFileSize IfAll IfNot", "", ""),
    ("IfAll", "IfFileName IfLastModified IfAccumulatedFileCount IfAccumulatedFileSize IfAny IfNot", "", ""),
    ("IfNot", "IfFileName IfLastModified IfAccumulatedFileCount IfAccumulatedFileSize IfAny IfAll", "", ""),
    ("Filters", FILTERS, "", "Filters, tried in order."),
    ("ThresholdFilter", "", "level=$L onMatch=$A onMismatch=$A", "Accepts events at or above `level`."),
    ("LevelRangeFilter", "", "minLevel=$L maxLevel=$L onMatch=$A onMismatch=$A", ""),
    ("MarkerFilter", "", "marker! onMatch=$A onMismatch=$A", ""),
    ("NoMarkerFilter", "", "onMatch=$A onMismatch=$A", ""),
    ("RegexFilter", "", "regex! useRawMsg=$B onMatch=$A onMismatch=$A", ""),
    ("BurstFilter", "", "level=$L rate maxBurst onMatch=$A onMismatch=$A", ""),
    ("TimeFilter", "", "start end timezone onMatch=$A onMismatch=$A", ""),
    ("DynamicThresholdFilter", "KeyValuePair", "key! defaultThreshold=$L onMatch=$A onMismatch=$A", ""),
    ("ContextMapFilter", "KeyValuePair", "operator=or|and onMatch=$A onMismatch=$A", ""),
    ("ThreadContextMapFilter", "KeyValuePair", "operator=or|and onMatch=$A onMismatch=$A", ""),
    ("MapFilter", "KeyValuePair", "operator=or|and onMatch=$A onMismatch=$A", ""),
    ("DenyAllFilter", "", "", ""),
    ("ScriptFilter", "Script ScriptRef", "onMatch=$A onMismatch=$A", ""),
    ("ScriptRef", "", "ref", ""),
    ("Loggers", "Root Logger AsyncRoot AsyncLogger", "", "The loggers and the level and appenders each one has."),
    ("Root", "AppenderRef Filters Properties", "level=$L includeLocation=$B", "The logger every other one inherits from."),
    ("Logger", "AppenderRef Filters Properties", "name! level=$L additivity=$B includeLocation=$B", "A logger by name; it covers everything below it in the package tree."),
    ("AsyncRoot", "AppenderRef Filters", "level=$L includeLocation=$B", "The root logger, asynchronous."),
    ("AsyncLogger", "AppenderRef Filters", "name! level=$L additivity=$B includeLocation=$B", ""),
    ("AppenderRef", "Filters", "ref! level", "Sends this logger's events to the appender named `ref`."),
];

// ── Logback ─────────────────────────────────────────────────────────────────────

const LOGBACK: &[Row] = &[
    (
        "configuration",
        "statusListener contextListener include property springProperty springProfile contextName jmxConfigurator \
         shutdownHook timestamp define conversionRule appender logger root turboFilter insertFromJNDI",
        "debug=$B scan=$B scanPeriod packagingData=$B",
        "The root of a Logback configuration. `scan` reloads the file when it changes, every `scanPeriod`.",
    ),
    ("statusListener", "", "class!", "Where Logback's own status messages go."),
    ("contextListener", "", "class!", ""),
    ("include", "", "file url resource optional=$B", "Another configuration file merged in here."),
    ("property", "", "name value file resource scope=local|context|system", "A value that can be named as ${name} in the rest of the file."),
    ("springProperty", "", "name! source! scope=local|context|system defaultValue", "Spring Boot only: a property of the Spring Environment."),
    (
        "springProfile",
        "property springProperty include appender logger root contextName",
        "name!",
        "Spring Boot only: what is inside applies only when the profile is active. `!prod` negates.",
    ),
    ("contextName", "", "", "A name for this logging context."),
    ("jmxConfigurator", "", "", ""),
    ("shutdownHook", "", "class delay", ""),
    ("timestamp", "", "key! datePattern! timeReference", "The time the configuration was loaded, as ${key}."),
    ("define", "", "name! class!", ""),
    ("conversionRule", "", "conversionWord! converterClass!", "A new `%word` for patterns."),
    ("turboFilter", "", "class!", ""),
    ("insertFromJNDI", "", "env-entry-name as scope=local|context|system", ""),
    (
        "appender",
        "encoder layout file append prudent immediateFlush target withJansi rollingPolicy triggeringPolicy filter \
         appender-ref queueSize discardingThreshold includeCallerData maxFlushTime neverBlock discriminator sift param",
        "name! class!",
        "Where log events go. `class` is the appender: ch.qos.logback.core.ConsoleAppender, FileAppender, \
         rolling.RollingFileAppender, classic.AsyncAppender, …",
    ),
    ("encoder", "pattern charset immediateFlush outputPatternAsHeader layout", "class", "Turns an event into bytes."),
    ("layout", "pattern", "class", ""),
    (
        "pattern",
        "",
        "",
        "The conversion pattern: `%d` the date, `%-5level` the level, `%logger{36}` the logger, `%thread` the thread, `%msg` the message, `%n` a newline.",
    ),
    ("charset", "", "", ""),
    ("immediateFlush", "", "", ""),
    ("outputPatternAsHeader", "", "", ""),
    ("file", "", "", "The file to write to."),
    ("append", "", "", "true to add to an existing file, false to start it again."),
    ("prudent", "", "", "Safe for several JVMs writing the same file; slower."),
    ("target", "", "", "System.out or System.err."),
    ("withJansi", "", "", ""),
    ("rollingPolicy", "fileNamePattern maxHistory totalSizeCap cleanHistoryOnStart maxFileSize minIndex maxIndex timeBasedFileNamingAndTriggeringPolicy", "class!", "When a RollingFileAppender starts a new file, and how many to keep."),
    ("fileNamePattern", "", "", "The archive name, with `%d{yyyy-MM-dd}` for time-based rolling and `%i` for size-based. A `.gz` or `.zip` ending compresses it."),
    ("maxHistory", "", "", "How many archives to keep."),
    ("totalSizeCap", "", "", "The most all archives may take together: `3GB`."),
    ("cleanHistoryOnStart", "", "", ""),
    ("maxFileSize", "", "", "`10MB`. Needs `%i` in `fileNamePattern`."),
    ("minIndex", "", "", ""),
    ("maxIndex", "", "", ""),
    ("timeBasedFileNamingAndTriggeringPolicy", "maxFileSize", "class!", ""),
    ("triggeringPolicy", "maxFileSize", "class!", ""),
    ("filter", "level onMatch onMismatch", "class!", ""),
    ("level", "", "", "TRACE, DEBUG, INFO, WARN or ERROR."),
    ("onMatch", "", "", "ACCEPT, DENY or NEUTRAL."),
    ("onMismatch", "", "", "ACCEPT, DENY or NEUTRAL."),
    ("appender-ref", "", "ref!", "Sends the events to the appender named `ref`."),
    ("queueSize", "", "", ""),
    ("discardingThreshold", "", "", ""),
    ("includeCallerData", "", "", ""),
    ("maxFlushTime", "", "", ""),
    ("neverBlock", "", "", ""),
    ("discriminator", "key defaultValue", "class", ""),
    ("key", "", "", ""),
    ("defaultValue", "", "", ""),
    ("sift", "appender", "", ""),
    ("param", "", "name value", ""),
    (
        "logger",
        "appender-ref",
        "name! level=$L additivity=$B",
        "A logger by name; it covers everything below it in the package tree. `additivity=\"false\"` stops its events reaching the root's appenders.",
    ),
    ("root", "appender-ref", "level=$L", "The logger every other one inherits from."),
];

// ── building ────────────────────────────────────────────────────────────────────

/// The Log4j2 grammar.
pub fn log4j2() -> Grammar {
    build("Log4j2 configuration (built in)", "Configuration", LOG4J2, LOG4J2_LEVELS)
}

/// The Logback grammar.
pub fn logback() -> Grammar {
    build("Logback configuration (built in)", "configuration", LOGBACK, LOGBACK_LEVELS)
}

fn build(source: &str, root: &str, rows: &[Row], levels: &str) -> Grammar {
    Grammar {
        source: source.to_string(),
        kind: Some(GrammarKind::Builtin),
        roots: vec![root.to_string()],
        // Plugin-based: a table of what ships is never the whole vocabulary, so the grammar is
        // trusted for completion and not for accusations — see the module docs.
        approximate: true,
        elements: rows
            .iter()
            .map(|(name, children, attributes, doc)| Element {
                name: name.to_string(),
                children: children.split_whitespace().map(|n| Child { name: n.to_string(), required: false }).collect(),
                attributes: attributes.split_whitespace().map(|spec| attribute(spec, levels)).collect(),
                text: children.is_empty(),
                open: false,
                doc: doc.to_string(),
                decl: Default::default(),
            })
            .collect(),
    }
}

/// `name`, `name!` or `name=A|B|C` as an [`Attribute`], with the shorthands expanded.
fn attribute(spec: &str, levels: &str) -> Attribute {
    let (head, values) = spec.split_once('=').map_or((spec, ""), |(h, v)| (h, v));
    let (name, required) = head.strip_suffix('!').map_or((head, false), |n| (n, true));
    let values = values
        .split('|')
        .filter(|v| !v.is_empty())
        .flat_map(|v| match v {
            "$L" => levels.split('|').map(str::to_string).collect::<Vec<_>>(),
            "$A" => FILTER_ANSWERS.split('|').map(str::to_string).collect(),
            "$B" => BOOLEAN.split('|').map(str::to_string).collect(),
            other => vec![other.to_string()],
        })
        .collect();
    Attribute { name: name.to_string(), required, values, ..Attribute::default() }
}

// ── recognising ─────────────────────────────────────────────────────────────────

/// Elements only a Logback file has, which tell one from any other document that calls its root
/// `configuration`.
const LOGBACK_SIGNS: &[&str] = &[
    "appender", "root", "logger", "springProfile", "springProperty", "contextName", "statusListener", "turboFilter",
    "conversionRule", "shutdownHook",
];

/// The logging grammar for a document, judged by what it contains.
///
/// A Log4j2 file is recognised by its root: `Configuration`, capitalised, is a name nothing else in
/// a Java project uses. A Logback file's root is the common word `configuration`, so one of its own
/// elements has to be inside as well — which is why [`grammar_for_file`] also takes the file name.
pub fn grammar_for(scan: &Scan) -> Option<Grammar> {
    let root = scan.tags.iter().find(|t| t.kind != TagKind::Close)?;
    match root.local() {
        "Configuration" => Some(log4j2()),
        "configuration" if scan.tags.iter().any(|t| LOGBACK_SIGNS.contains(&t.local())) => Some(logback()),
        _ => None,
    }
}

/// As [`grammar_for`], and also for a file named like a logging configuration — so a `logback.xml`
/// that is just `<configuration>` and a caret already completes.
pub fn grammar_for_file(scan: &Scan, doc_path: &str) -> Option<Grammar> {
    grammar_for(scan).or_else(|| {
        let root = scan.tags.iter().find(|t| t.kind != TagKind::Close)?;
        let name = doc_path.rsplit(['/', '\\']).next()?.to_ascii_lowercase();
        (root.local() == "configuration" && name.starts_with("logback")).then(logback)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan;

    fn names_resolve(g: &Grammar) {
        for e in &g.elements {
            for c in e.child_names() {
                assert!(g.element(c).is_some(), "`{c}` is a child of `{}` but is not declared", e.name);
            }
        }
    }

    #[test]
    fn every_name_a_row_offers_is_declared() {
        names_resolve(&log4j2());
        names_resolve(&logback());
    }

    #[test]
    fn a_log4j2_file_is_recognised_by_its_root() {
        let g = grammar_for(&scan("<Configuration status=\"WARN\"><Appenders/></Configuration>")).expect("Log4j2");
        assert_eq!(g.roots, ["Configuration"]);
        assert!(grammar_for(&scan("<configuration/>")).is_none(), "a bare lowercase root is not enough");
    }

    #[test]
    fn a_logback_file_is_recognised_by_what_is_inside_or_by_its_name() {
        assert!(grammar_for(&scan("<configuration><appender name=\"A\" class=\"x\"/></configuration>")).is_some());
        assert!(grammar_for(&scan("<configuration></configuration>")).is_none());
        assert!(grammar_for_file(&scan("<configuration>"), "/p/src/main/resources/logback-spring.xml").is_some());
        assert!(grammar_for_file(&scan("<configuration>"), "/p/other.xml").is_none());
        assert!(grammar_for(&scan("<project/>")).is_none());
    }

    #[test]
    fn the_nesting_people_type_resolves() {
        let g = log4j2();
        for (parent, child) in [
            ("Configuration", "Appenders"),
            ("Appenders", "Console"),
            ("Console", "PatternLayout"),
            ("Appenders", "RollingFile"),
            ("RollingFile", "Policies"),
            ("Policies", "SizeBasedTriggeringPolicy"),
            ("Loggers", "Root"),
            ("Root", "AppenderRef"),
        ] {
            assert!(g.element(parent).unwrap().child_names().contains(&child), "{parent} → {child}");
        }
        let l = logback();
        for (parent, child) in [("configuration", "appender"), ("appender", "encoder"), ("encoder", "pattern"), ("root", "appender-ref")] {
            assert!(l.element(parent).unwrap().child_names().contains(&child), "{parent} → {child}");
        }
    }

    #[test]
    fn attributes_carry_their_required_flag_and_their_values() {
        let g = log4j2();
        let file = g.element("File").unwrap();
        assert!(file.attribute("fileName").unwrap().required);
        assert!(!file.attribute("append").unwrap().required);
        assert_eq!(file.attribute("append").unwrap().values, ["true", "false"]);
        let root = g.element("Root").unwrap();
        assert!(root.attribute("level").unwrap().values.contains(&"DEBUG".to_string()));
        assert_eq!(g.element("ThresholdFilter").unwrap().attribute("onMatch").unwrap().values, ["ACCEPT", "DENY", "NEUTRAL"]);
        // Logback's levels are its own.
        assert!(logback().element("logger").unwrap().attribute("level").unwrap().values.contains(&"INHERITED".to_string()));
        assert!(!log4j2().element("Root").unwrap().attribute("level").unwrap().values.contains(&"INHERITED".to_string()));
    }

    /// The whole reason the grammar is safe to ship: a project's own appender is not an error.
    #[test]
    fn a_curated_vocabulary_makes_no_accusations() {
        assert!(log4j2().approximate && logback().approximate);
        let doc = "<Configuration><Appenders><MyCompanyAppender name=\"x\" secret=\"1\"/></Appenders></Configuration>";
        let g = grammar_for(&scan(doc)).unwrap();
        assert!(crate::intel::diagnostics(&g, &scan(doc)).is_empty());
    }
}
