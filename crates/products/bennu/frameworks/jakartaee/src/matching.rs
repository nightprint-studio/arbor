//! Matching an injection point against the beans that could satisfy it.
//!
//! Every comparison answers [`Fit::Yes`], [`Fit::No`] or [`Fit::Maybe`], and the two consumers read the
//! third answer in opposite directions — which is the whole trick:
//!
//! - **"Unsatisfied"** needs every bean to be a certain `No`. One `Maybe` and the point might be
//!   satisfied.
//! - **"Ambiguous"** needs two certain `Yes`es. A `Maybe` does not count towards it.
//!
//! The gutter and go-to show `Yes` and `Maybe` alike: a picker the user reads can afford a row the
//! container would not have chosen; a diagnostic cannot.

use crate::archive::ArchiveMode;
use crate::beans::{Bean, BeanOrigin};
use crate::inject::{InjectKind, InjectionPoint};
use crate::qualifiers::Qualifiers;
use crate::text::simple_name;
use crate::types::{TypeRef, TypeView};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Fit {
    No,
    Maybe,
    Yes,
}

/// Types a container provides itself — never a project bean's to satisfy, and never ambiguous.
const BUILT_IN: &[&str] = &[
    "Instance", "Provider", "Event", "InjectionPoint", "BeanManager", "Conversation", "Principal",
    "HttpServletRequest", "HttpSession", "ServletContext", "UserTransaction", "Interceptor",
    "Decorator", "Bean", "Object",
];

/// Whether a bean's types include `target`.
pub fn assignable(view: &TypeView<'_>, bean: &Bean, target: &TypeRef) -> Fit {
    let fit = match target {
        TypeRef::Project(t) => match &bean.declared {
            TypeRef::Project(b) => {
                let a = view.ancestry(b);
                match a.project.get(t) {
                    Some(true) => Fit::Yes,
                    Some(false) => Fit::Maybe,
                    None if a.open => Fit::Maybe,
                    None => Fit::No,
                }
            }
            TypeRef::Library(_) => Fit::No,
            TypeRef::Unresolved(_) => Fit::Maybe,
        },
        TypeRef::Library(t) | TypeRef::Unresolved(t) => match &bean.declared {
            TypeRef::Project(b) => {
                let a = view.ancestry(b);
                let best = a.library.iter().map(|l| same_name(l, t)).max().unwrap_or(Fit::No);
                if best == Fit::No && a.open { Fit::Maybe } else { best }
            }
            TypeRef::Library(b) | TypeRef::Unresolved(b) => same_name(b, t),
        },
    };
    // A parameterized bean type is assignable to a raw one only when its arguments are unbounded
    // variables or `Object` — which the source rarely says plainly enough to be sure.
    if bean.parameterized { fit.min(Fit::Maybe) } else { fit }
}

/// Two names for the same library type: certain when both are qualified, a guess when one is not.
fn same_name(a: &str, b: &str) -> Fit {
    if a.contains('.') && b.contains('.') {
        return if a == b { Fit::Yes } else { Fit::No };
    }
    if simple_name(a) == simple_name(b) { Fit::Maybe } else { Fit::No }
}

/// Whether a bean's qualifiers satisfy what a point requires.
pub fn qualifiers_fit(required: &Qualifiers, bean: &Bean) -> Fit {
    if required.unclassified || required.unnamed || required.legacy_new {
        return Fit::Maybe;
    }
    let only_any = required.any && required.named.is_none() && required.custom.is_empty() && !required.explicit_default;
    if only_any {
        return Fit::Yes;
    }
    let have = &bean.quals;
    let mut fit = Fit::Yes;
    if let Some(name) = &required.named {
        if have.named.as_deref() != Some(name.as_str()) {
            return Fit::No;
        }
    }
    for wanted in &required.custom {
        match have.custom.iter().find(|h| h.fqcn == wanted.fqcn) {
            Some(h) if h.args == wanted.args => {}
            // Differing members may be `@Nonbinding`.
            Some(_) => fit = fit.min(Fit::Maybe),
            None => return Fit::No,
        }
    }
    let wants_default = required.explicit_default || (required.named.is_none() && required.custom.is_empty());
    let named_only = required.named.is_some() && required.custom.is_empty() && !required.explicit_default;
    if wants_default || named_only {
        let default = if have.explicit_default {
            Fit::Yes
        } else if !have.custom.is_empty() {
            Fit::No
        } else if have.unclassified {
            Fit::Maybe
        } else {
            Fit::Yes
        };
        // Whether `@Named` alone at an injection point also requires `@Default` changed in CDI 4.
        let default = if named_only && default == Fit::No { Fit::Maybe } else { default };
        fit = fit.min(default);
    }
    fit
}

/// A bean and how well it fits a point.
#[derive(Debug, Clone, Copy)]
pub struct Candidate<'b> {
    pub bean: &'b Bean,
    pub fit: Fit,
}

/// How well `bean` fits `point`, qualifiers included (an `@EJB` point has none to compare).
pub fn fit(view: &TypeView<'_>, bean: &Bean, point: &InjectionPoint) -> Fit {
    if !bean.injectable || (point.kind == InjectKind::Ejb && !bean.ejb) {
        return Fit::No;
    }
    let types = assignable(view, bean, &point.target);
    if types == Fit::No || point.kind == InjectKind::Ejb {
        return types;
    }
    types.min(qualifiers_fit(&point.quals, bean))
}

/// The beans worth showing for a point: explicit beans and producers always, plain classes only in
/// an archive where they are beans. Certain fits first.
pub fn candidates<'b>(view: &TypeView<'_>, beans: &'b [Bean], point: &InjectionPoint, mode: ArchiveMode) -> Vec<Candidate<'b>> {
    let mut out: Vec<Candidate<'b>> = beans
        .iter()
        .filter(|b| b.explicit || b.origin != BeanOrigin::Class || mode == ArchiveMode::All)
        .map(|b| Candidate { bean: b, fit: fit(view, b, point) })
        .filter(|c| c.fit != Fit::No)
        .collect();
    out.sort_by(|a, b| b.fit.cmp(&a.fit).then_with(|| a.bean.label().cmp(&b.bean.label())));
    out
}

/// What the container would make of a point — when the source is enough to say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Satisfied,
    Unsatisfied,
    Ambiguous(usize),
    Unknown,
}

/// Project-wide reasons to claim nothing about any point.
#[derive(Debug, Clone, Copy)]
pub struct Gates {
    pub mode: ArchiveMode,
    /// Beans may exist that the source does not show: a portable extension, a producer of a type
    /// nobody can read, EJBs declared in `ejb-jar.xml`, a scan that did not finish, Quarkus.
    pub open_world: bool,
}

/// Resolve a point as the container would, or say [`Resolution::Unknown`].
pub fn resolution(view: &TypeView<'_>, beans: &[Bean], point: &InjectionPoint, gates: Gates) -> Resolution {
    if point.kind == InjectKind::Ejb || point.parameterized || gates.mode == ArchiveMode::Unknown {
        return Resolution::Unknown;
    }
    let q = &point.quals;
    if q.unclassified || q.unnamed || q.legacy_new {
        return Resolution::Unknown;
    }
    let project_target = matches!(point.target, TypeRef::Project(_));
    let qualified_library = matches!(&point.target, TypeRef::Library(n) if n.contains('.') && !BUILT_IN.contains(&simple_name(n)));
    if !project_target && !qualified_library {
        return Resolution::Unknown;
    }
    let related: Vec<&Bean> = beans.iter().filter(|b| assignable(view, b, &point.target) != Fit::No).collect();
    if related.iter().any(|b| b.typed || b.alternative || b.priority || b.specializes) {
        return Resolution::Unknown;
    }
    let possible = related
        .iter()
        .filter(|b| b.explicit || b.maybe_explicit || b.origin != BeanOrigin::Class || gates.mode == ArchiveMode::All)
        .filter(|b| fit(view, b, point) != Fit::No)
        .count();
    if possible == 0 {
        return if project_target && !gates.open_world { Resolution::Unsatisfied } else { Resolution::Unknown };
    }
    if gates.open_world {
        return Resolution::Unknown;
    }
    let certain = related
        .iter()
        .filter(|b| b.explicit && !b.ejb && !inherits_annotations(view, b))
        .filter(|b| fit(view, b, point) == Fit::Yes)
        .count();
    if certain >= 2 { Resolution::Ambiguous(certain) } else { Resolution::Satisfied }
}

/// Whether a class bean extends a project class carrying annotations — an `@Inherited` qualifier
/// among them would change what the bean matches.
fn inherits_annotations(view: &TypeView<'_>, bean: &Bean) -> bool {
    if bean.origin != BeanOrigin::Class {
        return false;
    }
    let mut current = bean.owner.clone();
    for _ in 0..16 {
        let Some(row) = view.get(&current) else { return false };
        let Some(TypeRef::Project(parent)) = row.extends.as_ref().map(|e| &e.target) else { return false };
        match view.get(parent) {
            Some(p) if p.annotated => return true,
            Some(_) => current = parent.clone(),
            None => return true,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::Project;

    fn resolve_first(p: &Project) -> Resolution {
        p.resolve_point(0)
    }

    #[test]
    fn a_single_scoped_implementation_satisfies_its_interface() {
        let p = Project::cdi(&[
            ("/p/a/Svc.java", "package a;\npublic interface Svc {}\n"),
            ("/p/a/Impl.java", "package a;\n@ApplicationScoped public class Impl implements Svc {}\n"),
            ("/p/a/Use.java", "package a;\n@RequestScoped public class Use { @Inject Svc svc; }\n"),
        ]);
        assert_eq!(resolve_first(&p), Resolution::Satisfied);
        let c = p.candidates_of(0);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].fit, Fit::Yes);
    }

    #[test]
    fn assignability_reaches_through_the_project_hierarchy() {
        let p = Project::cdi(&[
            ("/p/a/Svc.java", "package a;\npublic interface Svc {}\n"),
            ("/p/a/Base.java", "package a;\npublic abstract class Base implements Svc {}\n"),
            ("/p/a/Impl.java", "package a;\n@ApplicationScoped public class Impl extends Base {}\n"),
            ("/p/a/Use.java", "package a;\n@RequestScoped public class Use { @Inject Svc svc; }\n"),
        ]);
        assert_eq!(resolve_first(&p), Resolution::Satisfied);
    }

    #[test]
    fn a_qualifier_selects_among_implementations() {
        let p = Project::cdi(&[
            ("/p/a/Svc.java", "package a;\npublic interface Svc {}\n"),
            ("/p/a/Fast.java", "package a;\n@Qualifier public @interface Fast {}\n"),
            ("/p/a/Slow.java", "package a;\n@ApplicationScoped public class Slow implements Svc {}\n"),
            ("/p/a/Quick.java", "package a;\n@Fast @ApplicationScoped public class Quick implements Svc {}\n"),
            ("/p/a/Use.java", "package a;\n@RequestScoped public class Use { @Inject @Fast Svc svc; @Inject Svc plain; }\n"),
        ]);
        let fast = p.candidates_of(0);
        assert_eq!(fast.iter().map(|c| c.bean.label()).collect::<Vec<_>>(), ["Quick"]);
        let plain = p.candidates_of(1);
        assert_eq!(plain.iter().map(|c| c.bean.label()).collect::<Vec<_>>(), ["Slow"], "a qualified bean loses @Default");
        assert_eq!(p.resolve_point(1), Resolution::Satisfied);
    }

    #[test]
    fn named_matches_by_name() {
        let p = Project::cdi(&[
            ("/p/a/Svc.java", "package a;\npublic interface Svc {}\n"),
            ("/p/a/A.java", "package a;\n@Named(\"one\") @ApplicationScoped public class A implements Svc {}\n"),
            ("/p/a/B.java", "package a;\n@Named @ApplicationScoped public class B implements Svc {}\n"),
            ("/p/a/Use.java", "package a;\n@RequestScoped public class Use { @Inject @Named(\"b\") Svc svc; }\n"),
        ]);
        assert_eq!(p.candidates_of(0).iter().map(|c| c.bean.label()).collect::<Vec<_>>(), ["B"]);
    }
}
