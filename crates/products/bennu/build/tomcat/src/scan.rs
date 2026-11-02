//! What a project's own files say it needs from a servlet container.
//!
//! Two facts, both of which the project already states and which Tomcat will only complain about
//! after it has started:
//!
//! * the **JNDI names** the application looks up — `<resource-ref>` and `<env-entry>` in `web.xml`,
//!   `@Resource(name = …)` and `lookup()` in Java, `<jta-data-source>` in `persistence.xml`,
//!   `jndi-name=` in Spring XML — so a context can be checked against them (a missing one is a
//!   `NameNotFoundException` at start-up, far from the cause);
//! * the **servlet namespace** the sources import, `javax.servlet` or `jakarta.servlet`, which must
//!   match the one Tomcat speaks.
//!
//! Text scanning, deliberately: it has to run over every file of a project in the time it takes to
//! open a dialog, and the patterns it looks for are literal.

/// One JNDI name the project declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    /// Relative to `java:comp/env/` — `jdbc/Gestionale`.
    pub name: String,
    /// `"datasource"` or `"env"`.
    pub kind: String,
    /// `WEB-INF/web.xml:18 · <resource-ref>` — where it was found, for the person to check.
    pub source: String,
}

/// Everything found so far; feed it files with [`Scan::file`].
#[derive(Debug, Clone, Default)]
pub struct Scan {
    pub requirements: Vec<Requirement>,
    /// Source files importing `javax.servlet`.
    pub javax_files: usize,
    /// Source files importing `jakarta.servlet`.
    pub jakarta_files: usize,
}

const ENV: &str = "java:comp/env/";

fn normalise(name: &str) -> String {
    let n = name.trim();
    n.strip_prefix(ENV).unwrap_or(n).to_string()
}

fn line_of(text: &str, at: usize) -> usize {
    text[..at].bytes().filter(|b| *b == b'\n').count() + 1
}

/// Every `open … close` pair in `text` as `(value, byte offset of value)`.
fn between<'a>(text: &'a str, open: &str, close: &str) -> Vec<(&'a str, usize)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = text[from..].find(open) {
        let start = from + rel + open.len();
        let Some(end_rel) = text[start..].find(close) else { break };
        out.push((&text[start..start + end_rel], start));
        from = start + end_rel + close.len();
    }
    out
}

impl Scan {
    fn add(&mut self, name: &str, kind: &str, file: &str, text: &str, at: usize, what: &str) {
        let name = normalise(name);
        if name.is_empty() || name.contains(char::is_whitespace) || self.requirements.iter().any(|r| r.name == name) {
            return;
        }
        self.requirements.push(Requirement {
            name,
            kind: kind.to_string(),
            source: format!("{file}:{} · {what}", line_of(text, at)),
        });
    }

    /// Read one file. `path` is only for the source label; the file's kind comes from its name.
    pub fn file(&mut self, path: &str, text: &str) {
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".java") {
            self.java(path, text);
        } else if lower.ends_with("web.xml") {
            self.web_xml(path, text);
        } else if lower.ends_with("persistence.xml") {
            for (v, at) in between(text, "<jta-data-source>", "</jta-data-source>") {
                self.add(v, "datasource", path, text, at, "<jta-data-source>");
            }
        } else if lower.ends_with(".xml") && text.contains("jndi-name") {
            for (v, at) in between(text, "jndi-name=\"", "\"") {
                self.add(v, "datasource", path, text, at, "jndi-name");
            }
        }
    }

    fn java(&mut self, path: &str, text: &str) {
        if text.contains("javax.servlet") {
            self.javax_files += 1;
        }
        if text.contains("jakarta.servlet") {
            self.jakarta_files += 1;
        }
        for marker in ["@Resource(", "@Resource ("] {
            let mut from = 0;
            while let Some(rel) = text[from..].find(marker) {
                let start = from + rel + marker.len();
                let end = text[start..].find(')').map_or(text.len(), |e| start + e);
                let args = &text[start..end];
                for key in ["name", "lookup"] {
                    if let Some(v) = quoted_after(args, key) {
                        // `Type` decides the kind; without one a `@Resource` is almost always a
                        // data source or a plain environment entry, and the type on the next line
                        // says which.
                        let tail = &text[end..text.len().min(end + 160)];
                        let kind = if tail.contains("DataSource") { "datasource" } else { "env" };
                        self.add(v, kind, path, text, start, "@Resource");
                    }
                }
                from = end;
            }
        }
        for (v, at) in between(text, "lookup(\"java:comp/env/", "\"") {
            self.add(v, "datasource", path, text, at, "lookup()");
        }
    }

    fn web_xml(&mut self, path: &str, text: &str) {
        let refs = between(text, "<res-ref-name>", "</res-ref-name>");
        for (v, at) in refs {
            // Only a data source is something this knows how to define; a mail session or a queue
            // would be a suggestion the form cannot fulfil.
            let window = &text[at..text.len().min(at + 240)];
            let kind = if window.contains("DataSource") { "datasource" } else { "" };
            if !kind.is_empty() {
                self.add(v, kind, path, text, at, "<resource-ref>");
            }
        }
        for (v, at) in between(text, "<env-entry-name>", "</env-entry-name>") {
            self.add(v, "env", path, text, at, "<env-entry>");
        }
        if text.contains("xmlns.jcp.org") || text.contains("java.sun.com/xml") {
            self.javax_files += 1;
        }
        if text.contains("jakarta.ee/xml") {
            self.jakarta_files += 1;
        }
    }

    /// `"javax"`, `"jakarta"`, or `None` when the project does not say (no servlet code at all) or
    /// says both — a migration half done, which is not something to guess a winner in.
    pub fn namespace(&self) -> Option<&'static str> {
        match (self.javax_files > 0, self.jakarta_files > 0) {
            (true, false) => Some("javax"),
            (false, true) => Some("jakarta"),
            _ => None,
        }
    }
}

/// The string literal assigned to `key` inside an annotation's arguments: `name = "x"` → `x`.
fn quoted_after<'a>(args: &'a str, key: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(rel) = args[from..].find(key) {
        let at = from + rel;
        let before_ok = at == 0 || !args.as_bytes()[at - 1].is_ascii_alphanumeric();
        let after = args[at + key.len()..].trim_start();
        if before_ok {
            if let Some(rest) = after.strip_prefix('=') {
                let rest = rest.trim_start();
                if let Some(lit) = rest.strip_prefix('"') {
                    return lit.split('"').next();
                }
            }
        }
        from = at + key.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_xml_declares_data_sources_and_environment_entries() {
        let xml = "<web-app xmlns=\"http://xmlns.jcp.org/xml/ns/javaee\">\n<resource-ref>\n<res-ref-name>jdbc/Gestionale</res-ref-name>\n<res-type>javax.sql.DataSource</res-type>\n</resource-ref>\n<resource-ref><res-ref-name>mail/Session</res-ref-name><res-type>javax.mail.Session</res-type></resource-ref>\n<env-entry><env-entry-name>app/modalita</env-entry-name></env-entry></web-app>";
        let mut s = Scan::default();
        s.file("WEB-INF/web.xml", xml);
        let names: Vec<(&str, &str)> = s.requirements.iter().map(|r| (r.name.as_str(), r.kind.as_str())).collect();
        assert_eq!(names, [("jdbc/Gestionale", "datasource"), ("app/modalita", "env")], "a mail session is not something the form can define");
        assert!(s.requirements[0].source.starts_with("WEB-INF/web.xml:3 · <resource-ref>"), "{:?}", s.requirements[0]);
        assert_eq!(s.namespace(), Some("javax"));
    }

    #[test]
    fn java_resources_come_from_the_annotation_and_the_lookup() {
        let java = "import javax.servlet.http.HttpServlet;\nclass A {\n  @Resource(name = \"jdbc/Reporting\")\n  private DataSource ds;\n  void f() { ctx.lookup(\"java:comp/env/jdbc/Storico\"); }\n  @Resource(lookup = \"java:comp/env/app/x\") String x;\n}";
        let mut s = Scan::default();
        s.file("A.java", java);
        let got: Vec<(&str, &str)> = s.requirements.iter().map(|r| (r.name.as_str(), r.kind.as_str())).collect();
        assert!(got.contains(&("jdbc/Reporting", "datasource")), "{got:?}");
        assert!(got.contains(&("jdbc/Storico", "datasource")), "{got:?}");
        assert!(got.contains(&("app/x", "env")), "{got:?}");
        assert_eq!(s.javax_files, 1);
    }

    #[test]
    fn persistence_and_spring_xml_name_data_sources_and_a_name_is_listed_once() {
        let mut s = Scan::default();
        s.file("META-INF/persistence.xml", "<persistence-unit><jta-data-source>java:comp/env/jdbc/Storico</jta-data-source></persistence-unit>");
        s.file("beans.xml", "<jee:jndi-lookup id=\"d\" jndi-name=\"jdbc/Storico\"/>");
        assert_eq!(s.requirements.len(), 1);
        assert_eq!(s.requirements[0].name, "jdbc/Storico");
    }

    #[test]
    fn a_namespace_is_only_claimed_when_the_project_is_unambiguous() {
        let mut s = Scan::default();
        assert_eq!(s.namespace(), None);
        s.file("A.java", "import jakarta.servlet.Filter;");
        assert_eq!(s.namespace(), Some("jakarta"));
        s.file("B.java", "import javax.servlet.Filter;");
        assert_eq!(s.namespace(), None, "half a migration is not a verdict");
    }
}
