//! A stopped JVM, read over JDWP, as a [`DebugTarget`] for the shared expression language.
//!
//! `bennu-debug-expr` owns what an expression *means*; this owns how to read a value out of a
//! suspended VM: a local out of a stack frame, a field out of an object, an element out of an
//! array, a **static field** (and so an enum constant) out of a loaded class, a string's text, the
//! primitive inside an `Integer`. Every read goes through the same helpers the variables panel uses
//! ([`crate::debug_value`]), so a watch and the tree cannot disagree about what a field holds.
//!
//! ## `Type.CONSTANT`
//!
//! The VM knows classes by binary name (`com.acme.Order$Status`); the user wrote `Status.ACTIVE`.
//! `bennu_debug_expr::java_names` turns the name as typed into candidate binary names using the
//! frame's file (its imports), its package and the classes around it; each is asked of the VM
//! (`ClassesBySignature`) and the first **loaded** class that declares the member wins. A class that
//! is not loaded yet is "unknown", not an error — the evaluator says the class may not be loaded —
//! and one that is loaded but not initialized has no static values to read, which is said too.
//! Constants a class inherits from an *interface* are not found: reading them would need
//! `ReferenceType.Interfaces`, which the transport does not have yet.
//!
//! ## What it never does
//!
//! Call a method. `toString()` of an object, `list.size()`, `equals` — each would run application
//! code inside a paused program, so the language refuses them by name before they get here.

use std::cell::OnceCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use bennu_debug_expr::prelude::{
    candidate_classes, eval, parse_expr, parse_imports, static_import_owners, DebugTarget, Expr,
    JavaImports, Prim, Val, JAVA,
};
use bennu_jdwp::prelude::{
    class_signature, classes_by_signature, object_type, string_value, ClassRef, Frame,
    Id, Tag, Value,
};
use bennu_proto::prelude::DebugValue;

use crate::debug::Session;
use crate::debug_path::Step;
use crate::debug_value;

/// `INITIALIZED` in a class status: statics have their values only from here on.
const CLASS_INITIALIZED: i32 = 4;

/// An object the VM knows, identified by its id. The tag rides along (a string, an array) because a
/// row built from it later needs it, but two handles are the same object when their ids are.
#[derive(Debug, Clone, Copy)]
pub(crate) struct JRef {
    id: Id,
    tag: Tag,
}

impl PartialEq for JRef {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl JRef {
    fn value(self) -> Value {
        Value::Object { tag: self.tag, id: self.id }
    }
}

/// A JDWP value as the evaluator sees it. A `float` becomes a `double` (what Java promotes it to
/// the moment it is compared or computed with), a `byte` / `short` an `int`.
fn lift(value: Value) -> Val<JRef> {
    match value {
        Value::Boolean(b) => Val::Prim(Prim::Bool(b)),
        Value::Byte(v) => Val::Prim(Prim::Int(i32::from(v))),
        Value::Short(v) => Val::Prim(Prim::Int(i32::from(v))),
        Value::Int(v) => Val::Prim(Prim::Int(v)),
        Value::Long(v) => Val::Prim(Prim::Long(v)),
        Value::Char(c) => Val::Prim(Prim::Char(u32::from(c))),
        Value::Float(v) => Val::Prim(Prim::Double(f64::from(v))),
        Value::Double(v) => Val::Prim(Prim::Double(v)),
        Value::Object { id: 0, .. } => Val::Null,
        Value::Object { tag, id } => Val::Ref(JRef { id, tag }),
        // Never the value of a field or a slot; null is the reading that cannot mislead.
        Value::Void => Val::Null,
    }
}

/// The frame of a stopped thread, read through JDWP.
pub(crate) struct JdwpTarget<'a> {
    session: &'a Session,
    thread: Id,
    frame: Frame,
    /// The imports of the frame's file, read the first time a name turns out not to be a variable.
    imports: OnceCell<Arc<JavaImports>>,
}

impl<'a> JdwpTarget<'a> {
    pub(crate) fn new(session: &'a Session, thread: Id, frame: Frame) -> Self {
        JdwpTarget { session, thread, frame, imports: OnceCell::new() }
    }

    /// The binary name of the class the frame is executing in.
    fn current_class(&self) -> String {
        self.session.class_name_of(self.frame.location.class)
    }

    fn imports(&self) -> Arc<JavaImports> {
        self.imports.get_or_init(|| imports_of(self.session, &self.current_class())).clone()
    }

    /// The loaded class named `binary_name`, if the VM has it.
    fn loaded(&self, binary_name: &str) -> Option<ClassRef> {
        classes_by_signature(&self.session.client, &class_signature(binary_name))
            .ok()
            .and_then(|found| found.into_iter().next())
    }

    /// `member` as a static field of `class`: `Ok(None)` when it declares none by that name.
    fn static_in(&self, class: ClassRef, member: &str) -> Result<Option<Val<JRef>>, String> {
        let Some(value) = debug_value::static_field_of(self.session, class.id, member)? else {
            return Ok(None);
        };
        if class.status & CLASS_INITIALIZED == 0 {
            return Err(format!(
                "`{}` is loaded but not initialized yet, so its static `{member}` has no value to read",
                self.session.class_name_of(class.id)
            ));
        }
        Ok(Some(lift(value)))
    }

    /// Every binary name `qualifier` could be: through the file's imports first, then any project
    /// class with that simple name.
    fn candidates(&self, qualifier: &[String]) -> Vec<String> {
        let mut names = candidate_classes(&self.imports(), &self.current_class(), qualifier);
        if let Some((head, rest)) = qualifier.split_first() {
            let nested: String = rest.iter().map(|segment| format!("${segment}")).collect();
            for known in self.session.project_class_names() {
                if known.rsplit(['.', '$']).next() == Some(head.as_str()) {
                    names.push(format!("{known}{nested}"));
                }
            }
        }
        names
    }
}

impl DebugTarget for JdwpTarget<'_> {
    type Ref = JRef;

    fn variable(&self, name: &str) -> Result<Option<Val<JRef>>, String> {
        if let Some(value) =
            debug_value::root_variable(self.session, self.thread, &self.frame, name)?
        {
            return Ok(Some(lift(value)));
        }
        // `import static com.acme.Limits.MAX;` makes a bare `MAX` mean that field.
        for owner in static_import_owners(&self.imports(), name) {
            if let Some(class) = self.loaded(&owner) {
                if let Some(found) = self.static_in(class, name)? {
                    return Ok(Some(found));
                }
            }
        }
        Ok(None)
    }

    fn field(&self, owner: &JRef, name: &str, _of: &str) -> Result<Val<JRef>, String> {
        debug_value::follow(self.session, owner.value(), &Step::Field(name.to_string())).map(lift)
    }

    fn index(&self, owner: &JRef, at: i64, _of: &str) -> Result<Val<JRef>, String> {
        let at = i32::try_from(at).map_err(|_| format!("index {at} is outside any array"))?;
        debug_value::follow(self.session, owner.value(), &Step::Index(at)).map(lift)
    }

    fn deref(&self, _pointer: &JRef, _of: &str) -> Result<Val<JRef>, String> {
        Err("there is no `*` in a Java expression".to_string())
    }

    fn static_field(&self, qualifier: &[String], name: &str) -> Result<Option<Val<JRef>>, String> {
        let key = format!("{}|{}.{name}", self.current_class(), qualifier.join("."));
        let remembered = self
            .session
            .cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .resolved_types
            .get(&key)
            .copied();
        if let Some(class) = remembered {
            if let Some(found) = self.static_in(class, name)? {
                return Ok(Some(found));
            }
        }
        for candidate in self.candidates(qualifier) {
            let Some(class) = self.loaded(&candidate) else { continue };
            if let Some(found) = self.static_in(class, name)? {
                self.session
                    .cache
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .resolved_types
                    .insert(key, class);
                return Ok(Some(found));
            }
        }
        Ok(None)
    }

    fn text(&self, of: &JRef) -> Result<Option<String>, String> {
        if of.tag != Tag::String {
            return Ok(None);
        }
        string_value(&self.session.client, of.id).map(Some).map_err(|e| e.to_string())
    }

    fn unbox(&self, of: &JRef) -> Result<Option<Prim>, String> {
        if of.tag != Tag::Object {
            return Ok(None);
        }
        let class = object_type(&self.session.client, of.id).map_err(|e| e.to_string())?;
        if class.type_tag != 1 || !is_box(&self.session.class_name_of(class.id)) {
            return Ok(None);
        }
        let inner = debug_value::follow(self.session, of.value(), &Step::Field("value".to_string()))?;
        Ok(match lift(inner) {
            Val::Prim(p) => Some(p),
            _ => None,
        })
    }
}

/// `java.lang.Integer` and its seven siblings — the classes `count > 3` unboxes.
fn is_box(class: &str) -> bool {
    matches!(
        class,
        "java.lang.Integer"
            | "java.lang.Long"
            | "java.lang.Short"
            | "java.lang.Byte"
            | "java.lang.Character"
            | "java.lang.Boolean"
            | "java.lang.Double"
            | "java.lang.Float"
    )
}

// ── imports of the frame's file ─────────────────────────────────────────────────

type ImportsCache = Mutex<HashMap<String, (SystemTime, Arc<JavaImports>)>>;

/// The imports of the file declaring the (top-level class of the) frame's class, cached by path and
/// modification time. Read only when a name is not a variable, so a plain condition never touches
/// the filesystem; empty for a library class, whose source the project does not have.
fn imports_of(session: &Session, current_class: &str) -> Arc<JavaImports> {
    static CACHE: OnceLock<ImportsCache> = OnceLock::new();
    let top_level = current_class.split('$').next().unwrap_or(current_class);
    let Some(file) = session.declaring_file(top_level) else { return Arc::default() };
    let modified = std::fs::metadata(file).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some((stamp, imports)) = cache.lock().unwrap_or_else(|p| p.into_inner()).get(file) {
        if *stamp == modified {
            return imports.clone();
        }
    }
    let imports = Arc::new(
        std::fs::read_to_string(file).map(|source| parse_imports(&source)).unwrap_or_default(),
    );
    cache
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(file.to_string(), (modified, imports.clone()));
    imports
}

// ── watches ─────────────────────────────────────────────────────────────────────

/// A watch expression read in a named frame, as the row the panel shows.
///
/// A plain path whose first name is a variable is walked exactly as before — the row keeps the
/// declared shape (`float`, an expandable object). Anything else is evaluated by the shared
/// language, and the row says what the result is.
pub(crate) fn watch_row(
    session: &Session,
    thread: Id,
    frame: &Frame,
    expression: &str,
) -> Result<DebugValue, String> {
    let expr = parse_expr(expression, JAVA)?;
    read(session, thread, frame, &expr, expression)
}

/// Evaluate a parsed expression in a named frame and describe the result as a row called `name`.
///
/// Separate from [`watch_row`] because a breakpoint condition lists what it read (`total = 250`) in
/// a frame the session has not announced a pause for — see [`debug_value::walk_path`].
pub(crate) fn read(
    session: &Session,
    thread: Id,
    frame: &Frame,
    expr: &Expr,
    name: &str,
) -> Result<DebugValue, String> {
    if let Some(why) = expr.refusal(JAVA) {
        return Err(why);
    }
    if let Some(steps) = expr.as_path() {
        let root_is_variable = match steps.first() {
            Some(Step::Field(root)) => {
                debug_value::root_variable(session, thread, frame, root)?.is_some()
            }
            _ => false,
        };
        if root_is_variable {
            let value = debug_value::walk_path(session, thread, frame, &steps)?;
            return Ok(debug_value::row(session, name, "watch", value));
        }
    }
    let target = JdwpTarget::new(session, thread, *frame);
    let value = eval(expr, &target, JAVA)?;
    Ok(row_of(session, name, value))
}

/// The row for a computed result.
fn row_of(session: &Session, name: &str, value: Val<JRef>) -> DebugValue {
    let typed = |value: Value, type_name: &str| {
        let mut row = debug_value::row(session, name, "watch", value);
        row.type_name = type_name.to_string();
        row
    };
    match value {
        Val::Prim(Prim::Bool(b)) => typed(Value::Boolean(b), "boolean"),
        Val::Prim(Prim::Char(c)) => typed(Value::Char(c as u16), "char"),
        Val::Prim(Prim::Int(v)) => typed(Value::Int(v), "int"),
        Val::Prim(Prim::Long(v)) => typed(Value::Long(v), "long"),
        Val::Prim(Prim::Double(v)) => typed(Value::Double(v), "double"),
        Val::Str(text) => DebugValue {
            name: name.to_string(),
            kind: "watch".to_string(),
            type_name: "String".to_string(),
            value: debug_value::quote(&text),
            object: None,
        },
        Val::Null => debug_value::row(session, name, "watch", Value::Object { tag: Tag::Object, id: 0 }),
        Val::Ref(r) => debug_value::row(session, name, "watch", r.value()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_jdwp_value_is_lifted_to_the_widest_type_java_would_compute_in() {
        assert_eq!(lift(Value::Byte(-3)), Val::Prim(Prim::Int(-3)));
        assert_eq!(lift(Value::Short(7)), Val::Prim(Prim::Int(7)));
        assert_eq!(lift(Value::Char(65)), Val::Prim(Prim::Char(65)));
        assert_eq!(lift(Value::Float(0.5)), Val::Prim(Prim::Double(0.5)));
        assert_eq!(lift(Value::Long(1 << 40)), Val::Prim(Prim::Long(1 << 40)));
        assert_eq!(lift(Value::Object { tag: Tag::Object, id: 0 }), Val::Null);
    }

    /// Two references are the same object when their ids are, whatever tag each was read with.
    #[test]
    fn two_handles_are_one_object_when_their_ids_match() {
        let a = JRef { id: 9, tag: Tag::Object };
        let b = JRef { id: 9, tag: Tag::String };
        assert_eq!(a, b);
        assert_ne!(a, JRef { id: 10, tag: Tag::Object });
    }

    #[test]
    fn only_the_eight_boxes_unbox() {
        for name in [
            "java.lang.Integer",
            "java.lang.Long",
            "java.lang.Short",
            "java.lang.Byte",
            "java.lang.Character",
            "java.lang.Boolean",
            "java.lang.Double",
            "java.lang.Float",
        ] {
            assert!(is_box(name), "{name}");
        }
        assert!(!is_box("java.math.BigDecimal"));
        assert!(!is_box("java.lang.Number"));
        assert!(!is_box("com.acme.Integer"));
    }
}
