//! A method declaration's parameters in the one form that compares with a resolved member's.
//!
//! Source writes a parameter type the way the file imports it (`Function<UriBuilder, URI>`); the
//! member model carries binary names (`java/util/function/Function`). What both sides can say is each
//! parameter's simple type name and array depth, and that is what picks the declaration of an
//! overload out of a file — for go-to and rename (`bennu-intel`) and for a library's Javadoc
//! ([`crate::javadoc::FileDocs`]), which must never disagree about which overload is which.

use tree_sitter::Node;

use crate::seam::TypeRef;

/// One parameter as written: simple type name and array depth (`Object...` is `("Object", 1)`).
pub type ParamShape = (String, usize);

/// The parameters of the method, constructor or annotation element `decl`, as [`ParamShape`]s.
/// `None` when `decl` has no parameter list or one of its types cannot be read.
pub fn declared_parameter_shapes(decl: &Node, source: &str) -> Option<Vec<ParamShape>> {
    let list = decl.child_by_field_name("parameters")?;
    let mut c = list.walk();
    let params: Vec<Node> = list
        .named_children(&mut c)
        .filter(|p| matches!(p.kind(), "formal_parameter" | "spread_parameter"))
        .collect();
    params
        .iter()
        .map(|p| {
            let (ty, varargs) = crate::symbols::parameter_type_node(p)?;
            let erased = crate::typename::erase_type_arguments(ty.utf8_text(source.as_bytes()).ok()?);
            let (base, dims) = crate::typename::split_array_dims(&erased);
            let simple = base.rsplit('.').next().unwrap_or(base).trim().to_string();
            Some((simple, dims + usize::from(varargs)))
        })
        .collect()
}

/// Whether written shapes and a member's parameter types agree, by simple name and depth.
pub fn parameter_shapes_match(written: &[ParamShape], params: &[TypeRef]) -> bool {
    written.len() == params.len()
        && written.iter().zip(params).all(|((simple, dims), p)| {
            let mut binary = p.binary_name.as_str();
            let mut depth = p.dims as usize;
            while let Some(element) = binary.strip_suffix("[]") {
                binary = element;
                depth += 1;
            }
            let bound = binary.rsplit(['/', '$', '.']).next().unwrap_or(binary);
            bound == simple && depth == *dims
        })
}

/// Which of `declarations` (in source order; `None` where a list could not be read) takes `params`.
///
/// The first whose shapes match; failing that, the only one with the same parameter COUNT (a type
/// variable the classpath erased to its bound reads differently on the two sides). `None` when
/// neither rule settles it — the caller keeps its own fallback.
pub fn choose_overload(declarations: &[Option<&[ParamShape]>], params: &[TypeRef]) -> Option<usize> {
    if let Some(i) = declarations
        .iter()
        .position(|w| w.is_some_and(|w| parameter_shapes_match(w, params)))
    {
        return Some(i);
    }
    let mut same_count = declarations
        .iter()
        .enumerate()
        .filter(|(_, w)| w.is_some_and(|w| w.len() == params.len()))
        .map(|(i, _)| i);
    let only = same_count.next()?;
    same_count.next().is_none().then_some(only)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shapes(list: &[(&str, usize)]) -> Vec<ParamShape> {
        list.iter().map(|(s, d)| (s.to_string(), *d)).collect()
    }

    #[test]
    fn written_shapes_erase_generics_and_count_varargs() {
        let src = "interface U { U uri(java.util.function.Function<B, URI> f, Object... rest); }";
        let tree = crate::grammar::parse_java(src).unwrap();
        let name = src.find("uri").unwrap();
        let decl = tree
            .root_node()
            .named_descendant_for_byte_range(name, name)
            .and_then(|n| n.parent())
            .unwrap();
        assert_eq!(
            declared_parameter_shapes(&decl, src),
            Some(shapes(&[("Function", 0), ("Object", 1)]))
        );
    }

    #[test]
    fn the_matching_shape_wins_over_declaration_order() {
        let by_uri = shapes(&[("URI", 0)]);
        let by_function = shapes(&[("Function", 0)]);
        let decls = [Some(by_uri.as_slice()), Some(by_function.as_slice())];
        let function = [TypeRef::simple("java/util/function/Function")];
        assert_eq!(choose_overload(&decls, &function), Some(1));
    }

    #[test]
    fn an_unmatched_shape_settles_only_on_a_unique_count() {
        let one = shapes(&[("T", 0)]);
        let two = shapes(&[("T", 0), ("U", 0)]);
        let erased = [TypeRef::simple("java/lang/Object")];
        assert_eq!(choose_overload(&[Some(one.as_slice()), Some(two.as_slice())], &erased), Some(0));
        assert_eq!(choose_overload(&[Some(one.as_slice()), Some(one.as_slice())], &erased), None);
    }
}
