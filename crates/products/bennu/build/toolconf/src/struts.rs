//! `struts.properties` — the constants Struts 2 reads, as data.
//!
//! The third file of this shape, and the one where it earns the most: a legacy Struts application
//! is configured almost entirely through constants, half of them security-relevant, and their only
//! documentation is a `default.properties` inside `struts2-core.jar` that nobody opens.
//!
//! ## The same file, three places
//!
//! A constant may be written in `struts.properties`, as `<constant name="…" value="…"/>` in
//! `struts.xml`, or as a `<init-param>` in `web.xml`. The vocabulary is identical in all three —
//! this table is that vocabulary — and what is served here is the `.properties` spelling, because
//! that is the file whose keys nothing else in the editor can explain. The XML spelling already
//! has a DTD behind it.
//!
//! ## Where the versions stop
//!
//! As in the other two tables, and it matters more here: Struts 2 has been released for eighteen
//! years, most of these constants have been there the whole time, and the ones that moved did so
//! for security reasons that are worth being exactly right about. So a `since` is recorded **only**
//! where the introducing release is certain — the 2.5 security constants, which are documented in
//! the release notes — and every other key is offered to every project. A gate that is missing
//! costs a key too many; a gate that is wrong hides a key the project really has.
//!
//! Two defaults **changed** between releases rather than the key arriving
//! (`struts.enable.DynamicMethodInvocation`, `struts.devMode`), and that is written in the prose
//! rather than in `default_value`: the field is what the tool does today, and a note is the only
//! honest way to say "and it used to be the other one".

use crate::model::{Catalogue, ConfigKey};

const BOOL: &[&str] = &["true", "false"];

const KEYS: &[ConfigKey] = &[
    // ── The two everybody sets first ─────────────────────────────────────────
    ConfigKey::new(
        "struts.devMode",
        "boolean",
        "Development mode: reloads configuration and resource bundles on every request, turns on \
         the extra validation and makes Struts report problems loudly instead of swallowing them. \
         **Never true in production** — it costs a full configuration reload per request and \
         reports internals in the response.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "struts.i18n.encoding",
        "charset",
        "The encoding used to decode request parameters and to write responses. On a legacy \
         application this is the one setting that decides whether accented characters survive a \
         form post.",
    )
    .default_value("UTF-8"),
    // ── Which URLs reach Struts ──────────────────────────────────────────────
    ConfigKey::new(
        "struts.action.extension",
        "list",
        "The URL suffixes the filter handles, comma-separated. An empty entry means \"no \
         extension\" — which is why the default ends in a comma, and why deleting that comma \
         quietly stops extensionless URLs from mapping.",
    )
    .default_value("action,,"),
    ConfigKey::new(
        "struts.action.excludePattern",
        "list",
        "Regular expressions for URLs the filter passes straight through. What a REST endpoint or \
         a servlet living beside a Struts application needs in order to be reachable at all.",
    ),
    ConfigKey::new(
        "struts.enable.SlashesInActionNames",
        "boolean",
        "Allow `/` inside an action name, so one action can serve a whole path.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "struts.mapper.alwaysSelectFullNamespace",
        "boolean",
        "Read everything before the last `/` as the namespace, instead of matching the longest \
         namespace that exists. The Convention plugin turns this on.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "struts.mapper.class",
        "class name",
        "The `ActionMapper` that turns a request URL into an action invocation. Replaced by the \
         REST plugin, and by anything that wants URLs Struts does not natively produce.",
    ),
    // ── Security: the constants worth reading twice ──────────────────────────
    ConfigKey::new(
        "struts.enable.DynamicMethodInvocation",
        "boolean",
        "Allow `action!method` URLs to choose which method runs. This is remote method selection \
         from a URL: it was on by default before 2.5 and is off from 2.5 on, and turning it back \
         on re-opens what several advisories were about.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "struts.strictMethodInvocation",
        "boolean",
        "Only methods a package or action explicitly allows may be called through the URL. The \
         replacement for dynamic method invocation, and the reason it could be turned off.",
    )
    .default_value("true")
    .values(BOOL)
    .since("2.5"),
    ConfigKey::new(
        "struts.ognl.allowStaticFieldAccess",
        "boolean",
        "Allow OGNL expressions to read static fields (`@java.lang.Integer@MAX_VALUE`). Static \
         **method** access is gone entirely from 2.5 — this is the half that remains, and it is \
         still reachable from user input wherever an expression is evaluated.",
    )
    .default_value("false")
    .values(BOOL)
    .since("2.5"),
    ConfigKey::new(
        "struts.ognl.allowStaticMethodAccess",
        "boolean",
        "Allowed OGNL expressions to call static methods. Removed in 2.5 — a value here does \
         nothing, and the reason it was removed is that reaching arbitrary statics from an \
         expression is remote code execution wherever the expression comes from a parameter.",
    )
    .default_value("false")
    .values(BOOL)
    .deprecated("2.5", "struts.ognl.allowStaticFieldAccess"),
    ConfigKey::new(
        "struts.disallowProxyMemberAccess",
        "boolean",
        "Refuse OGNL access to proxied objects — the way a Spring or Hibernate proxy could be \
         used to reach what its target refuses.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "struts.excludedClasses",
        "list",
        "Classes OGNL may never touch, comma-separated. The deny list every Struts advisory ends \
         by extending — replacing it wholesale rather than appending is how an application \
         silently loses the entries a later release added.",
    ),
    ConfigKey::new(
        "struts.excludedPackageNames",
        "list",
        "Whole packages OGNL may never touch, comma-separated.",
    ),
    ConfigKey::new(
        "struts.excludedPackageNamePatterns",
        "list",
        "The same as `struts.excludedPackageNames`, as regular expressions — for the package \
         families a literal list cannot keep up with.",
    ),
    ConfigKey::new(
        "struts.additional.excludedPatterns",
        "list",
        "Parameter names the framework refuses to bind, **added** to the built-in list rather \
         than replacing it. The safe way to extend the parameter deny list.",
    ),
    ConfigKey::new(
        "struts.additional.acceptedPatterns",
        "list",
        "Parameter names allowed past the built-in accepted pattern, added to it. Needed by a \
         form whose field names the default pattern rejects — and the thing to check first when \
         one field of a form silently never arrives.",
    ),
    // ── File upload ──────────────────────────────────────────────────────────
    ConfigKey::new(
        "struts.multipart.parser",
        "identifier",
        "Which multipart parser handles file uploads. `jakarta` buffers to disk; \
         `jakarta-stream` streams, which is what a large upload needs.",
    )
    .default_value("jakarta")
    .values(&["jakarta", "jakarta-stream", "pell", "cos"]),
    ConfigKey::new(
        "struts.multipart.maxSize",
        "bytes",
        "The largest multipart request accepted, in bytes. Applies to the **whole** request, not \
         to one file — which is why three small uploads can fail a limit each of them is under.",
    )
    .default_value("2097152"),
    ConfigKey::new(
        "struts.multipart.saveDir",
        "path",
        "Where uploads are buffered. Empty means the servlet container's temporary directory, \
         which on some containers is cleared between requests.",
    ),
    // ── Static content and the UI tags ───────────────────────────────────────
    ConfigKey::new(
        "struts.serve.static",
        "boolean",
        "Serve the framework's own static resources (the tag JavaScript, the theme CSS) from \
         inside the jars. Off means they have to be unpacked into the web application by hand.",
    )
    .default_value("true")
    .values(BOOL),
    ConfigKey::new(
        "struts.serve.static.browserCache",
        "boolean",
        "Send caching headers with those static resources. Turn it off while developing them and \
         on everywhere else.",
    )
    .default_value("true")
    .values(BOOL),
    ConfigKey::new(
        "struts.ui.theme",
        "identifier",
        "The theme the UI tags render with — `simple` emits the bare control, `xhtml` wraps it in \
         a table row with its label and errors. A project may add its own, which is why this is \
         not a closed list.",
    )
    .default_value("xhtml"),
    ConfigKey::new(
        "struts.ui.templateDir",
        "path",
        "Where the tag templates live, relative to the classpath root.",
    )
    .default_value("template"),
    ConfigKey::new(
        "struts.ui.templateSuffix",
        "identifier",
        "Which template engine renders the UI tags: `ftl` for FreeMarker, `vm` for Velocity, \
         `jsp` for JSP.",
    )
    .default_value("ftl")
    .values(&["ftl", "vm", "jsp"]),
    ConfigKey::new(
        "struts.tag.altSyntax",
        "boolean",
        "Read `%{…}` in a tag attribute as an OGNL expression rather than as text. Off makes \
         every expression in every JSP stop evaluating, which is not a change anybody makes twice.",
    )
    .default_value("true")
    .values(BOOL),
    // ── Configuration, i18n and reloading ────────────────────────────────────
    ConfigKey::new(
        "struts.configuration.files",
        "list",
        "The configuration files loaded, in order, comma-separated. Adding a file means adding it \
         **to this list** — a `struts-plugin.xml` in a jar is found on its own, a hand-written \
         `struts-orders.xml` is not.",
    )
    .default_value("struts-default.xml,struts-plugin.xml,struts.xml"),
    ConfigKey::new(
        "struts.configuration.xml.reload",
        "boolean",
        "Re-read the XML configuration when it changes on disk, without a restart. Development \
         only: it stats every configuration file on every request.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "struts.custom.properties",
        "list",
        "Extra `.properties` files, comma-separated and without their extension, loaded as \
         Struts constants — how an application keeps its own settings beside the framework's.",
    ),
    ConfigKey::new(
        "struts.custom.i18n.resources",
        "list",
        "The application's global resource bundles, comma-separated and without their extension \
         or locale suffix. What `getText(\"key\")` falls back to when no bundle nearer the action \
         has the key.",
    ),
    ConfigKey::new(
        "struts.i18n.reload",
        "boolean",
        "Re-read the resource bundles when they change on disk. Development only, for the same \
         reason as the configuration reload.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "struts.locale",
        "locale",
        "The default locale, as a Java locale string (`it_IT`), used where the request names none.",
    ),
    // ── URLs the tags build ──────────────────────────────────────────────────
    ConfigKey::new(
        "struts.url.includeParams",
        "none | get | all",
        "Which of the current request's parameters `<s:url>` carries into the URL it builds. \
         `all` includes POST parameters, which is how a value nobody meant to publish ends up in \
         a link.",
    )
    .default_value("get")
    .values(&["none", "get", "all"]),
    ConfigKey::new("struts.url.http.port", "port", "The port used when a tag has to build an `http` URL.")
        .default_value("80"),
    ConfigKey::new(
        "struts.url.https.port",
        "port",
        "The port used when a tag has to build an `https` URL — the one to set behind a reverse \
         proxy that terminates TLS on a non-standard port.",
    )
    .default_value("443"),
    // ── Wiring ───────────────────────────────────────────────────────────────
    ConfigKey::new(
        "struts.objectFactory",
        "identifier | class name",
        "Who instantiates actions, results and interceptors. `spring` hands that to the Spring \
         container, which is what makes an action's dependencies injectable.",
    ),
    ConfigKey::new(
        "struts.objectFactory.spring.autoWire",
        "identifier",
        "How Spring wires an action's dependencies when the object factory is `spring`.",
    )
    .default_value("name")
    .values(&["name", "type", "auto", "constructor"]),
    ConfigKey::new(
        "struts.objectFactory.spring.autoWire.alwaysRespect",
        "boolean",
        "Apply the autowire mode above even to beans Spring itself created.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "struts.objectFactory.spring.useClassCache",
        "boolean",
        "Cache the class lookups the Spring object factory does.",
    )
    .default_value("true")
    .values(BOOL),
    ConfigKey::new(
        "struts.handle.exception",
        "boolean",
        "Let the framework map an uncaught exception to a result instead of letting it reach the \
         container. Off means the container's error page, which is rarely what an application \
         with its own error result wants.",
    )
    .default_value("true")
    .values(BOOL),
    ConfigKey::new(
        "struts.freemarker.templatesCache",
        "boolean",
        "Cache the FreeMarker templates the UI tags render with.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "struts.freemarker.beanwrapperCache",
        "boolean",
        "Cache FreeMarker's bean wrappers. Faster, at the cost of not seeing a class reloaded \
         under it.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "struts.xslt.nocache",
        "boolean",
        "Re-read the XSLT stylesheets on every request instead of caching them.",
    )
    .default_value("false")
    .values(BOOL),
];

/// Struts 2's vocabulary.
pub const CATALOGUE: Catalogue = Catalogue {
    tool: "struts",
    display_name: "Struts",
    file_name: "struts.properties",
    // `struts2-core` is where the constants are read and where `default.properties` lives. The
    // plugins that add constants of their own all depend on it, so its version dates the file.
    artifacts: &[("org.apache.struts", "struts2-core")],
    min_version: "2.0",
    keys: KEYS,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// The 2.5 security constants are the ones a version gate has to be right about: offering
    /// `strictMethodInvocation` on a 2.3 project would be describing a setting that does nothing.
    #[test]
    fn the_security_constants_are_gated_at_the_release_that_introduced_them() {
        let offered = |v: &str, key: &str| CATALOGUE.known(Some(v)).any(|k| k.key == key);
        assert!(!offered("2.3.34", "struts.strictMethodInvocation"));
        assert!(offered("2.5.30", "struts.strictMethodInvocation"));
        assert!(!offered("2.3.34", "struts.ognl.allowStaticFieldAccess"));
        assert!(offered("2.5.30", "struts.ognl.allowStaticFieldAccess"));
    }

    /// A key with no `since` is offered to every project — including one whose version could not
    /// be resolved at all, which is the common case on a legacy build.
    #[test]
    fn an_undated_key_is_offered_whatever_the_version() {
        assert!(CATALOGUE.known(Some("2.3.16")).any(|k| k.key == "struts.devMode"));
        assert!(CATALOGUE.known(None).any(|k| k.key == "struts.devMode"));
        // And an unresolved version withholds nothing at all.
        assert_eq!(CATALOGUE.known(None).count(), CATALOGUE.keys.len());
    }

    /// A removed key stays in the table: it is sitting in the file being read, and the hover is
    /// the only thing that can say it does nothing now.
    #[test]
    fn a_removed_key_is_still_looked_up_and_says_what_replaced_it() {
        let k = CATALOGUE.lookup("struts.ognl.allowStaticMethodAccess").unwrap();
        assert_eq!(k.deprecated_since, "2.5");
        assert_eq!(k.replacement, "struts.ognl.allowStaticFieldAccess");
    }

    #[test]
    fn every_key_is_a_struts_constant_and_says_something() {
        for k in CATALOGUE.keys {
            assert!(k.key.starts_with("struts."), "{}", k.key);
            assert!(!k.doc.is_empty(), "{} has no documentation", k.key);
            assert!(!k.type_text.is_empty(), "{} has no type", k.key);
        }
    }

    /// No duplicates — two rows for one key means the second is unreachable and the tables drift.
    #[test]
    fn no_key_is_written_twice() {
        let mut seen: Vec<&str> = CATALOGUE.keys.iter().map(|k| k.key).collect();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), before);
    }
}
