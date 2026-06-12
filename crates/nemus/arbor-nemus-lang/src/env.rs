//! Lexical environments: a persistent chain of scopes.
//!
//! Cloning an [`Env`] is cheap (a refcount bump) so closures can capture their
//! defining scope and calls can push a child frame without copying. The chain
//! holds **locals only** — `fn`/lambda parameters and captured lexical scopes.
//! Top-level bindings live in a separate globals env the evaluator consults as a
//! fallback, so a function never holds a reference back to the globals that hold
//! it (no `Rc` cycle).

use std::collections::HashMap;
use std::rc::Rc;

use crate::value::Value;

#[derive(Debug)]
struct Frame {
    vars: HashMap<String, Value>,
    parent: Option<Rc<Frame>>,
}

/// A scope chain. Empty by default; grows with [`child`](Env::child).
#[derive(Clone, Debug, Default)]
pub struct Env(Option<Rc<Frame>>);

impl Env {
    /// The empty environment.
    pub fn empty() -> Env {
        Env(None)
    }

    /// A root environment holding `vars` (used for globals).
    pub fn from_map(vars: HashMap<String, Value>) -> Env {
        Env(Some(Rc::new(Frame { vars, parent: None })))
    }

    /// A child scope with `vars` bound, this env as its parent.
    pub fn child(&self, vars: HashMap<String, Value>) -> Env {
        Env(Some(Rc::new(Frame {
            vars,
            parent: self.0.clone(),
        })))
    }

    /// Look up a name, walking outward through the chain.
    pub fn lookup(&self, name: &str) -> Option<Value> {
        let mut frame = self.0.as_deref();
        while let Some(f) = frame {
            if let Some(v) = f.vars.get(name) {
                return Some(v.clone());
            }
            frame = f.parent.as_deref();
        }
        None
    }
}
