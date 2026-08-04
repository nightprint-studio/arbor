//! Beans declared **inside a dependency** — read from bytecode, kept apart from the
//! project's own.
//!
//! A Spring Boot application's interesting beans are not all in its source: a shared
//! platform module or an in-house starter declares half of them, and today the tooling
//! cannot say where a `DataSource` came from because nothing looked in the jar.
//!
//! Two rules make this safe to have, and they are the whole design:
//!
//! **1. Only the artifacts you name.** Scanning every dependency would bury the fifty
//! beans you wrote under the thousands Spring Boot's starters declare. The allowlist
//! ([`LibraryBeanAllowlist`]) is not just a volume control — it changes what gets indexed.
//! Your own modules (`com.acme.*`) declare beans with plain `@Service` / `@Configuration`,
//! which are unconditional and simply true. Boot's auto-configuration is conditional, and
//! only appears here if you deliberately ask for it.
//!
//! **2. A library bean is a *declaration*, never a fact.** `@ConditionalOnMissingBean`,
//! `@ConditionalOnClass` and `@ConditionalOnProperty` mean the class file records what
//! Spring *might* register, and deciding what it actually registers is Spring's own
//! condition evaluator — `@ConditionalOnMissingBean` depends on the entire bean set and on
//! registration order, so nothing short of running it gives a true answer. So these are
//! carried in their own tier, marked with the conditions that gate them, and they do not
//! take part in injection-candidate matching or in any diagnostic. `known.rs` states the
//! house rule this protects: a bean that does not exist, navigated to and counted in a
//! panel, is a confident lie — and here the opportunity to tell it is thousands of times
//! larger.

use bennu_classpath::prelude::{Annotation, AnnotationValue, ClassAnnotations};

// ── Which artifacts are read ───────────────────────────────────────────────────

/// Which dependencies contribute beans. Empty (the default) means none: this reads a
/// project's third-party code, and doing that has to be asked for.
///
/// Four axes because a coordinate is matched four ways in practice — a single artifact
/// (`shared-security`), a whole group (`com.acme.platform`), everything an organisation
/// publishes (`com.acme.`), or a naming convention (`acme-starter-`). Any match admits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibraryBeanAllowlist {
    /// Exact group ids (`com.acme.platform`).
    pub group_id: Vec<String>,
    /// Group-id prefixes (`com.acme.` admits `com.acme.platform` and `com.acme.web`).
    pub group_id_prefix: Vec<String>,
    /// Exact artifact ids (`shared-security`).
    pub artifact_id: Vec<String>,
    /// Artifact-id prefixes (`acme-starter-`).
    pub artifact_id_prefix: Vec<String>,
}

impl LibraryBeanAllowlist {
    /// Whether nothing is allowed — the default, and the state in which no jar is opened
    /// and the whole feature costs nothing.
    pub fn is_empty(&self) -> bool {
        self.group_id.is_empty()
            && self.group_id_prefix.is_empty()
            && self.artifact_id.is_empty()
            && self.artifact_id_prefix.is_empty()
    }

    /// Whether this coordinate's beans are read.
    ///
    /// A prefix entry is a prefix, not a fuzzy match: `com.acme` admits `com.acmegroup`
    /// too, which is why the setting's own examples end in a dot. Left as written rather
    /// than silently normalized — a rule that quietly means something other than what it
    /// says is worse than one that needs a dot.
    pub fn admits(&self, group_id: &str, artifact_id: &str) -> bool {
        self.group_id.iter().any(|g| g == group_id)
            || self.artifact_id.iter().any(|a| a == artifact_id)
            || self.group_id_prefix.iter().any(|p| !p.is_empty() && group_id.starts_with(p))
            || self.artifact_id_prefix.iter().any(|p| !p.is_empty() && artifact_id.starts_with(p))
    }
}

// ── What comes out ─────────────────────────────────────────────────────────────

/// A bean declared in a dependency. Deliberately its own type and not a `BeanDef`: the
/// project's model is a statement about what the application has, and merging these into
/// it is exactly the mistake this feature has to avoid. It carries no file/offset either —
/// there is no source to point at unless a `-sources.jar` was downloaded, and go-to routes
/// through the library source view by binary name instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryBean {
    /// The name Spring would register it under — the explicit one, else the convention
    /// (decapitalized simple class name, or the `@Bean` factory method's name).
    pub name: String,
    /// Dotted FQCN of the implementation.
    pub fqcn: String,
    /// What was written (`@Service`, `@Bean`, `@Configuration`) — the badge.
    pub stereotype: String,
    /// The declaring class, dotted — for a `@Bean` method this is the configuration class,
    /// which is what you actually want to open.
    pub declared_in: String,
    /// The `@ConditionalOn…` annotations gating it, as written. **Non-empty means this
    /// bean may well not exist in your application**, and the UI must say so.
    pub conditions: Vec<String>,
    /// Whether it is `@Primary`.
    pub primary: bool,
}

/// The beans one dependency contributes, so the panel can group by where they came from
/// rather than presenting one undifferentiated list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryBeanGroup {
    pub group_id: String,
    pub artifact_id: String,
    pub version: String,
    pub beans: Vec<LibraryBean>,
}

impl LibraryBeanGroup {
    /// `com.acme:shared-security:2.1.0` — the coordinate as anyone writes it.
    pub fn coordinate(&self) -> String {
        format!("{}:{}:{}", self.group_id, self.artifact_id, self.version)
    }
}

// ── The annotations that mean something ────────────────────────────────────────

const STEREOTYPES: &[(&str, &str)] = &[
    ("org.springframework.stereotype.Service", "@Service"),
    ("org.springframework.stereotype.Component", "@Component"),
    ("org.springframework.stereotype.Repository", "@Repository"),
    ("org.springframework.stereotype.Controller", "@Controller"),
    ("org.springframework.web.bind.annotation.RestController", "@RestController"),
    ("org.springframework.context.annotation.Configuration", "@Configuration"),
];

const BEAN_METHOD: &str = "org.springframework.context.annotation.Bean";
const PRIMARY: &str = "org.springframework.context.annotation.Primary";

/// Matched by simple-name prefix rather than by an exhaustive list: Boot ships dozens of
/// `@ConditionalOn…` annotations and third parties add their own, and the useful thing to
/// report is "this is gated", which the prefix establishes. The FQN is still required to
/// start with a Spring Boot package, so a `com.acme.ConditionalOnFriday` is not read as
/// one of Boot's.
const CONDITION_PACKAGE: &str = "org.springframework.boot.autoconfigure.condition.";

fn condition_summary(a: &Annotation) -> Option<String> {
    if !a.type_name.starts_with(CONDITION_PACKAGE) {
        return None;
    }
    let simple = a.simple_name();
    // Render the argument when there is one — `@ConditionalOnProperty(app.audit.enabled)`
    // says far more than `@ConditionalOnProperty`, and it is what you would check first.
    let arg = a
        .element("name")
        .or_else(|| a.element("value"))
        .or_else(|| a.element("prefix"))
        .map(|v| v.texts().join(", "))
        .filter(|s| !s.is_empty());
    Some(match arg {
        Some(text) => format!("@{simple}({text})"),
        None => format!("@{simple}"),
    })
}

fn conditions_of(annotations: &[Annotation]) -> Vec<String> {
    annotations.iter().filter_map(condition_summary).collect()
}

fn stereotype_of(annotations: &[Annotation]) -> Option<&'static str> {
    annotations.iter().find_map(|a| {
        STEREOTYPES.iter().find(|(fqn, _)| *fqn == a.type_name).map(|(_, badge)| *badge)
    })
}

fn is_primary(annotations: &[Annotation]) -> bool {
    annotations.iter().any(|a| a.type_name == PRIMARY)
}

/// The name Spring registers a stereotype-annotated class under: the annotation's `value`
/// when written, else the decapitalized simple class name.
fn component_bean_name(fqcn: &str, annotations: &[Annotation]) -> String {
    let explicit = annotations
        .iter()
        .find(|a| STEREOTYPES.iter().any(|(fqn, _)| *fqn == a.type_name))
        .and_then(|a| a.value())
        .and_then(AnnotationValue::as_text)
        .filter(|s| !s.is_empty());
    match explicit {
        Some(name) => name.to_string(),
        None => decapitalize(simple_name(fqcn)),
    }
}

/// The name a `@Bean` factory method registers under: the annotation's `name`/`value` when
/// written, else the method's own name.
fn bean_method_name(method: &str, bean: &Annotation) -> String {
    bean.element("name")
        .or_else(|| bean.value())
        .and_then(AnnotationValue::as_text)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| method.to_string())
}

fn simple_name(fqcn: &str) -> &str {
    fqcn.rsplit('.').next().unwrap_or(fqcn)
}

/// Spring's own convention (`java.beans.Introspector.decapitalize`): lowercase the first
/// letter, **unless** the first two are both upper case — `URLResolver` stays `URLResolver`
/// and does not become `uRLResolver`.
fn decapitalize(name: &str) -> String {
    let mut chars = name.chars();
    let Some(first) = chars.next() else { return String::new() };
    let second_is_upper = name.chars().nth(1).is_some_and(char::is_uppercase);
    if !first.is_uppercase() || second_is_upper {
        return name.to_string();
    }
    first.to_lowercase().collect::<String>() + chars.as_str()
}

/// The declaring class's return type for a `@Bean` method, from its erased descriptor —
/// `()Ljavax/sql/DataSource;` → `javax.sql.DataSource`. The bean's type is what it returns,
/// which is the whole point of a factory method. `None` for a `void` or primitive return,
/// which is not a bean whatever the annotation says.
fn return_fqcn(descriptor: &str) -> Option<String> {
    let ret = descriptor.rsplit(')').next()?;
    let inner = ret.trim_start_matches('[').strip_prefix('L')?.strip_suffix(';')?;
    Some(inner.replace('/', "."))
}

// ── Extraction ─────────────────────────────────────────────────────────────────

/// The beans one class declares: itself when it carries a stereotype, plus one per `@Bean`
/// factory method. A class with neither yields nothing, which is almost all of them.
///
/// Conditions are inherited: a `@Bean` method inside a `@ConditionalOnClass` configuration
/// is gated by that condition too, and reporting the method's own gates alone would
/// describe it as more certain than it is.
pub fn beans_of_class(annotations: &ClassAnnotations) -> Vec<LibraryBean> {
    let fqcn = annotations.binary_name.replace('/', ".");
    let class_conditions = conditions_of(&annotations.class);
    let mut out = Vec::new();

    if let Some(stereotype) = stereotype_of(&annotations.class) {
        out.push(LibraryBean {
            name: component_bean_name(&fqcn, &annotations.class),
            fqcn: fqcn.clone(),
            stereotype: stereotype.to_string(),
            declared_in: fqcn.clone(),
            conditions: class_conditions.clone(),
            primary: is_primary(&annotations.class),
        });
    }

    for method in &annotations.methods {
        let Some(bean) = method.annotations.iter().find(|a| a.type_name == BEAN_METHOD) else {
            continue;
        };
        let Some(returns) = return_fqcn(&method.descriptor) else { continue };
        let mut conditions = class_conditions.clone();
        conditions.extend(conditions_of(&method.annotations));
        out.push(LibraryBean {
            name: bean_method_name(&method.name, bean),
            fqcn: returns,
            stereotype: "@Bean".to_string(),
            declared_in: fqcn.clone(),
            conditions,
            primary: is_primary(&method.annotations),
        });
    }

    out
}

/// Every bean the given classes declare, ordered by name so the panel is stable across
/// rebuilds (a jar enumerates in whatever order it was written).
pub fn beans_of_classes<'a, I>(classes: I) -> Vec<LibraryBean>
where
    I: IntoIterator<Item = &'a ClassAnnotations>,
{
    let mut beans: Vec<LibraryBean> = classes.into_iter().flat_map(beans_of_class).collect();
    beans.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.fqcn.cmp(&b.fqcn)));
    beans
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_classpath::prelude::MemberAnnotations;

    fn ann(type_name: &str) -> Annotation {
        Annotation { type_name: type_name.to_string(), elements: vec![] }
    }

    fn ann_with(type_name: &str, element: &str, value: &str) -> Annotation {
        Annotation {
            type_name: type_name.to_string(),
            elements: vec![(element.to_string(), AnnotationValue::Text(value.to_string()))],
        }
    }

    fn class(binary: &str, class_anns: Vec<Annotation>) -> ClassAnnotations {
        ClassAnnotations {
            binary_name: binary.to_string(),
            class: class_anns,
            methods: vec![],
            fields: vec![],
        }
    }

    // ── The allowlist ──────────────────────────────────────────────────────────

    #[test]
    fn nothing_is_allowed_by_default() {
        let list = LibraryBeanAllowlist::default();
        assert!(list.is_empty());
        assert!(!list.admits("com.acme", "shared"));
    }

    #[test]
    fn each_axis_admits_on_its_own() {
        let by_group = LibraryBeanAllowlist {
            group_id: vec!["com.acme.platform".into()],
            ..Default::default()
        };
        assert!(by_group.admits("com.acme.platform", "anything"));
        assert!(!by_group.admits("com.acme.platformx", "anything"));

        let by_prefix = LibraryBeanAllowlist {
            group_id_prefix: vec!["com.acme.".into()],
            ..Default::default()
        };
        assert!(by_prefix.admits("com.acme.web", "anything"));
        assert!(!by_prefix.admits("com.other", "anything"));

        let by_artifact = LibraryBeanAllowlist {
            artifact_id: vec!["shared-security".into()],
            ..Default::default()
        };
        assert!(by_artifact.admits("any.group", "shared-security"));

        let by_artifact_prefix = LibraryBeanAllowlist {
            artifact_id_prefix: vec!["acme-starter-".into()],
            ..Default::default()
        };
        assert!(by_artifact_prefix.admits("any.group", "acme-starter-audit"));
        assert!(!by_artifact_prefix.admits("any.group", "other-starter"));
    }

    /// An empty prefix would admit everything — the state a half-edited setting is in, and
    /// silently reading every jar because of a stray blank line is not a reasonable
    /// reading of "I have not finished typing".
    #[test]
    fn an_empty_prefix_admits_nothing() {
        let list = LibraryBeanAllowlist {
            group_id_prefix: vec![String::new()],
            artifact_id_prefix: vec![String::new()],
            ..Default::default()
        };
        assert!(!list.admits("com.acme", "shared"));
    }

    // ── Bean names ─────────────────────────────────────────────────────────────

    #[test]
    fn a_stereotype_class_is_a_bean_named_by_convention() {
        let beans =
            beans_of_class(&class("com/acme/AuditService", vec![ann(STEREOTYPES[0].0)]));
        assert_eq!(beans.len(), 1);
        assert_eq!(beans[0].name, "auditService");
        assert_eq!(beans[0].fqcn, "com.acme.AuditService");
        assert_eq!(beans[0].stereotype, "@Service");
        assert!(beans[0].conditions.is_empty());
    }

    #[test]
    fn an_explicit_name_wins_over_the_convention() {
        let beans = beans_of_class(&class(
            "com/acme/AuditService",
            vec![ann_with(STEREOTYPES[0].0, "value", "audit")],
        ));
        assert_eq!(beans[0].name, "audit");
    }

    /// Spring's `Introspector.decapitalize`: two leading capitals are left alone, so a
    /// `URLResolver` is `URLResolver` and not `uRLResolver`.
    #[test]
    fn decapitalization_follows_springs_convention() {
        assert_eq!(decapitalize("AuditService"), "auditService");
        assert_eq!(decapitalize("URLResolver"), "URLResolver");
        assert_eq!(decapitalize("A"), "a");
        assert_eq!(decapitalize(""), "");
    }

    #[test]
    fn a_class_with_no_spring_annotation_declares_nothing() {
        assert!(beans_of_class(&class("com/acme/Plain", vec![ann("java.lang.Deprecated")]))
            .is_empty());
    }

    // ── @Bean factory methods ──────────────────────────────────────────────────

    #[test]
    fn a_bean_method_is_named_after_the_method_and_typed_by_its_return() {
        let mut c = class("com/acme/AuditConfig", vec![ann(STEREOTYPES[5].0)]);
        c.methods.push(MemberAnnotations {
            name: "auditDataSource".into(),
            descriptor: "()Ljavax/sql/DataSource;".into(),
            annotations: vec![ann(BEAN_METHOD)],
        });
        let beans = beans_of_class(&c);
        // The configuration class itself is a bean, plus the factory method's.
        let bean = beans.iter().find(|b| b.stereotype == "@Bean").expect("the @Bean method");
        assert_eq!(bean.name, "auditDataSource");
        assert_eq!(bean.fqcn, "javax.sql.DataSource");
        assert_eq!(bean.declared_in, "com.acme.AuditConfig");
    }

    #[test]
    fn a_void_bean_method_is_not_a_bean() {
        let mut c = class("com/acme/AuditConfig", vec![]);
        c.methods.push(MemberAnnotations {
            name: "nothing".into(),
            descriptor: "()V".into(),
            annotations: vec![ann(BEAN_METHOD)],
        });
        assert!(beans_of_class(&c).is_empty());
    }

    // ── Conditions ─────────────────────────────────────────────────────────────

    /// The reason this whole tier is kept separate: a gated bean must arrive carrying the
    /// gate, or it reads as something the application definitely has.
    #[test]
    fn conditions_are_reported_with_their_argument() {
        let beans = beans_of_class(&class(
            "com/acme/AuditService",
            vec![
                ann(STEREOTYPES[0].0),
                ann_with(
                    "org.springframework.boot.autoconfigure.condition.ConditionalOnProperty",
                    "name",
                    "app.audit.enabled",
                ),
            ],
        ));
        assert_eq!(beans[0].conditions, vec!["@ConditionalOnProperty(app.audit.enabled)"]);
    }

    /// A `@Bean` inside a gated configuration is gated too — reporting only the method's
    /// own conditions would describe it as more certain than it is.
    #[test]
    fn a_bean_method_inherits_its_configurations_conditions() {
        let mut c = class(
            "com/acme/AuditConfig",
            vec![
                ann(STEREOTYPES[5].0),
                ann("org.springframework.boot.autoconfigure.condition.ConditionalOnClass"),
            ],
        );
        c.methods.push(MemberAnnotations {
            name: "auditDataSource".into(),
            descriptor: "()Ljavax/sql/DataSource;".into(),
            annotations: vec![
                ann(BEAN_METHOD),
                ann("org.springframework.boot.autoconfigure.condition.ConditionalOnMissingBean"),
            ],
        });
        let bean = beans_of_class(&c)
            .into_iter()
            .find(|b| b.stereotype == "@Bean")
            .expect("the @Bean method");
        assert_eq!(
            bean.conditions,
            vec!["@ConditionalOnClass", "@ConditionalOnMissingBean"],
            "the configuration's gate and the method's own, in that order"
        );
    }

    /// `@Service` is not a reserved word and neither is `ConditionalOnFriday`: an
    /// annotation from somebody else's package is not one of Boot's.
    #[test]
    fn a_lookalike_condition_from_another_package_is_not_read_as_one() {
        let beans = beans_of_class(&class(
            "com/acme/AuditService",
            vec![ann(STEREOTYPES[0].0), ann("com.acme.ConditionalOnFriday")],
        ));
        assert!(beans[0].conditions.is_empty());
    }

    #[test]
    fn beans_come_out_ordered_so_the_panel_is_stable() {
        let classes = vec![
            class("com/acme/Zeta", vec![ann(STEREOTYPES[0].0)]),
            class("com/acme/Alpha", vec![ann(STEREOTYPES[0].0)]),
        ];
        let beans = beans_of_classes(classes.iter());
        assert_eq!(beans.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), ["alpha", "zeta"]);
    }
}
