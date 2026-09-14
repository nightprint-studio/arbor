//! Injection points: the places a bean is asked for.
//!
//! `@Inject` on a field, on a constructor (every parameter), on an initializer method (every
//! parameter); and `@EJB` on a field. Producer, disposer and observer parameters are injection points
//! too, and are not read here — they are rarely where the question "what do I get" is asked.

use bennu_facts::prelude::AnnFacts;

use crate::known;
use crate::qualifiers::{qualifiers_of, Qualifiers};
use crate::text::{erase, line_of, simple_name};
use crate::types::{resolve, TypeRef, TypeView, Unit};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectKind {
    Field,
    Constructor,
    Initializer,
    /// `@EJB` — resolved by the EJB container through business interfaces, not by CDI's rules.
    Ejb,
}

impl InjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            InjectKind::Field => "field",
            InjectKind::Constructor => "constructor",
            InjectKind::Initializer => "initializer",
            InjectKind::Ejb => "@EJB",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InjectionPoint {
    pub owner: String,
    /// The field or parameter name.
    pub member: String,
    pub kind: InjectKind,
    pub type_text: String,
    pub target: TypeRef,
    /// Written with type arguments, or as an array.
    pub parameterized: bool,
    pub quals: Qualifiers,
    pub is_final: bool,
    pub is_static: bool,
    /// Span of the `@Inject` / `@EJB` that makes this an injection point.
    pub marker_start: usize,
    pub marker_end: usize,
    pub file: String,
    /// Byte offset of the member's name.
    pub offset: usize,
    pub line: u32,
}

impl InjectionPoint {
    /// `OrderController.orders`.
    pub fn label(&self) -> String {
        format!("{}.{}", simple_name(&self.owner), self.member)
    }

    /// `com.acme.OrderController#orders`.
    pub fn id(&self) -> String {
        format!("{}#{}", self.owner, self.member)
    }

    /// Whether the caret is on the member's name.
    pub fn covers(&self, offset: usize) -> bool {
        offset >= self.offset && offset <= self.offset + self.member.len()
    }
}

/// Every injection point a unit declares.
pub fn points_of(unit: &Unit, view: &TypeView<'_>) -> Vec<InjectionPoint> {
    let facts = &unit.facts;
    let mut out = Vec::new();
    for t in &facts.types {
        for f in &t.fields {
            let (marker, kind) = match known::find(&f.annotations, facts, "Inject") {
                Some(a) => (a, InjectKind::Field),
                None => match known::find(&f.annotations, facts, "EJB") {
                    Some(a) => (a, InjectKind::Ejb),
                    None => continue,
                },
            };
            let site = Site { owner: &t.fqcn, name: &f.name, type_text: &f.type_text, offset: f.name_offset };
            let mut found = point(unit, view, site, &f.annotations, Some(f.name.as_str()), marker, kind);
            found.is_final = f.is_final;
            found.is_static = f.is_static;
            out.push(found);
        }
        for m in &t.methods {
            let Some(marker) = known::find(&m.annotations, facts, "Inject") else { continue };
            let kind = if m.is_constructor { InjectKind::Constructor } else { InjectKind::Initializer };
            for p in &m.params {
                let site = Site { owner: &t.fqcn, name: &p.name, type_text: &p.type_text, offset: p.name_offset };
                // A parameter's `@Named` must say its name: CDI has no default for one.
                out.push(point(unit, view, site, &p.annotations, None, marker, kind));
            }
        }
    }
    out
}

/// Where a point is declared.
struct Site<'a> {
    owner: &'a str,
    name: &'a str,
    type_text: &'a str,
    offset: usize,
}

fn point(
    unit: &Unit,
    view: &TypeView<'_>,
    site: Site<'_>,
    anns: &[AnnFacts],
    default_name: Option<&str>,
    marker: &AnnFacts,
    kind: InjectKind,
) -> InjectionPoint {
    let facts = &unit.facts;
    let erased = erase(site.type_text);
    InjectionPoint {
        owner: site.owner.to_string(),
        member: site.name.to_string(),
        kind,
        type_text: site.type_text.to_string(),
        target: resolve(site.type_text, facts, &view.names()),
        parameterized: erased.parameterized || erased.array,
        quals: qualifiers_of(anns, facts, view, default_name).0,
        is_final: false,
        is_static: false,
        marker_start: marker.start,
        marker_end: marker.end,
        file: facts.file.clone(),
        offset: site.offset,
        line: line_of(&unit.text, site.offset),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TypeTable;

    fn points(src: &str) -> Vec<InjectionPoint> {
        let units = vec![Unit::new("/p/a/C.java", src).unwrap()];
        let table = TypeTable::build(&units, Vec::<String>::new());
        points_of(&units[0], &TypeView::of(&table))
    }

    #[test]
    fn fields_constructors_initializers_and_ejb_fields_are_points() {
        let src = "package a;\nimport jakarta.inject.*;\nimport jakarta.ejb.EJB;\nclass C {\n  @Inject Repo repo;\n  @EJB Mailer mailer;\n  Clock untouched;\n  @Inject C(Dao dao, @Named(\"fast\") Engine engine) {}\n  @Inject void init(Audit audit) {}\n}\n";
        let p = points(src);
        let kinds: Vec<(&str, InjectKind)> = p.iter().map(|x| (x.member.as_str(), x.kind)).collect();
        assert_eq!(
            kinds,
            [
                ("repo", InjectKind::Field),
                ("mailer", InjectKind::Ejb),
                ("dao", InjectKind::Constructor),
                ("engine", InjectKind::Constructor),
                ("audit", InjectKind::Initializer),
            ]
        );
        assert_eq!(p[3].quals.named.as_deref(), Some("fast"));
        assert_eq!(&src[p[0].offset..p[0].offset + 4], "repo");
        assert_eq!(&src[p[0].marker_start..p[0].marker_end], "@Inject");
        assert_eq!(p[0].line, 5);
    }

    #[test]
    fn a_spring_autowired_is_not_a_cdi_point() {
        assert!(points("package a;\nimport org.springframework.beans.factory.annotation.Autowired;\nclass C { @Autowired Repo repo; }\n").is_empty());
    }

    #[test]
    fn modifiers_and_generics_ride_along() {
        let p = points("package a;\nimport javax.inject.Inject;\nclass C { @Inject static final java.util.List<Foo> all; }\n");
        assert!(p[0].is_final && p[0].is_static && p[0].parameterized);
    }
}
