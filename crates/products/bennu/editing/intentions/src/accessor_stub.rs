//! The accessor a field does not have yet, as text.
//!
//! ## Why a completion and not a dialog
//!
//! Arbor already generates accessors: **Generate** (Alt+Insert) asks which fields, in which
//! style, and writes them all. That is the right shape when you are finishing a class. It is the
//! wrong shape when you are in the middle of one and want *this* getter — you know its name, you
//! have already typed half of it, and being asked to open a dialog and find the field in a
//! checklist is the interruption the popup exists to avoid.
//!
//! So the same members are also offered where they are being reached for: typing `getNa` in a
//! class body offers `getName()`, and accepting it writes the method. This module is the text; the
//! deciding — which fields, which of them already have one — is `bennu-intel`'s, because it needs
//! the type hierarchy and this crate has no resolver.
//!
//! ## The conventional form, and only that
//!
//! `public`, the field's own type, `this.` on the assignment, `static` when the field is. The
//! Generate dialog carries the options — fluent accessors, snake_case naming, `final` parameters —
//! because a dialog is where a style conversation belongs. A completion candidate has no
//! conversation: it is one row that either says what you meant or does not, so it takes the form
//! Java writes by default and nothing else.
//!
//! Sits next to [`crate::override_stub`] for the same reason it exists: what a member looks like
//! is a string transform, and it is testable without a project.

/// A field an accessor can be written for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSpec {
    /// The identifier, as declared.
    pub name: String,
    /// The type **as written** — `String`, `int`, `List<Order>`. Copied verbatim so the accessor
    /// reads like the code around it rather than like a binary name.
    pub type_text: String,
    pub is_static: bool,
    /// A `final` field has nothing to set, so neither a setter nor a wither is offered for it.
    pub is_final: bool,
    /// The simple name of the type that declares it — what a wither returns, and the one thing
    /// about an accessor that is not a fact about the field alone.
    pub owner: String,
}

/// Which accessor to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accessor {
    Get,
    Set,
    /// The builder-style setter: `withCustomer(…)` assigns and returns the object, so calls chain.
    /// The same member the **Generate** dialog writes under "With" — one house form, not two.
    With,
}

impl Accessor {
    /// Every accessor a field can be offered, in the order the popup ranks them.
    pub const ALL: [Accessor; 3] = [Accessor::Get, Accessor::Set, Accessor::With];
}

/// The name Java conventionally gives this accessor.
///
/// The `is` prefix is a **primitive** `boolean` rule and not a `Boolean` one — JavaBeans says so,
/// and every framework that reflects over accessors reads it that way, so a `Boolean isActive()`
/// is a property nothing finds. A field already named `isActive` keeps the prefix it has rather
/// than growing a second one, which is also the rule Lombok follows: what makes it a prefix is
/// that a lowercase letter does not follow it.
pub fn accessor_name(field: &FieldSpec, which: Accessor) -> String {
    match which {
        Accessor::Set => format!("set{}", strip_is(&field.name)),
        Accessor::With => format!("with{}", strip_is(&field.name)),
        Accessor::Get if field.type_text.trim() == "boolean" => {
            if has_is_prefix(&field.name) {
                field.name.clone()
            } else {
                format!("is{}", upper_first(&field.name))
            }
        }
        Accessor::Get => format!("get{}", strip_is(&field.name)),
    }
}

/// The accessor as source, every line after the first prefixed with `indent`.
///
/// The first line is NOT indented: it lands where the caret already is, and whatever whitespace
/// put the caret there is the indentation the reader chose.
pub fn render_accessor(field: &FieldSpec, which: Accessor, indent: &str) -> String {
    let name = accessor_name(field, which);
    let statik = if field.is_static { "static " } else { "" };
    let ty = field.type_text.trim();
    let f = &field.name;
    match which {
        Accessor::Get => format!(
            "public {statik}{ty} {name}() {{\n{indent}    return {f};\n{indent}}}"
        ),
        // `this.` on the assignment, because the parameter shadows the field and the assignment
        // would otherwise be the parameter to itself — legal, silent, and wrong.
        Accessor::Set => format!(
            "public {statik}void {name}({ty} {f}) {{\n{indent}    this.{f} = {f};\n{indent}}}"
        ),
        // Returns the object so calls chain — which is the whole of what makes it a wither rather
        // than a setter with a longer name. `static` never applies: there is no `this` to return.
        Accessor::With => {
            let owner = &field.owner;
            format!(
                "public {owner} {name}({ty} {f}) {{\n{indent}    this.{f} = {f};\n{indent}    return this;\n{indent}}}"
            )
        }
    }
}

/// Whether `which` is worth offering for this field at all.
///
/// A `final` field is assigned once, at construction — nothing to set and nothing to chain. And a
/// wither returns `this`, which a `static` member does not have.
pub fn offers(field: &FieldSpec, which: Accessor) -> bool {
    match which {
        Accessor::Get => true,
        Accessor::Set => !field.is_final,
        Accessor::With => !field.is_final && !field.is_static,
    }
}

/// `name` → `Name`, leaving a name that is already capitalised alone.
fn upper_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Whether `name` already carries the `is` prefix — `is` followed by something that is not a
/// lowercase letter, so `isActive` does and `island` does not.
fn has_is_prefix(name: &str) -> bool {
    name.strip_prefix("is")
        .is_some_and(|rest| !rest.is_empty() && !rest.starts_with(|c: char| c.is_lowercase()))
}

/// The property name for a `get`/`set` accessor: `isActive` → `Active`, `name` → `Name`.
///
/// A boolean field named `isActive` is the property `active`, so its setter is `setActive` and not
/// `setIsActive` — which is what every reflective framework will look for.
fn strip_is(name: &str) -> String {
    if has_is_prefix(name) {
        upper_first(&name[2..])
    } else {
        upper_first(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(name: &str, ty: &str) -> FieldSpec {
        FieldSpec {
            name: name.to_string(),
            type_text: ty.to_string(),
            is_static: false,
            is_final: false,
            owner: "Order".to_string(),
        }
    }

    #[test]
    fn the_conventional_names() {
        assert_eq!(accessor_name(&f("name", "String"), Accessor::Get), "getName");
        assert_eq!(accessor_name(&f("name", "String"), Accessor::Set), "setName");
    }

    /// JavaBeans says `is` for a primitive `boolean` and `get` for the wrapper, and every
    /// framework that reflects over accessors reads it that way.
    #[test]
    fn a_primitive_boolean_reads_with_is_and_a_wrapper_does_not() {
        assert_eq!(accessor_name(&f("active", "boolean"), Accessor::Get), "isActive");
        assert_eq!(accessor_name(&f("active", "Boolean"), Accessor::Get), "getActive");
    }

    /// A field that already carries the prefix keeps the one it has — and its setter is named for
    /// the PROPERTY, which is what a framework will look for.
    #[test]
    fn a_field_already_named_is_something_does_not_grow_a_second_prefix() {
        assert_eq!(accessor_name(&f("isActive", "boolean"), Accessor::Get), "isActive");
        assert_eq!(accessor_name(&f("isActive", "boolean"), Accessor::Set), "setActive");
    }

    /// …and `is` is only a prefix when what follows it is not a lowercase letter.
    #[test]
    fn a_field_that_merely_starts_with_the_letters_is_not_prefixed() {
        assert_eq!(accessor_name(&f("island", "String"), Accessor::Get), "getIsland");
    }

    #[test]
    fn a_getter_returns_the_field() {
        assert_eq!(
            render_accessor(&f("name", "String"), Accessor::Get, "    "),
            "public String getName() {\n        return name;\n    }"
        );
    }

    /// The parameter shadows the field, so the assignment without `this.` is the parameter to
    /// itself — legal, silent, and wrong.
    #[test]
    fn a_setter_qualifies_the_assignment() {
        assert_eq!(
            render_accessor(&f("name", "String"), Accessor::Set, "    "),
            "public void setName(String name) {\n        this.name = name;\n    }"
        );
    }

    #[test]
    fn a_static_field_gets_a_static_accessor() {
        let mut field = f("count", "int");
        field.is_static = true;
        assert!(render_accessor(&field, Accessor::Get, "").starts_with("public static int getCount()"));
    }

    /// A `final` field is assigned once, at construction — nothing to set and nothing to chain.
    #[test]
    fn a_final_field_has_nothing_to_set_or_chain() {
        let mut field = f("id", "long");
        field.is_final = true;
        assert!(!offers(&field, Accessor::Set));
        assert!(!offers(&field, Accessor::With));
        assert!(offers(&field, Accessor::Get), "reading it is still fine");
        assert!(offers(&f("id", "long"), Accessor::Set));
    }

    /// The wither returns the object so calls chain — the whole of what makes it one.
    #[test]
    fn a_wither_assigns_and_returns_the_object() {
        assert_eq!(accessor_name(&f("customer", "String"), Accessor::With), "withCustomer");
        assert_eq!(
            render_accessor(&f("customer", "String"), Accessor::With, "    "),
            "public Order withCustomer(String customer) {\n        this.customer = customer;\n        return this;\n    }"
        );
    }

    /// A `static` member has no `this` to return.
    #[test]
    fn a_static_field_is_offered_no_wither() {
        let mut field = f("count", "int");
        field.is_static = true;
        assert!(!offers(&field, Accessor::With));
    }

    /// Named for the PROPERTY, like the setter: a boolean `isActive` chains as `withActive`.
    #[test]
    fn a_wither_takes_the_property_name() {
        assert_eq!(accessor_name(&f("isActive", "boolean"), Accessor::With), "withActive");
    }

    /// A generic type is copied verbatim: the accessor should read like the declaration it is for.
    #[test]
    fn a_generic_type_is_written_as_declared() {
        let g = render_accessor(&f("orders", "List<Order>"), Accessor::Get, "    ");
        assert!(g.starts_with("public List<Order> getOrders()"), "{g}");
    }
}
