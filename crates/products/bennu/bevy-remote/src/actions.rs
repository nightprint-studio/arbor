//! Named actions a game registers so a remote client can trigger them: `start_new_game`, `spawn_wave`,
//! `toggle_debug_overlay`. The game owns what an action does; the registry only maps a name to a closure
//! that gets the `World` and the client's optional JSON argument.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde_json::Value;

type Handler = Box<dyn Fn(&mut World, Option<Value>) -> Result<Value, String> + Send + Sync>;

struct Entry {
    description: String,
    handler: Handler,
}

/// The actions registered with [`RemoteActionsExt::register_remote_action`].
#[derive(Resource, Default)]
pub struct RemoteActions {
    entries: BTreeMap<String, Entry>,
}

impl RemoteActions {
    pub fn insert(
        &mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        handler: impl Fn(&mut World, Option<Value>) -> Result<Value, String> + Send + Sync + 'static,
    ) {
        self.entries.insert(name.into(), Entry { description: description.into(), handler: Box::new(handler) });
    }

    /// `(name, description)` of every action, alphabetical.
    pub fn list(&self) -> Vec<(&str, &str)> {
        self.entries.iter().map(|(n, e)| (n.as_str(), e.description.as_str())).collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }
}

/// Run `name` with `arg`. The registry is taken out of the world for the call, so an action may itself
/// register another (and the handler gets `&mut World` without a double borrow).
pub fn run_action(world: &mut World, name: &str, arg: Option<Value>) -> Result<Value, String> {
    let Some(registry) = world.remove_resource::<RemoteActions>() else {
        return Err("no actions are registered".into());
    };
    let result = match registry.entries.get(name) {
        Some(entry) => (entry.handler)(world, arg),
        None => {
            let known: Vec<&str> = registry.entries.keys().map(String::as_str).collect();
            Err(format!("unknown action `{name}`; registered: [{}]", known.join(", ")))
        }
    };
    // An action that registered more actions inserted a fresh registry: fold ours back into it.
    match world.remove_resource::<RemoteActions>() {
        Some(mut added) => {
            added.entries.extend(registry.entries);
            world.insert_resource(added);
        }
        None => world.insert_resource(registry),
    }
    result
}

/// `app.register_remote_action(...)`.
pub trait RemoteActionsExt {
    fn register_remote_action(
        &mut self,
        name: &str,
        description: &str,
        handler: impl Fn(&mut World, Option<Value>) -> Result<Value, String> + Send + Sync + 'static,
    ) -> &mut Self;
}

impl RemoteActionsExt for App {
    fn register_remote_action(
        &mut self,
        name: &str,
        description: &str,
        handler: impl Fn(&mut World, Option<Value>) -> Result<Value, String> + Send + Sync + 'static,
    ) -> &mut Self {
        self.world_mut().get_resource_or_insert_with(RemoteActions::default).insert(name, description, handler);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Counter(u32);

    #[test]
    fn an_action_runs_with_the_world_and_its_argument() {
        let mut app = App::new();
        app.init_resource::<Counter>().register_remote_action("bump", "adds the argument", |world, arg| {
            let by = arg.and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            world.resource_mut::<Counter>().0 += by;
            Ok(serde_json::json!(world.resource::<Counter>().0))
        });
        let world = app.world_mut();
        assert_eq!(run_action(world, "bump", Some(serde_json::json!(4))), Ok(serde_json::json!(4)));
        assert_eq!(run_action(world, "bump", None), Ok(serde_json::json!(5)));
        assert!(world.resource::<RemoteActions>().contains("bump"));
    }

    #[test]
    fn an_unknown_action_lists_what_exists() {
        let mut app = App::new();
        app.register_remote_action("a", "", |_, _| Ok(Value::Null)).register_remote_action("b", "", |_, _| Ok(Value::Null));
        let err = run_action(app.world_mut(), "c", None).unwrap_err();
        assert!(err.contains("unknown action `c`") && err.contains("a, b"), "{err}");
    }

    #[test]
    fn the_list_is_alphabetical_with_descriptions() {
        let mut app = App::new();
        app.register_remote_action("z", "last", |_, _| Ok(Value::Null)).register_remote_action("a", "first", |_, _| Ok(Value::Null));
        let registry = app.world().resource::<RemoteActions>();
        assert_eq!(registry.list(), vec![("a", "first"), ("z", "last")]);
    }
}
