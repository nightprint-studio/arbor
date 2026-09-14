//! Beans: the classes a container instantiates and the producers it calls.
//!
//! Two tiers, and the distinction carries the whole conservative design:
//!
//! - **Explicit** — a class with a bean-defining annotation (a scope, `@Model`, an EJB kind, a
//!   decorator or interceptor, a project stereotype), or a producer declared inside one. A bean in
//!   every archive mode; the only kind ever *counted* towards an ambiguity.
//! - **Possible** — a concrete class with a suitable constructor and no such annotation, or a
//!   producer in a class that is not itself a bean. A bean only in an `all` archive (or not at all),
//!   so it is enough to *silence* an "unsatisfied" report and never enough to raise an "ambiguous" one.

use std::collections::HashSet;

use bennu_facts::prelude::{AnnFacts, TypeFacts};

use crate::known;
use crate::qualifiers::{producer_default_name, qualifiers_of, Qualifiers};
use crate::text::{decapitalize, erase, line_of, simple_name};
use crate::types::{resolve, TypeRef, TypeRow, TypeView, Unit};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeanOrigin {
    Class,
    ProducerMethod,
    ProducerField,
}

/// One bean.
#[derive(Debug, Clone)]
pub struct Bean {
    /// `com.acme.OrderService`, or `com.acme.Resources#entityManager` for a producer.
    pub id: String,
    /// The class declaring it.
    pub owner: String,
    /// The producer's member name.
    pub member: Option<String>,
    pub origin: BeanOrigin,
    /// The badge a panel shows — a scope, an EJB kind, `@Produces`; empty for a possible class bean.
    pub badge: &'static str,
    /// A bean in every archive mode. See the module docs.
    pub explicit: bool,
    /// Not explicit, but carrying an annotation that may be bean-defining — a scope this crate does
    /// not catalogue, or one nobody can classify. Enough to silence, never enough to count.
    pub maybe_explicit: bool,
    /// The class itself, or the producer's declared type.
    pub declared: TypeRef,
    pub declared_text: String,
    /// A producer type with type arguments or array dimensions.
    pub parameterized: bool,
    pub quals: Qualifiers,
    pub alternative: bool,
    pub priority: bool,
    pub specializes: bool,
    pub typed: bool,
    pub normal_scoped: bool,
    /// A session or message-driven bean — whose bean types are its business interfaces, not its
    /// whole hierarchy.
    pub ejb: bool,
    /// Decorators, interceptors and message-driven beans are beans nobody can inject.
    pub injectable: bool,
    pub file: String,
    pub offset: usize,
    pub line: u32,
}

impl Bean {
    /// `OrderService`, `Resources.entityManager()`, `Resources.clock`.
    pub fn label(&self) -> String {
        let owner = simple_name(&self.owner);
        match (self.origin, &self.member) {
            (BeanOrigin::ProducerMethod, Some(m)) => format!("{owner}.{m}()"),
            (BeanOrigin::ProducerField, Some(m)) => format!("{owner}.{m}"),
            _ => owner.to_string(),
        }
    }

    /// `com.acme.OrderService · @ApplicationScoped`.
    pub fn detail(&self) -> String {
        match self.badge {
            "" => self.owner.clone(),
            badge => format!("{} · {badge}", self.owner),
        }
    }
}

/// Every bean a unit declares. `vetoed_packages` are the packages a `package-info.java` vetoes.
pub fn beans_of(unit: &Unit, view: &TypeView<'_>, vetoed_packages: &HashSet<String>) -> Vec<Bean> {
    let facts = &unit.facts;
    let mut out = Vec::new();
    for t in &facts.types {
        let Some(row) = view.get(&t.fqcn) else { continue };
        if known::has(&t.annotations, facts, "Vetoed") || vetoed_packages.contains(&facts.package) {
            continue;
        }
        let class = class_bean(t, row, unit, view);
        let explicit = class.as_ref().is_some_and(|b| b.explicit);
        let alternative = class.as_ref().is_some_and(|b| b.alternative);
        out.extend(class);
        out.extend(producers(t, unit, view, explicit, alternative));
    }
    out
}

fn class_bean(t: &TypeFacts, row: &TypeRow, unit: &Unit, view: &TypeView<'_>) -> Option<Bean> {
    let facts = &unit.facts;
    let anns = &t.annotations;
    let decorator = known::has(anns, facts, "Decorator");
    if !row.is_class() || !row.bean_capable || (row.is_abstract && !decorator) {
        return None;
    }
    let default_name = decapitalize(&t.name);
    let (mut quals, stereotypes) = qualifiers_of(anns, facts, view, Some(default_name.as_str()));
    let ejb = known::ejb_kind(anns, facts);
    let model = known::has(anns, facts, "Model");
    let interceptor = known::has(anns, facts, "Interceptor");
    let stereotype_scope = stereotypes.iter().find_map(|s| s.scope);
    let scope = known::scope_of(anns, facts).or(model.then_some("RequestScoped")).or(stereotype_scope);
    let explicit = ejb.is_some() || scope.is_some() || model || decorator || interceptor || !stereotypes.is_empty();
    if !explicit && !has_bean_constructor(row) {
        return None;
    }
    // A scope this crate does not catalogue (`@ViewScoped`, `@TransactionScoped`, a library's own)
    // or an annotation nobody can classify may well be bean-defining.
    let maybe_explicit = !explicit
        && (quals.unclassified
            || anns.iter().any(|a| a.name.ends_with("Scoped") && known::scope_of(std::slice::from_ref(a), facts).is_none()));
    if model && quals.named.is_none() {
        quals.named = Some(default_name);
    }
    let pseudo_singleton = anns.iter().any(|a| known::is_inject_singleton(a, facts));
    Some(Bean {
        id: t.fqcn.clone(),
        owner: t.fqcn.clone(),
        member: None,
        origin: BeanOrigin::Class,
        badge: class_badge(ejb, scope, pseudo_singleton, explicit),
        explicit,
        maybe_explicit,
        declared: TypeRef::Project(t.fqcn.clone()),
        declared_text: t.name.clone(),
        parameterized: false,
        quals,
        alternative: known::has(anns, facts, "Alternative") || stereotypes.iter().any(|s| s.alternative),
        priority: known::has(anns, facts, "Priority"),
        specializes: known::has(anns, facts, "Specializes"),
        typed: known::has(anns, facts, "Typed"),
        normal_scoped: scope.is_some_and(|s| known::NORMAL_SCOPES.contains(&s)),
        ejb: ejb.is_some(),
        injectable: !decorator && !interceptor && ejb != Some("MessageDriven"),
        file: facts.file.clone(),
        offset: t.name_offset,
        line: line_of(&unit.text, t.name_offset),
    })
}

/// A constructor the container can call: none declared, a no-argument one, or one marked `@Inject`.
pub fn has_bean_constructor(row: &TypeRow) -> bool {
    row.ctors.is_empty() || row.ctors.iter().any(|c| c.params == 0 || c.inject)
}

fn class_badge(ejb: Option<&str>, scope: Option<&str>, pseudo_singleton: bool, explicit: bool) -> &'static str {
    match (ejb, scope) {
        (Some("Stateless"), _) => "@Stateless",
        (Some("Stateful"), _) => "@Stateful",
        (Some("Singleton"), _) => "@Singleton",
        (Some(_), _) => "@MessageDriven",
        (None, Some("ApplicationScoped")) => "@ApplicationScoped",
        (None, Some("RequestScoped")) => "@RequestScoped",
        (None, Some("SessionScoped")) => "@SessionScoped",
        (None, Some("ConversationScoped")) => "@ConversationScoped",
        (None, Some(_)) => "@Dependent",
        (None, None) if pseudo_singleton => "@Singleton",
        (None, None) if explicit => "@Dependent",
        (None, None) => "",
    }
}

/// A producer member, whichever kind.
struct Member<'a> {
    name: &'a str,
    type_text: &'a str,
    anns: &'a [AnnFacts],
    offset: usize,
    origin: BeanOrigin,
}

fn producers(t: &TypeFacts, unit: &Unit, view: &TypeView<'_>, in_bean: bool, in_alternative: bool) -> Vec<Bean> {
    let facts = &unit.facts;
    let methods = t
        .methods
        .iter()
        .filter(|m| !m.is_constructor && known::has(&m.annotations, facts, "Produces"))
        .map(|m| Member {
            name: &m.name,
            type_text: &m.return_type,
            anns: &m.annotations,
            offset: m.name_offset,
            origin: BeanOrigin::ProducerMethod,
        });
    let fields = t.fields.iter().filter(|f| known::has(&f.annotations, facts, "Produces")).map(|f| Member {
        name: &f.name,
        type_text: &f.type_text,
        anns: &f.annotations,
        offset: f.name_offset,
        origin: BeanOrigin::ProducerField,
    });
    methods.chain(fields).map(|m| producer(t, unit, view, m, in_bean, in_alternative)).collect()
}

fn producer(t: &TypeFacts, unit: &Unit, view: &TypeView<'_>, m: Member<'_>, in_bean: bool, in_alternative: bool) -> Bean {
    let facts = &unit.facts;
    let erased = erase(m.type_text);
    let default_name = match m.origin {
        BeanOrigin::ProducerMethod => producer_default_name(m.name),
        _ => m.name.to_string(),
    };
    let (quals, stereotypes) = qualifiers_of(m.anns, facts, view, Some(default_name.as_str()));
    let scope = known::scope_of(m.anns, facts).or(stereotypes.iter().find_map(|s| s.scope));
    Bean {
        id: format!("{}#{}", t.fqcn, m.name),
        owner: t.fqcn.clone(),
        member: Some(m.name.to_string()),
        origin: m.origin,
        badge: "@Produces",
        explicit: in_bean,
        maybe_explicit: false,
        declared: resolve(m.type_text, facts, &view.names()),
        declared_text: m.type_text.to_string(),
        parameterized: erased.parameterized || erased.array,
        quals,
        alternative: in_alternative
            || known::has(m.anns, facts, "Alternative")
            || stereotypes.iter().any(|s| s.alternative),
        priority: known::has(m.anns, facts, "Priority"),
        specializes: known::has(m.anns, facts, "Specializes"),
        typed: known::has(m.anns, facts, "Typed"),
        normal_scoped: scope.is_some_and(|s| known::NORMAL_SCOPES.contains(&s)),
        ejb: false,
        injectable: true,
        file: facts.file.clone(),
        offset: m.offset,
        line: line_of(&unit.text, m.offset),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TypeTable;

    fn beans(project: &[(&str, &str)]) -> Vec<Bean> {
        let units: Vec<Unit> = project.iter().map(|(p, s)| Unit::new(p, s).unwrap()).collect();
        let table = TypeTable::build(&units, Vec::<String>::new());
        let view = TypeView::of(&table);
        units.iter().flat_map(|u| beans_of(u, &view, &HashSet::new())).collect()
    }

    fn find<'a>(beans: &'a [Bean], id: &str) -> &'a Bean {
        beans.iter().find(|b| b.id == id).unwrap_or_else(|| panic!("no bean {id}"))
    }

    #[test]
    fn a_scoped_class_is_explicit_and_a_plain_one_only_possible() {
        let b = beans(&[(
            "/p/a/S.java",
            "package a;\nimport jakarta.enterprise.context.ApplicationScoped;\n@ApplicationScoped public class S {}\nclass Plain {}\nclass NoCtor { NoCtor(int x) {} }\n",
        )]);
        let s = find(&b, "a.S");
        assert!(s.explicit && s.normal_scoped);
        assert_eq!(s.badge, "@ApplicationScoped");
        let plain = find(&b, "a.Plain");
        assert!(!plain.explicit);
        assert_eq!(plain.badge, "");
        assert!(b.iter().all(|x| x.id != "a.NoCtor"), "no constructor the container can call");
    }

    #[test]
    fn interfaces_abstract_and_inner_classes_are_not_beans() {
        let b = beans(&[(
            "/p/a/S.java",
            "package a;\nimport jakarta.enterprise.context.*;\n@ApplicationScoped interface I {}\n@ApplicationScoped abstract class A {}\nclass O { @RequestScoped class Inner {} }\n",
        )]);
        assert_eq!(b.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["a.O"]);
    }

    #[test]
    fn the_ejb_singleton_is_a_session_bean_and_the_inject_one_is_not() {
        let b = beans(&[
            ("/p/a/E.java", "package a;\nimport javax.ejb.Singleton;\n@Singleton public class E {}\n"),
            ("/p/a/P.java", "package a;\nimport javax.inject.Singleton;\n@Singleton public class P {}\n"),
        ]);
        let e = find(&b, "a.E");
        assert!(e.ejb && e.explicit);
        let p = find(&b, "a.P");
        assert!(!p.ejb && !p.explicit, "not bean-defining in an implicit archive");
        assert_eq!(p.badge, "@Singleton");
    }

    #[test]
    fn a_cdi_producer_is_a_bean_and_a_jax_rs_produces_is_not() {
        let b = beans(&[(
            "/p/a/R.java",
            "package a;\nimport jakarta.enterprise.context.ApplicationScoped;\nimport jakarta.enterprise.inject.Produces;\n@ApplicationScoped public class R {\n  @Produces Clock getClock() { return null; }\n  @Produces Clock fallback;\n}\n",
        ), (
            "/p/a/W.java",
            "package a;\nimport javax.ws.rs.Produces;\npublic class W { @Produces(\"text/plain\") public String hello() { return \"\"; } }\n",
        )]);
        let getter = find(&b, "a.R#getClock");
        assert!(getter.explicit);
        assert_eq!(getter.quals.named, None, "no @Named, no name");
        assert_eq!(getter.label(), "R.getClock()");
        assert!(b.iter().any(|x| x.id == "a.R#fallback"));
        assert!(b.iter().all(|x| x.id != "a.W#hello"));
    }

    #[test]
    fn a_project_stereotype_defines_the_bean_its_scope_and_its_name() {
        let b = beans(&[
            (
                "/p/a/Action.java",
                "package a;\nimport jakarta.enterprise.inject.Stereotype;\nimport jakarta.enterprise.context.RequestScoped;\nimport jakarta.inject.Named;\n@Stereotype @RequestScoped @Named public @interface Action {}\n",
            ),
            ("/p/a/Login.java", "package a;\n@Action public class Login {}\n"),
        ]);
        let login = find(&b, "a.Login");
        assert!(login.explicit && login.normal_scoped);
        assert_eq!(login.quals.named.as_deref(), Some("login"));
    }

    #[test]
    fn a_vetoed_class_declares_nothing() {
        let b = beans(&[(
            "/p/a/V.java",
            "package a;\nimport jakarta.enterprise.inject.*;\nimport jakarta.enterprise.context.*;\n@Vetoed @ApplicationScoped public class V { @Produces String s; }\n",
        )]);
        assert!(b.is_empty());
    }
}
