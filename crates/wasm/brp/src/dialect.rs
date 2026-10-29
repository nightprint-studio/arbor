//! Which BRP method names a running game actually answers to.
//!
//! The protocol's method names are not stable across Bevy releases: 0.15/0.16 spoke `bevy/query`,
//! `bevy/get`, `bevy/destroy`; 0.17 moved everything under `world.*` and renamed the verbs
//! (`world.despawn_entity`, `world.get_components`, …). Hard-coding one family silently breaks the
//! other, and guessing from a version number needs the version to be known — which it is not, on
//! the wire. What *is* on the wire is `rpc.discover`, so the answer is read from there at connect
//! time: for each logical [`Op`] the first candidate the game lists wins.
//!
//! When discovery gave nothing (the probe failed soft), the newest family is assumed — the
//! failure then surfaces on the first real call as an honest "method not found" instead of a
//! feature quietly disappearing.

/// One thing a client wants to ask a game to do, independent of what the game calls it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    Query,
    ListComponents,
    GetComponents,
    Spawn,
    Despawn,
    Insert,
    Remove,
    Mutate,
    Reparent,
    GetComponentsWatch,
    ListComponentsWatch,
    ListResources,
    GetResource,
    InsertResource,
    RemoveResource,
    MutateResource,
    Schema,
    TriggerEvent,
}

impl Op {
    pub const ALL: [Op; 18] = [
        Op::Query,
        Op::ListComponents,
        Op::GetComponents,
        Op::Spawn,
        Op::Despawn,
        Op::Insert,
        Op::Remove,
        Op::Mutate,
        Op::Reparent,
        Op::GetComponentsWatch,
        Op::ListComponentsWatch,
        Op::ListResources,
        Op::GetResource,
        Op::InsertResource,
        Op::RemoveResource,
        Op::MutateResource,
        Op::Schema,
        Op::TriggerEvent,
    ];

    /// Method names for this op, newest family first. Only names a Bevy release has really
    /// shipped: a plural `despawn_entities` was once guessed here and never existed server-side.
    fn candidates(self) -> &'static [&'static str] {
        match self {
            Op::Query => &["world.query", "bevy/query"],
            Op::ListComponents => &["world.list_components", "bevy/list"],
            Op::GetComponents => &["world.get_components", "bevy/get"],
            Op::Spawn => &["world.spawn_entity", "bevy/spawn"],
            Op::Despawn => &["world.despawn_entity", "bevy/destroy"],
            Op::Insert => &["world.insert_components", "bevy/insert"],
            Op::Remove => &["world.remove_components", "bevy/remove"],
            Op::Mutate => &["world.mutate_components", "bevy/mutate_component"],
            Op::Reparent => &["world.reparent_entities", "bevy/reparent"],
            Op::GetComponentsWatch => &["world.get_components+watch", "bevy/get+watch"],
            Op::ListComponentsWatch => &["world.list_components+watch", "bevy/list+watch"],
            Op::ListResources => &["world.list_resources", "bevy/list_resources"],
            Op::GetResource => &["world.get_resources", "bevy/get_resource"],
            Op::InsertResource => &["world.insert_resources", "bevy/insert_resource"],
            Op::RemoveResource => &["world.remove_resources", "bevy/remove_resource"],
            Op::MutateResource => &["world.mutate_resources", "bevy/mutate_resource"],
            Op::Schema => &["registry.schema", "bevy/registry/schema"],
            Op::TriggerEvent => &["world.trigger_event"],
        }
    }
}

/// The resolved method names of one connected game.
#[derive(Debug, Clone, Default)]
pub struct MethodMap {
    resolved: Vec<(Op, String)>,
    /// `true` when the names came from the game's own `rpc.discover` rather than the fallback.
    discovered: bool,
}

impl MethodMap {
    /// Resolve every op against the method list `rpc.discover` returned. An empty list means the
    /// probe produced nothing, and the newest family is assumed for every op.
    pub fn resolve(discovered: &[String]) -> Self {
        let mut resolved = Vec::new();
        for op in Op::ALL {
            let hit = if discovered.is_empty() {
                op.candidates().first().copied()
            } else {
                op.candidates()
                    .iter()
                    .copied()
                    .find(|c| discovered.iter().any(|d| d == c))
            };
            if let Some(name) = hit {
                resolved.push((op, name.to_string()));
            }
        }
        Self { resolved, discovered: !discovered.is_empty() }
    }

    /// The method name for `op`, or `None` when this game does not offer it.
    pub fn get(&self, op: Op) -> Option<&str> {
        self.resolved.iter().find(|(o, _)| *o == op).map(|(_, n)| n.as_str())
    }

    pub fn supports(&self, op: Op) -> bool {
        self.get(op).is_some()
    }

    pub fn was_discovered(&self) -> bool {
        self.discovered
    }

    /// `"world"` (0.17+), `"bevy"` (0.15/0.16) or `"unknown"` — for the status line only; nothing
    /// branches on it.
    pub fn family(&self) -> &'static str {
        match self.get(Op::Query) {
            Some(n) if n.starts_with("world.") => "world",
            Some(n) if n.starts_with("bevy/") => "bevy",
            _ => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn world_family_is_resolved_from_discovery() {
        let m = MethodMap::resolve(&names(&[
            "world.query",
            "world.despawn_entity",
            "world.get_components",
            "world.get_components+watch",
        ]));
        assert_eq!(m.get(Op::Despawn), Some("world.despawn_entity"));
        assert_eq!(m.get(Op::GetComponentsWatch), Some("world.get_components+watch"));
        assert_eq!(m.family(), "world");
        // Not listed → not offered. No guess.
        assert!(!m.supports(Op::Spawn));
    }

    #[test]
    fn legacy_family_is_resolved_from_discovery() {
        let m = MethodMap::resolve(&names(&["bevy/query", "bevy/destroy", "bevy/get"]));
        assert_eq!(m.get(Op::Despawn), Some("bevy/destroy"));
        assert_eq!(m.get(Op::GetComponents), Some("bevy/get"));
        assert_eq!(m.family(), "bevy");
    }

    #[test]
    fn empty_discovery_assumes_the_newest_family() {
        let m = MethodMap::resolve(&[]);
        assert!(!m.was_discovered());
        assert_eq!(m.get(Op::Query), Some("world.query"));
        assert_eq!(m.get(Op::Despawn), Some("world.despawn_entity"));
    }

    #[test]
    fn the_newest_family_wins_when_both_are_listed() {
        let m = MethodMap::resolve(&names(&["bevy/query", "world.query"]));
        assert_eq!(m.get(Op::Query), Some("world.query"));
    }
}
