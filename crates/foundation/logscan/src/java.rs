//! The JVM dialect: qualified names, exceptions, and stack-trace frames.
//!
//! A stack trace is the one place in a log where the text is a *reference* rather than a
//! message — `at com.acme.Order.total(Order.java:118)` names a file and a line, and a
//! console that cannot take you there is asking you to retype it into Go-to-Class. So the
//! frame rule produces a [`Link::Source`], which carries the class the frame named and
//! leaves resolving it to the host: turning `com.acme.Order` into a path needs a project
//! index, and a log interpreter that owned one would be a Java tool rather than a log
//! interpreter.
//!
//! What the rules here recognise:
//!
//! | Shape | Token |
//! |---|---|
//! | `at com.acme.Order.total(Order.java:118)` | [`Token::Package`] + [`Token::Frame`], both linked |
//! | `at java.base/java.lang.Thread.run(Thread.java:840)` | the same — the module prefix is dropped from the class |
//! | `at com.acme.Foo.bar(Native Method)` | [`Token::Frame`], no link (there is no source to open) |
//! | `com.acme.OrderNotFoundException` | [`Token::Exception`] |
//! | `com.acme.order.OrderService` | [`Token::Package`] |
//! | `NullPointerException` | [`Token::Exception`] |

use crate::model::{Link, Token};
use crate::rule::{FnRule, Hit, RuleSet};
use crate::scan::token_end;

impl RuleSet {
    /// [`RuleSet::common`] plus the JVM rules, and a continuation test that knows what a
    /// stack trace looks like — so the twenty frames under an `ERROR` inherit its level
    /// instead of reading as twenty unrelated lines.
    pub fn java() -> Self {
        RuleSet::common()
            .with(FnRule::new("java-frame", frame_rule))
            .with(FnRule::new("java-name", qualified_rule))
            .continued_by(java_continues)
    }
}

/// Whether a line continues the one above it: an indented line, a stack frame, a
/// `Caused by:` / `Suppressed:` head, or the `... 23 more` tail.
pub fn java_continues(text: &str) -> bool {
    // The language-neutral shapes first: indentation, and a compiler's source gutter. The console
    // that uses this rule set serves a `cargo` run as readily as a Maven one, and a `35 |` line
    // falling out of its diagnostic is visible as a red line inside an amber block.
    if crate::common::common_continues(text) {
        return true;
    }
    let t = text.trim_start();
    t.starts_with("at ")
        || t.starts_with("Caused by:")
        || t.starts_with("Suppressed:")
        || t.starts_with("... ")
}

/// The class a frame's qualified method reference belongs to — module prefix and method
/// name removed. `java.base/java.lang.Thread.run` → `java.lang.Thread`.
pub fn class_of(qualified_method: &str) -> String {
    // JDK 9+ frames are `module/class.method`, and a named classloader can precede even
    // that (`app//com.acme.Foo.bar`). The class is what follows the last `/`.
    let after_module = qualified_method.rsplit('/').next().unwrap_or(qualified_method);
    after_module.rsplit_once('.').map(|(class, _)| class).unwrap_or(after_module).to_string()
}

/// The outer class of a possibly-nested binary name — `com.acme.Foo$Inner$1` →
/// `com.acme.Foo`. That is the one with a source file, so it is what a host resolving a
/// frame against a class index should look up.
pub fn outer_class(class: &str) -> &str {
    match class.find('$') {
        Some(i) => &class[..i],
        None => class,
    }
}

/// The method a frame's qualified reference names — `com.acme.Order.total` → `total`.
/// `<init>` and `<clinit>` come through as written; a host looking them up finds no member
/// and falls back to the type, which is the right place for a constructor anyway.
pub fn method_of(qualified_method: &str) -> Option<&str> {
    let after_module = qualified_method.rsplit('/').next().unwrap_or(qualified_method);
    after_module.rsplit_once('.').map(|(_, method)| method).filter(|m| !m.is_empty())
}

/// Whether a frame names something **made at runtime** — a lambda's carrier, a dynamic
/// proxy, a generated reflection accessor. Ask it of the whole `class.method` reference: a
/// lambda carrier's name contains a `/` that the module-prefix rule would otherwise eat.
///
/// These have no source anywhere: not in the project, not in a jar, not in a `-sources.jar`.
/// A link on one is a link that always fails, and a console full of those teaches you not to
/// click any of them.
pub fn is_synthetic(class: &str) -> bool {
    class.contains("$$Lambda")
        || class.contains("$Proxy")
        || class.contains("$$EnhancerBy") // CGLIB — Spring's proxies, up to 5.1
        || class.contains("$$SpringCGLIB$$") // …and from 5.2, where the name changed
        || class.contains("$HibernateProxy")
        || class.contains("GeneratedMethodAccessor")
        || class.contains("GeneratedConstructorAccessor")
        || class.contains("$$FastClassBy")
        || class.contains("_$$_jvst") // Javassist — Hibernate's lazy proxies
        || class.contains("$$_javassist")
        || class.contains("$MockitoMock$")
        || class.contains("$ByteBuddy$")
}

/// `at com.acme.Order.total(Order.java:118)`.
fn frame_rule(text: &str, at: usize) -> Option<Hit> {
    let rest = &text[at..];
    if !rest.starts_with("at ") {
        return None;
    }
    let mut qstart = at + 3;
    while text[qstart..].starts_with(' ') {
        qstart += 1;
    }
    let qend = token_end(text, qstart);
    let qual = &text[qstart..qend];
    if qend <= qstart || !qual.contains('.') {
        return None;
    }
    let has_location = text[qend..].starts_with('(');
    // Without a `(File.java:line)` this could be the English word "at" in front of a dotted
    // something. Two dots at least, then — `at java.io options` stays prose.
    if !has_location && qual.matches('.').count() < 2 {
        return None;
    }

    let mut end = qend;
    let mut link = None;
    let mut location = None;
    if has_location {
        let close = qend + text[qend..].find(')')?;
        let inner = &text[qend + 1..close];
        end = close + 1;
        location = Some((qend, end));
        // `(Native Method)` and `(Unknown Source)` have no line, and nothing to open.
        if let Some((file, number)) = inner.rsplit_once(':') {
            if let Ok(line) = number.trim().parse::<u32>() {
                let class = class_of(qual);
                // A lambda carrier or a proxy has no source anywhere. Marked, never linked.
                // Tested against the WHOLE reference: a lambda's name carries a `/` of its own
                // (`Svc$$Lambda$14/0x0001.run`), which the module-prefix rule would eat.
                if !is_synthetic(qual) {
                    link = Some(Link::Source {
                        method: method_of(qual).map(str::to_string),
                        class,
                        file: Some(file.trim().to_string()),
                        line: Some(line),
                    });
                }
            }
        }
    }

    let mut hit = Hit::spanning(end).part(qstart, qend, Token::Package, link.clone());
    if let Some((start, stop)) = location {
        hit = hit.part(start, stop, Token::Frame, link);
    }
    Some(hit)
}

/// A dotted qualified name — a logger, a package, a class — or a name that reads as a
/// throwable.
fn qualified_rule(text: &str, at: usize) -> Option<Hit> {
    let mut end = token_end(text, at);
    // The `:` after an exception in `Caused by: com.acme.Foo: boom`, and the full stop at
    // the end of a sentence, are punctuation around the name.
    while end > at && matches!(text[..end].chars().next_back(), Some('.') | Some(':') | Some(',')) {
        end -= 1;
    }
    if end <= at {
        return None;
    }
    let word = &text[at..end];
    let segments: Vec<&str> = word.split('.').collect();
    if segments.iter().any(|s| !is_java_ident(s)) {
        return None;
    }
    let last = segments[segments.len() - 1];
    let dots = segments.len() - 1;

    // One dot is only a qualified name when it reads as one: `com.Acme` yes, `foo.bar` no —
    // otherwise every `object.method` in a message becomes a package.
    let qualified = dots >= 2 || (dots == 1 && starts_upper(last));
    if !qualified {
        // A bare `OrderNotFoundException` is worth marking even with no package in front of
        // it. Long enough to exclude the bare word `Exception`, which is prose.
        let bare_throwable =
            dots == 0 && starts_upper(last) && last.len() > 9 && last.ends_with("Exception");
        return bare_throwable.then(|| Hit::one(at, end, Token::Exception));
    }
    let token = if is_throwable_name(last) { Token::Exception } else { Token::Package };
    Some(Hit::one(at, end, token))
}

fn is_java_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

fn starts_upper(s: &str) -> bool {
    s.chars().next().is_some_and(char::is_uppercase)
}

/// The convention every JVM codebase follows, and the only signal available without
/// resolving the type: a throwable's name says so.
fn is_throwable_name(simple: &str) -> bool {
    simple.ends_with("Exception") || simple.ends_with("Error") || simple.ends_with("Throwable")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Level;
    use crate::reader::{interpret, LogReader};

    fn tokens(text: &str) -> Vec<(String, Token)> {
        let line = interpret(&RuleSet::java(), text);
        line.spans.iter().map(|s| (line.text[s.start..s.end].to_string(), s.token)).collect()
    }

    fn links(text: &str) -> Vec<Link> {
        interpret(&RuleSet::java(), text).links().cloned().collect()
    }

    #[test]
    fn a_frame_links_its_class_and_its_location() {
        let text = "\tat com.acme.Order.total(Order.java:118)";
        let got = tokens(text);
        assert_eq!(got[0], ("com.acme.Order.total".into(), Token::Package));
        assert_eq!(got[1], ("(Order.java:118)".into(), Token::Frame));
        let expected = Link::Source {
            class: "com.acme.Order".into(),
            method: Some("total".into()),
            file: Some("Order.java".into()),
            line: Some(118),
        };
        assert_eq!(links(text), vec![expected.clone(), expected]);
    }

    #[test]
    fn a_runtime_made_class_is_marked_but_never_linked() {
        // A lambda carrier, a Spring CGLIB proxy: no source exists anywhere, so a link on one
        // is a link that always fails.
        for frame in [
            "\tat com.acme.Svc$$Lambda$14/0x00000008001.run(Unknown Source:0)",
            "\tat com.acme.Svc$$EnhancerBySpringCGLIB$$1a2b.save(<generated>:1)",
            // Spring 5.2 renamed its proxies. A generated class that is not recognised as one
            // resolves through its outer name — which opens the real `Svc.java` at whatever
            // line the proxy claims, i.e. the right file at a meaningless point in it.
            "\tat com.acme.Svc$$SpringCGLIB$$0.save(<generated>:1)",
            "\tat com.acme.Order_$$_jvst1a2_3.getLines(Order_$$_jvst1a2_3.java:0)",
            "\tat jdk.internal.reflect.GeneratedMethodAccessor12.invoke(Unknown:5)",
        ] {
            assert!(links(frame).is_empty(), "for {frame}");
            assert!(tokens(frame).iter().any(|(_, t)| *t == Token::Frame), "for {frame}");
        }
    }

    #[test]
    fn a_module_prefixed_frame_resolves_to_the_class_alone() {
        let got = links("\tat java.base/java.lang.Thread.run(Thread.java:840)");
        assert!(matches!(&got[0], Link::Source { class, .. } if class == "java.lang.Thread"));
    }

    #[test]
    fn a_frame_with_no_source_is_marked_but_not_clickable() {
        let text = "\tat com.acme.Proxy.invoke(Native Method)";
        assert_eq!(tokens(text)[1].1, Token::Frame);
        assert!(links(text).is_empty());
    }

    #[test]
    fn the_word_at_in_a_sentence_is_not_a_frame() {
        // One dot, no location: prose. (`java.io` is a real package, which is the point —
        // the rule cannot lean on the name being unusual.)
        assert!(tokens("look at java.io options").is_empty());
    }

    #[test]
    fn an_exception_is_told_apart_from_a_package() {
        let got = tokens("Caused by: com.acme.OrderNotFoundException: no such order");
        assert_eq!(got[0], ("com.acme.OrderNotFoundException".into(), Token::Exception));
    }

    #[test]
    fn a_bare_exception_name_still_counts() {
        assert_eq!(tokens("threw NullPointerException here")[0].1, Token::Exception);
        // …but the word itself is prose.
        assert!(tokens("an Exception was thrown").is_empty());
    }

    #[test]
    fn a_logger_is_a_package() {
        assert_eq!(tokens("com.acme.order.OrderService starting")[0].1, Token::Package);
    }

    #[test]
    fn a_method_call_in_prose_is_not_a_package() {
        assert!(tokens("calling order.total again").is_empty());
    }

    #[test]
    fn a_version_number_is_not_a_qualified_name() {
        assert!(tokens("java 1.8.0_292 detected").is_empty());
    }

    #[test]
    fn a_whole_stack_trace_inherits_the_level_of_its_error() {
        let mut reader = LogReader::new(RuleSet::java());
        let levels: Vec<_> = [
            "ERROR failed to place the order",
            "com.acme.OrderNotFoundException: no such order",
            "\tat com.acme.Order.total(Order.java:118)",
            "Caused by: java.lang.NullPointerException",
            "\t... 23 more",
        ]
        .iter()
        .map(|l| reader.read(l).level)
        .collect();
        assert!(levels.iter().all(|l| *l == Some(Level::Error)), "{levels:?}");
    }

    #[test]
    fn an_unheralded_exception_line_is_an_error_by_itself() {
        let mut reader = LogReader::new(RuleSet::java());
        let line = reader.read("Exception in thread \"main\" java.lang.IllegalStateException: boom");
        assert_eq!(line.level, Some(Level::Error));
    }

    #[test]
    fn an_ordinary_line_ends_the_inheritance() {
        let mut reader = LogReader::new(RuleSet::java());
        reader.read("ERROR boom");
        reader.read("\tat com.acme.Order.total(Order.java:118)");
        assert_eq!(reader.read("carrying on").level, None);
        assert_eq!(reader.read("\tat com.acme.Order.total(Order.java:118)").level, None);
    }

    #[test]
    fn outer_class_is_where_the_source_is() {
        assert_eq!(outer_class("com.acme.Foo$Inner$1"), "com.acme.Foo");
        assert_eq!(outer_class("com.acme.Foo"), "com.acme.Foo");
    }
}
