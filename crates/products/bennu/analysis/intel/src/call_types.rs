//! The types a *Create method* signature is written with — read from the resolver.
//!
//! `bennu-refactor` renders the method a call asks for from the tree alone, and what the tree does
//! not declare (`total(order.getCustomer().name())`, `var x = …`, an assignment to a field) it
//! used to write as `Object`. This answers those questions with the same inference the checks and
//! hover use, so the method is created with the types it will actually be called with.
//!
//! What it declines — a type that does not resolve, a bare type variable, `null`, `void` — it
//! leaves to the caller's fallback, which is still `Object`. The classes it wrote are remembered
//! ([`ResolverCallTypes::classes`]) so the offer can carry the imports the signature needs.

use std::cell::RefCell;

use bennu_java::prelude::{
    expected_type_at, infer_node_type_cached, FileSymbols, InferCache, TypeRef, TypeResolver,
};
use bennu_refactor::prelude::CallTypes;
use tree_sitter::Node;

use crate::accessor_completion::{collect_classes, written_type};

/// A [`CallTypes`] backed by a resolver, over one parsed buffer.
pub struct ResolverCallTypes<'a> {
    root: Node<'a>,
    source: &'a str,
    symbols: &'a FileSymbols,
    resolver: &'a dyn TypeResolver,
    cache: &'a InferCache,
    classes: RefCell<Vec<String>>,
}

impl<'a> ResolverCallTypes<'a> {
    pub fn new(
        root: Node<'a>,
        source: &'a str,
        symbols: &'a FileSymbols,
        resolver: &'a dyn TypeResolver,
        cache: &'a InferCache,
    ) -> Self {
        ResolverCallTypes { root, source, symbols, resolver, cache, classes: RefCell::new(Vec::new()) }
    }

    /// Every class the types written so far name, dotted — what the file may have to import for the
    /// signature to compile.
    pub fn classes(&self) -> Vec<String> {
        self.classes.borrow().clone()
    }

    fn write(&self, ty: &TypeRef) -> Option<String> {
        if !writable(ty) {
            return None;
        }
        collect_classes(ty, &mut self.classes.borrow_mut());
        Some(written_type(ty))
    }
}

impl CallTypes for ResolverCallTypes<'_> {
    fn argument(&self, node: &Node<'_>) -> Option<String> {
        let ty = infer_node_type_cached(&self.root, self.source, self.symbols, node, self.resolver, self.cache)?;
        self.write(&ty)
    }

    fn result(&self, call: &Node<'_>) -> Option<String> {
        let ty = expected_type_at(&self.root, self.source, self.symbols, call.start_byte(), self.resolver)?;
        self.write(&ty)
    }
}

/// Whether a type can be written into a parameter list or as a return type: a class, an array of
/// one, or a primitive. Not `void`, not `null`, and not a bare type variable (`T`), which names
/// something that is not in scope where the method is being written.
fn writable(ty: &TypeRef) -> bool {
    let name = ty.binary_name.as_str();
    if name.contains('/') {
        return true;
    }
    matches!(name, "int" | "long" | "double" | "float" | "boolean" | "char" | "byte" | "short")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_and_primitives_can_be_written_and_the_rest_cannot() {
        assert!(writable(&TypeRef::simple("java/lang/String")));
        assert!(writable(&TypeRef::simple("int")));
        assert!(writable(&TypeRef { dims: 1, ..TypeRef::simple("long") }));
        assert!(!writable(&TypeRef::simple("void")));
        assert!(!writable(&TypeRef::simple("null")));
        assert!(!writable(&TypeRef::simple("T")), "a type variable is not in scope in the new method");
    }
}
