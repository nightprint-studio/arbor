//! Changing the program under debug: throw an exception in the stopped thread, return early from the method it
//! is in, pop frames, write a variable or a field.
//!
//! A child of [`crate::debug`] (declared there with `#[path]`) because it works on the session's private state — the
//! paused thread and its frames — and the JDWP session is the only backend that can do any of it. The
//! assistant is the only caller, behind its own family switch and an approval per conversation; this module
//! just does the work and says plainly when the VM cannot (a capability it does not have, a value it cannot
//! write).
//!
//! What can be written is deliberately small: primitives, `String` and `null` (see
//! [`bennu_jdwp::mutate::typed_value`]). A new exception is the one place an object is built — by running
//! its constructor in the program — and the caller says so.

use bennu_jdwp::prelude::{
    capabilities, create_string, force_early_return, frame_this, frames, new_instance, object_type, parse_literal, pop_frames,
    return_descriptor, set_field_value, set_frame_value, stop_thread, superclass, typed_value, Tag, Typed, Value,
};
use bennu_proto::prelude::DebugValue;

use super::{Frame, Id, Session, EVT_DEBUG_PAUSED, MAX_FRAMES};
use crate::debug_path::{self, Step as PathStep};

/// How far up a class chain to look for a field or for `Throwable`.
const MAX_SUPERS: usize = 30;

/// One change the assistant asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Mutation {
    /// Create `class` (running its constructor) and throw it from the line the thread is stopped on.
    Throw { class: String, message: Option<String> },
    /// The method on top of the stack returns `value` now (empty for `void`).
    ForceReturn { value: String },
    /// Discard frame `frame` and the ones above it.
    DropFrame { frame: usize },
    /// Write `value` to the variable or field at `path`, in `frame`.
    SetValue { frame: usize, path: String, value: String },
}

/// What a change did, in words, with the value before and after when it was a write.
#[derive(Debug, Clone, Default)]
pub(crate) struct Mutated {
    pub summary: String,
    pub before: Option<DebugValue>,
    pub after: Option<DebugValue>,
}

impl Mutated {
    fn said(summary: impl Into<String>) -> Self {
        Mutated { summary: summary.into(), ..Mutated::default() }
    }
}

fn vm<T>(r: bennu_jdwp::prelude::Result<T>) -> Result<T, String> {
    r.map_err(|e| e.to_string())
}

impl Session {
    /// Apply a change to the program, which must be stopped.
    pub(crate) fn mutate(&self, op: &Mutation) -> Result<Mutated, String> {
        let (thread, stack) = {
            let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            let paused = state.paused.as_ref().ok_or("the program is not stopped, so there is nothing to change")?;
            (paused.thread, paused.frames.clone())
        };
        match op {
            Mutation::Throw { class, message } => self.throw(thread, class, message.as_deref()),
            Mutation::ForceReturn { value } => self.force_return(thread, &stack, value),
            Mutation::DropFrame { frame } => self.drop_frame(thread, &stack, *frame),
            Mutation::SetValue { frame, path, value } => self.set_value(thread, &stack, *frame, path, value),
        }
    }

    fn throw(&self, thread: Id, class: &str, message: Option<&str>) -> Result<Mutated, String> {
        let signature = bennu_jdwp::prelude::class_signature(class);
        let loaded = vm(bennu_jdwp::prelude::classes_by_signature(&self.client, &signature))?;
        let class_ref = loaded.first().ok_or_else(|| format!("`{class}` is not loaded in the program. Use an exception class it has already used (java.lang.IllegalStateException, java.io.IOException…)"))?;
        self.ensure_throwable(class_ref.id, class)?;
        let wanted = if message.is_some() { "(Ljava/lang/String;)V" } else { "()V" };
        let constructor = self
            .methods_of(class_ref.id)
            .into_iter()
            .find(|m| m.name == "<init>" && m.signature == wanted)
            .ok_or_else(|| format!("`{class}` has no constructor taking {}", if message.is_some() { "a single String" } else { "no arguments" }))?;
        let args = match message {
            Some(text) => vec![Value::Object { tag: Tag::String, id: vm(create_string(&self.client, text))? }],
            None => Vec::new(),
        };
        // This runs the exception's constructor inside the program.
        let made = vm(new_instance(&self.client, class_ref.id, thread, constructor.id, &args))?;
        if made.exception != 0 || made.object == 0 {
            return Err(format!("the constructor of `{class}` threw by itself, so nothing was thrown"));
        }
        vm(stop_thread(&self.client, thread, made.object))?;
        Ok(Mutated::said(format!("`{class}` was created in the program and will be thrown from the line it is stopped on as soon as it resumes")))
    }

    fn ensure_throwable(&self, class: Id, name: &str) -> Result<(), String> {
        let mut current = class;
        for _ in 0..MAX_SUPERS {
            if current == 0 {
                break;
            }
            if self.signature_of(current) == "Ljava/lang/Throwable;" {
                return Ok(());
            }
            current = superclass(&self.client, current).unwrap_or(0);
        }
        Err(format!("`{name}` is not a Throwable"))
    }

    fn force_return(&self, thread: Id, stack: &[Frame], value: &str) -> Result<Mutated, String> {
        if !vm(capabilities(&self.client))?.can_force_early_return {
            return Err("this VM cannot return early from a method (it lacks the canForceEarlyReturn capability)".into());
        }
        let top = stack.first().ok_or("the stack is empty")?;
        let method = self
            .methods_of(top.location.class)
            .into_iter()
            .find(|m| m.id == top.location.method)
            .ok_or("could not find the method on top of the stack")?;
        let descriptor = return_descriptor(&method.signature).to_string();
        let literal = parse_literal(value)?;
        let ready = self.materialize(typed_value(&descriptor, &literal)?)?;
        vm(force_early_return(&self.client, thread, &ready))?;
        let shown = if descriptor == "V" { "void".to_string() } else { value.trim().to_string() };
        Ok(Mutated::said(format!("`{}` will return {shown} as soon as the program resumes; the rest of its body (finally blocks included) does not run", method.name)))
    }

    fn drop_frame(&self, thread: Id, stack: &[Frame], index: usize) -> Result<Mutated, String> {
        if !vm(capabilities(&self.client))?.can_pop_frames {
            return Err("this VM cannot drop frames (it lacks the canPopFrames capability)".into());
        }
        if index + 1 >= stack.len() {
            return Err(format!("frame {index} is the outermost one: there is no caller to go back to"));
        }
        let frame = stack.get(index).ok_or_else(|| format!("there is no frame {index} (the stack has {})", stack.len()))?;
        vm(pop_frames(&self.client, thread, frame.id))?;
        // The stack changed under the panel: read it again and tell it.
        self.restack(thread)?;
        Ok(Mutated::said(format!("dropped frames 0..={index}; the program is back at the call in frame {} and will run it again when it resumes. Side effects of what ran are NOT undone", index + 1)))
    }

    fn set_value(&self, thread: Id, stack: &[Frame], index: usize, path: &str, value: &str) -> Result<Mutated, String> {
        let frame = *stack.get(index).ok_or_else(|| format!("there is no frame {index} (the stack has {})", stack.len()))?;
        let steps = debug_path::parse(path, debug_path::JAVA)?;
        let literal = parse_literal(value)?;
        let before = crate::debug_value::watch(self, index, path).ok();
        match steps.as_slice() {
            [PathStep::Field(name)] => self.write_name(thread, &frame, name, &literal)?,
            [rest @ .., PathStep::Field(name)] => {
                let owner = crate::debug_value::walk_path(self, thread, &frame, rest)?;
                self.write_field(owner, name, &literal)?;
            }
            _ => return Err("only a variable or a field can be written, not an array element".into()),
        }
        let after = crate::debug_value::watch(self, index, path).ok();
        Ok(Mutated { summary: format!("`{path}` was written"), before, after })
    }

    /// A bare name: a local of the frame, else a field of `this`.
    fn write_name(&self, thread: Id, frame: &Frame, name: &str, literal: &bennu_jdwp::prelude::Literal) -> Result<(), String> {
        let table = self.variables_of(frame.location.class, frame.location.method);
        if let Some(local) = table.iter().find(|l| l.name == name && l.in_scope(frame.location.index)) {
            let ready = self.materialize(typed_value(&local.signature, literal)?)?;
            return vm(set_frame_value(&self.client, thread, frame.id, local.slot, &ready));
        }
        let this = vm(frame_this(&self.client, thread, frame.id))?;
        if this.is_null() {
            return Err(format!("no variable named `{name}` here"));
        }
        self.write_field(this, name, literal)
    }

    fn write_field(&self, owner: Value, name: &str, literal: &bennu_jdwp::prelude::Literal) -> Result<(), String> {
        let Value::Object { id, .. } = owner else { return Err("that is a primitive: there are no fields to write".into()) };
        if id == 0 {
            return Err("that is null: there are no fields to write".into());
        }
        let class = vm(object_type(&self.client, id))?;
        let mut current = class.id;
        for _ in 0..MAX_SUPERS {
            if current == 0 {
                break;
            }
            if let Some(field) = self.fields_of(current).into_iter().find(|f| f.name == name) {
                if field.is_static() {
                    return Err(format!("`{name}` is static: only instance fields can be written"));
                }
                if field.mod_bits & 0x0010 != 0 {
                    return Err(format!("`{name}` is final"));
                }
                let ready = self.materialize(typed_value(&field.signature, literal)?)?;
                return vm(set_field_value(&self.client, id, field.id, &ready));
            }
            current = superclass(&self.client, current).unwrap_or(0);
        }
        Err(format!("no field named `{name}` on {}", super::simple_name(&self.class_name_of(class.id))))
    }

    /// A typed literal as a value, creating the `String` in the VM when it is one.
    fn materialize(&self, typed: Typed) -> Result<Value, String> {
        match typed {
            Typed::Ready(v) => Ok(v),
            Typed::String(text) => Ok(Value::Object { tag: Tag::String, id: vm(create_string(&self.client, &text))? }),
        }
    }

    /// The stack changed (frames were dropped): read it again, store it, and tell the panel.
    fn restack(&self, thread: Id) -> Result<(), String> {
        let stack = vm(frames(&self.client, thread))?;
        let cut: Vec<Frame> = stack.into_iter().take(MAX_FRAMES).collect();
        let described: Vec<_> = cut.iter().enumerate().map(|(i, f)| self.describe(i as u32, f)).collect();
        let pause = {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            let Some(paused) = state.paused.as_mut() else { return Ok(()) };
            paused.frames = cut;
            paused.pause.frames = described;
            paused.pause.clone()
        };
        self.sink.emit(EVT_DEBUG_PAUSED, serde_json::to_value(pause).unwrap_or_default());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mutation_compares_by_what_it_asks() {
        let a = Mutation::DropFrame { frame: 1 };
        assert_eq!(a, Mutation::DropFrame { frame: 1 });
        assert_ne!(a, Mutation::DropFrame { frame: 2 });
    }

    #[test]
    fn a_summary_is_carried_without_values_when_it_was_not_a_write() {
        let m = Mutated::said("done");
        assert_eq!(m.summary, "done");
        assert!(m.before.is_none() && m.after.is_none());
    }
}
