//! `bennu_beans` — the DI container as data: Spring definitions from the typed model, CDI beans from
//! their catalog rows.

use std::collections::BTreeMap;

use bennu_ext::prelude::ExtEntry;
use bennu_spring::prelude::{simple_name, BeanDef, SpringModel};
use serde::Serialize;

use super::has;

/// `injections` is only honoured up to this many beans: working out who injects whom costs a pass over
/// every injection point, and a thousand beans with their wiring is not an answer anyone reads.
const MAX_WITH_INJECTIONS: usize = 100;
/// Cap on the dependents listed under one bean.
const MAX_DEPENDENTS: usize = 20;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct InjectionRow {
    pub member: String,
    #[serde(rename = "type")]
    pub type_name: String,
    /// `field` | `constructor` | `setter`.
    pub kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub qualifier: String,
    /// Names of the beans that can satisfy it, `@Primary` first.
    pub resolves_to: Vec<String>,
    /// Why `resolves_to` is empty, when it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unresolved: Option<&'static str>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct BeanRow {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    /// `stereotype` | `factory` | `xml` | `cdi`.
    pub kind: String,
    /// What was written: `@Service`, `@Bean`, `<bean>`…
    pub declared_as: String,
    pub scope: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub primary: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub lazy: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub profile: String,
    /// `@ConditionalOn…` readings: reasons the bean might not exist.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub supertypes: Vec<String>,
    /// `groupId:artifactId` of the dependency that declares it; absent for the project's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// What the bean asks for. Present only when `injections` was requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub injects: Option<Vec<InjectionRow>>,
    /// The classes that inject it. Present only when `injections` was requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub injected_into: Option<Vec<String>>,
}

#[derive(Debug, Default)]
pub struct BeanFilter {
    pub name: Option<String>,
    pub type_name: Option<String>,
    pub stereotype: Option<String>,
    pub scope: Option<String>,
    pub profile: Option<String>,
    pub include_libraries: bool,
    pub injections: bool,
}

impl BeanFilter {
    fn keeps(&self, b: &BeanDef) -> bool {
        let stereotype_ok = self
            .stereotype
            .as_deref()
            .filter(|s| !s.is_empty())
            .is_none_or(|s| b.stereotype.trim_start_matches('@').eq_ignore_ascii_case(s.trim_start_matches('@')));
        let scope_ok = self.scope.as_deref().filter(|s| !s.is_empty()).is_none_or(|s| scope_of(b).eq_ignore_ascii_case(s));
        let type_ok = has(&b.fqcn, &self.type_name) || b.supertypes.iter().any(|s| has(s, &self.type_name));
        stereotype_ok && scope_ok && type_ok && has(&b.name, &self.name) && has(&b.profile, &self.profile)
    }
}

/// An unwritten scope is a singleton.
fn scope_of(b: &BeanDef) -> &str {
    if b.scope.is_empty() { "singleton" } else { b.scope.trim_matches('"') }
}

fn row_of(b: &BeanDef) -> BeanRow {
    BeanRow {
        name: b.name.clone(),
        type_name: b.fqcn.clone(),
        kind: b.kind.as_str().to_string(),
        declared_as: b.stereotype.clone(),
        scope: scope_of(b).to_string(),
        primary: b.primary,
        lazy: b.lazy,
        profile: b.profile.clone(),
        conditions: b.conditions.iter().map(|c| c.summary.clone()).collect(),
        supertypes: b.supertypes.clone(),
        library: (!b.artifact.is_empty()).then(|| b.artifact.clone()),
        file: Some(b.file.clone()).filter(|f| !f.is_empty()),
        line: Some(b.line).filter(|l| *l > 0),
        injects: None,
        injected_into: None,
    }
}

/// Fill in `injects` and `injected_into` for `rows`.
fn add_wiring(model: &SpringModel, rows: &mut [BeanRow]) {
    // Who injects each bean name: one pass over the injection points, each resolved once.
    let mut dependents: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for point in &model.injections {
        for bean in model.candidates(&point.type_text, &point.qualifier) {
            let owners = dependents.entry(bean.name.as_str()).or_default();
            let owner = simple_name(&point.owner_fqcn).to_string();
            if !owners.contains(&owner) {
                owners.push(owner);
            }
        }
    }
    for row in rows.iter_mut() {
        // A factory bean's class is its return type, not the configuration class that holds its
        // injection points, so only the beans that ARE their declaring class own points.
        let own: Vec<InjectionRow> = if row.kind == "factory" {
            Vec::new()
        } else {
            model.injections.iter().filter(|p| p.owner_fqcn == row.type_name).map(|p| injection_row(model, p)).collect()
        };
        row.injects = Some(own);
        let mut by = dependents.get(row.name.as_str()).cloned().unwrap_or_default();
        by.truncate(MAX_DEPENDENTS);
        row.injected_into = Some(by);
    }
}

fn injection_row(model: &SpringModel, p: &bennu_spring::prelude::InjectionPoint) -> InjectionRow {
    let resolves_to: Vec<String> = model.candidates(&p.type_text, &p.qualifier).iter().map(|b| b.name.clone()).collect();
    let unresolved = resolves_to.is_empty().then(|| {
        if model.declares_type(&p.type_text) {
            "no bean of this type is declared"
        } else {
            "a library type — Spring or a dependency may supply it"
        }
    });
    InjectionRow {
        member: p.member.clone(),
        type_name: p.type_text.clone(),
        kind: p.kind.as_str().to_string(),
        qualifier: p.qualifier.clone(),
        resolves_to,
        unresolved,
    }
}

/// A CDI catalog row as a bean. Its children are the injection points the bean satisfies.
pub fn from_cdi(e: &ExtEntry, with_wiring: bool) -> BeanRow {
    BeanRow {
        name: e.primary.clone(),
        type_name: e.secondary.clone(),
        kind: "cdi".into(),
        declared_as: e.kind.clone(),
        scope: String::new(),
        primary: false,
        lazy: false,
        profile: String::new(),
        conditions: e.tags.clone(),
        supertypes: Vec::new(),
        library: None,
        file: e.file.clone(),
        line: e.line,
        injects: None,
        injected_into: with_wiring.then(|| e.children.iter().map(|c| c.primary.clone()).take(MAX_DEPENDENTS).collect()),
    }
}

pub fn collect(root: &str, filter: &BeanFilter) -> Vec<BeanRow> {
    let mut rows: Vec<BeanRow> = Vec::new();
    if let Some(model) = crate::frameworks::spring_model_of(root) {
        rows.extend(spring_rows(&model, filter));
    }
    for e in crate::frameworks::catalog_of(root, "jakartaee.beans") {
        let row = from_cdi(&e, filter.injections);
        if has(&row.name, &filter.name) && has(&row.type_name, &filter.type_name) && cdi_passes_spring_only_filters(filter) {
            rows.push(row);
        }
    }
    rows
}

/// CDI beans have no scope, profile or Spring stereotype to match, so those filters exclude them.
fn cdi_passes_spring_only_filters(f: &BeanFilter) -> bool {
    [&f.stereotype, &f.scope, &f.profile].iter().all(|o| o.as_deref().is_none_or(str::is_empty))
}

pub fn spring_rows(model: &SpringModel, filter: &BeanFilter) -> Vec<BeanRow> {
    let library: &[BeanDef] = if filter.include_libraries { &model.library_beans } else { &[] };
    let mut rows: Vec<BeanRow> = model.beans.iter().chain(library).filter(|b| filter.keeps(b)).map(row_of).collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    if filter.injections && rows.len() <= MAX_WITH_INJECTIONS {
        add_wiring(model, &mut rows);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_spring::prelude::{BeanKind, InjectionKind, InjectionPoint};

    fn bean(name: &str, fqcn: &str, supertypes: &[&str]) -> BeanDef {
        BeanDef {
            name: name.into(),
            fqcn: fqcn.into(),
            kind: BeanKind::Stereotype,
            stereotype: "@Service".into(),
            file: "/p/A.java".into(),
            offset: 0,
            line: 3,
            scope: String::new(),
            primary: false,
            profile: String::new(),
            lazy: false,
            is_abstract: false,
            supertypes: supertypes.iter().map(|s| s.to_string()).collect(),
            conditions: Vec::new(),
            artifact: String::new(),
        }
    }

    fn point(owner: &str, member: &str, ty: &str) -> InjectionPoint {
        InjectionPoint {
            owner_fqcn: owner.into(),
            member: member.into(),
            type_text: ty.into(),
            qualifier: String::new(),
            kind: InjectionKind::Constructor,
            file: "/p/A.java".into(),
            offset: 0,
            line: 5,
        }
    }

    fn model() -> SpringModel {
        let mut m = SpringModel::default();
        m.beans = vec![
            bean("orderService", "com.acme.OrderService", &["OrderApi"]),
            BeanDef { scope: "prototype".into(), ..bean("mailer", "com.acme.Mailer", &[]) },
            BeanDef { primary: true, ..bean("fastRepo", "com.acme.FastRepo", &["Repo"]) },
            bean("slowRepo", "com.acme.SlowRepo", &["Repo"]),
        ];
        m.injections = vec![
            point("com.acme.OrderService", "repo", "Repo"),
            point("com.acme.OrderService", "mailer", "Mailer"),
            point("com.acme.OrderService", "clock", "Clock"),
        ];
        m
    }

    fn names(rows: &[BeanRow]) -> Vec<&str> {
        rows.iter().map(|r| r.name.as_str()).collect()
    }

    #[test]
    fn filters_by_type_reach_supertypes_and_scope_defaults_to_singleton() {
        let m = model();
        let by_type = spring_rows(&m, &BeanFilter { type_name: Some("Repo".into()), ..BeanFilter::default() });
        assert_eq!(names(&by_type), vec!["fastRepo", "slowRepo"]);
        let proto = spring_rows(&m, &BeanFilter { scope: Some("prototype".into()), ..BeanFilter::default() });
        assert_eq!(names(&proto), vec!["mailer"]);
        let single = spring_rows(&m, &BeanFilter { scope: Some("singleton".into()), ..BeanFilter::default() });
        assert_eq!(single.len(), 3);
        assert_eq!(single[0].scope, "singleton");
    }

    #[test]
    fn stereotype_matches_with_or_without_the_at_sign() {
        let m = model();
        let f = |s: &str| BeanFilter { stereotype: Some(s.into()), ..BeanFilter::default() };
        assert_eq!(spring_rows(&m, &f("service")).len(), 4);
        assert_eq!(spring_rows(&m, &f("@Service")).len(), 4);
        assert!(spring_rows(&m, &f("Repository")).is_empty());
    }

    #[test]
    fn wiring_resolves_primary_first_and_says_why_nothing_matched() {
        let m = model();
        let rows = spring_rows(&m, &BeanFilter { name: Some("orderService".into()), injections: true, ..BeanFilter::default() });
        let injects = rows[0].injects.as_ref().unwrap();
        assert_eq!(injects[0].resolves_to, vec!["fastRepo", "slowRepo"], "@Primary leads");
        assert_eq!(injects[1].resolves_to, vec!["mailer"]);
        assert!(injects[2].resolves_to.is_empty());
        assert!(injects[2].unresolved.unwrap().contains("library"), "Clock is declared nowhere in the project");
    }

    #[test]
    fn a_bean_knows_which_classes_inject_it() {
        let m = model();
        let rows = spring_rows(&m, &BeanFilter { name: Some("mailer".into()), injections: true, ..BeanFilter::default() });
        assert_eq!(rows[0].injected_into.as_deref(), Some(&["OrderService".to_string()][..]));
    }

    #[test]
    fn wiring_is_left_out_when_too_many_beans_match() {
        let mut m = SpringModel::default();
        m.beans = (0..=MAX_WITH_INJECTIONS).map(|i| bean(&format!("b{i}"), &format!("p.B{i}"), &[])).collect();
        let rows = spring_rows(&m, &BeanFilter { injections: true, ..BeanFilter::default() });
        assert_eq!(rows.len(), MAX_WITH_INJECTIONS + 1);
        assert!(rows[0].injects.is_none());
    }

    #[test]
    fn cdi_rows_are_beans_without_spring_only_attributes() {
        let e = ExtEntry {
            primary: "audit".into(),
            secondary: "com.acme.Audit".into(),
            kind: "@Produces".into(),
            children: vec![ExtEntry { primary: "Service.audit".into(), ..ExtEntry::default() }],
            ..ExtEntry::default()
        };
        let row = from_cdi(&e, true);
        assert_eq!(row.kind, "cdi");
        assert_eq!(row.injected_into, Some(vec!["Service.audit".to_string()]));
        assert!(from_cdi(&e, false).injected_into.is_none());
        assert!(!cdi_passes_spring_only_filters(&BeanFilter { scope: Some("request".into()), ..BeanFilter::default() }));
    }
}
