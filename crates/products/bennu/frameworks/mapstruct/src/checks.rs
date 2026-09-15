//! The diagnostics, derived from an analysed mapper.
//!
//! Three of them are build failures MapStruct reports against generated code — a property that does
//! not exist, a target mapped twice, elements that cannot go together. The fourth is the one the
//! build lets through: a target property nothing maps, which arrives `null` at run time.

use bennu_proto::prelude::{severity, Diagnostic};

use crate::annotations::{Elem, Lit, MappingAnn};
use crate::mapper::{Mapper, MappingMethod, MethodKind, Policy};
use crate::paths::{segments, source_root, walk, Segment, Side, SourceRoot, Walk};
use crate::properties::Access;
use crate::table::{TypeRef, TypeTable};

pub const CODE_UNKNOWN_TARGET: &str = "mapstruct.unknown-target-property";
pub const CODE_UNKNOWN_SOURCE: &str = "mapstruct.unknown-source-property";
pub const CODE_DUPLICATE_TARGET: &str = "mapstruct.duplicate-target";
pub const CODE_CONFLICTING: &str = "mapstruct.conflicting-mapping";
pub const CODE_UNMAPPED: &str = "mapstruct.unmapped-target";

/// The pairs of `@Mapping` elements the processor refuses together, each with its own message.
/// `ignore` combined with the others is left out: which MapStruct versions reject it is not certain.
const REFUSED: &[(&str, &str)] = &[
    ("source", "constant"),
    ("source", "expression"),
    ("constant", "expression"),
    ("constant", "defaultValue"),
    ("constant", "defaultExpression"),
    ("expression", "defaultValue"),
    ("expression", "defaultExpression"),
    ("defaultValue", "defaultExpression"),
];

/// The properties MapStruct reads off a `String` source: `getBytes()`, `isEmpty()`, `isBlank()`.
const STRING_PROPERTIES: &[&str] = &["bytes", "empty", "blank"];

/// A diagnostic, plus what a fix needs to know about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub diag: Diagnostic,
    /// For an unknown property: the names the path could have meant.
    pub candidates: Vec<String>,
}

pub fn findings(mappers: &[Mapper], table: &TypeTable) -> Vec<Finding> {
    let mut out = Vec::new();
    for method in mappers.iter().flat_map(|m| m.methods.iter()) {
        method_findings(method, table, &mut out);
    }
    out.sort_by_key(|f| f.diag.start);
    out
}

fn method_findings(method: &MappingMethod, table: &TypeTable, out: &mut Vec<Finding>) {
    duplicates(method, out);
    for mapping in &method.anns.mappings {
        conflicts(mapping, out);
    }
    if method.kind == MethodKind::Collection {
        return;
    }
    for mapping in &method.anns.mappings {
        if let Elem::Literal(lit) = &mapping.target {
            // `resultType` and friends replace the declared target — its properties are not the ones.
            if !method.anns.opaque {
                target_path(lit, method, table, out);
            }
        }
        if let Elem::Literal(lit) = &mapping.source {
            source_path(lit, method, table, out);
        }
    }
    if let Some(names) = unmapped(method, table) {
        if !names.is_empty() {
            out.push(unmapped_finding(method, &names));
        }
    }
}

fn target_path(lit: &Lit, method: &MappingMethod, table: &TypeTable, out: &mut Vec<Finding>) {
    if lit.value == "." {
        return;
    }
    let segs = segments(lit);
    report_walk(walk(table, &method.target, &segs), &segs, Side::Target, out);
}

fn source_path(lit: &Lit, method: &MappingMethod, table: &TypeTable, out: &mut Vec<Finding>) {
    if lit.value == "." {
        return;
    }
    let segs = segments(lit);
    let SourceRoot::Param { param, skip } = source_root(&method.sources, table, &segs) else { return };
    let rest = &segs[skip..];
    report_walk(walk(table, &param.ty, rest), rest, Side::Source, out);
}

fn report_walk(end: Walk, segs: &[Segment<'_>], side: Side, out: &mut Vec<Finding>) {
    let Walk::Missing { index, owner, candidates } = end else { return };
    let seg = segs[index];
    let (code, what) = match side {
        Side::Target => (CODE_UNKNOWN_TARGET, "target"),
        Side::Source => (CODE_UNKNOWN_SOURCE, "source"),
    };
    out.push(Finding {
        diag: Diagnostic {
            message: format!(
                "`{}` is not a property of {owner} — MapStruct fails the build with an unknown {what} \
                 property",
                seg.text
            ),
            severity: severity::ERROR.to_string(),
            code: code.to_string(),
            start: seg.start,
            end: seg.end,
        },
        candidates: candidates.iter().filter(|p| side.admits(p)).map(|p| p.name.clone()).collect(),
    });
}

fn duplicates(method: &MappingMethod, out: &mut Vec<Finding>) {
    let mut seen: Vec<&str> = Vec::new();
    for mapping in &method.anns.mappings {
        let Elem::Literal(lit) = &mapping.target else { continue };
        if lit.value.is_empty() || lit.value == "." {
            continue;
        }
        if !seen.contains(&lit.value.as_str()) {
            seen.push(lit.value.as_str());
            continue;
        }
        out.push(error(
            CODE_DUPLICATE_TARGET,
            format!(
                "`{}` is mapped more than once on `{}` — MapStruct refuses a target property mapped twice",
                lit.value, method.name
            ),
            lit.start,
            lit.end,
        ));
    }
}

fn conflicts(mapping: &MappingAnn, out: &mut Vec<Finding>) {
    let Some((a, b)) = REFUSED.iter().find(|(a, b)| mapping.has(a) && mapping.has(b)) else { return };
    out.push(error(
        CODE_CONFLICTING,
        format!("`{a}` and `{b}` cannot both be set on one @Mapping — MapStruct refuses the mapper"),
        mapping.start,
        mapping.end,
    ));
}

fn error(code: &str, message: String, start: usize, end: usize) -> Finding {
    Finding {
        diag: Diagnostic { message, severity: severity::ERROR.to_string(), code: code.to_string(), start, end },
        candidates: Vec::new(),
    }
}

/// The target properties a method certainly leaves unset — `None` when that cannot be known.
///
/// Every `None` below is a reason the answer is not this crate's to give: a policy that is not a
/// warning, mappings that come from elsewhere, a type that is not fully visible, a wildcard.
pub fn unmapped(method: &MappingMethod, table: &TypeTable) -> Option<Vec<String>> {
    if method.kind == MethodKind::Collection || matches!(method.policy, Policy::Ignore | Policy::Unknown) {
        return None;
    }
    let anns = &method.anns;
    if anns.ignore_by_default || anns.inherits || anns.opaque || anns.foreign {
        return None;
    }
    let TypeRef::Project(target) = &method.target else { return None };
    let view = table.view(target)?;
    if !view.complete {
        return None;
    }

    // Explicitly targeted: the first segment of every `target`, whatever it is mapped from.
    let mut covered: Vec<String> = Vec::new();
    for mapping in &anns.mappings {
        let Elem::Literal(target) = &mapping.target else { return None };
        let wildcard_source = matches!(&mapping.source, Elem::Literal(s) if s.value == ".");
        if target.value == "." || wildcard_source {
            return None;
        }
        covered.push(target.value.split('.').next().unwrap_or("").to_string());
    }
    // Implicitly: a parameter of that name, or a property of that name on any source.
    for source in &method.sources {
        covered.push(source.name.clone());
        match &source.ty {
            TypeRef::Project(fqcn) => {
                let v = table.view(fqcn)?;
                if !v.complete {
                    return None;
                }
                covered.extend(v.props.into_iter().map(|p| p.name));
            }
            TypeRef::Value => {
                if matches!(source.type_text.trim(), "String" | "java.lang.String") {
                    covered.extend(STRING_PROPERTIES.iter().map(|n| n.to_string()));
                }
            }
            TypeRef::Collection | TypeRef::Unknown => return None,
        }
    }
    Some(
        view.props
            .iter()
            .filter(|p| p.writable == Access::Yes && !covered.contains(&p.name))
            .map(|p| p.name.clone())
            .collect(),
    )
}

fn unmapped_finding(method: &MappingMethod, names: &[String]) -> Finding {
    let noun = if names.len() == 1 { "property" } else { "properties" };
    let level = if method.policy == Policy::Error { severity::ERROR } else { severity::WARNING };
    Finding {
        diag: Diagnostic {
            message: format!(
                "Unmapped target {noun}: \"{}\" — the generated `{}` leaves {} unset",
                names.join(", "),
                method.name,
                if names.len() == 1 { "it" } else { "them" }
            ),
            severity: level.to_string(),
            code: CODE_UNMAPPED.to_string(),
            start: method.name_offset,
            end: method.name_offset + method.name.len(),
        },
        candidates: names.to_vec(),
    }
}
