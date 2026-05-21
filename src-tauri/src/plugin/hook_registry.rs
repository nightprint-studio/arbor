use mlua::{Lua, LuaSerdeExt, Table};

use crate::error::{AppError, Result};

/// Glob match between a subscription pattern and a concrete event name.
///
/// `*` matches any sequence of characters (including empty / across ':' or '.').
/// Literal strings without `*` must match exactly. This keeps the matcher
/// predictable and cheap — no regex, no segment boundaries.
pub fn matches_pattern(pattern: &str, event: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == event;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut cursor: usize = 0;

    // Anchor the first segment at the start unless the pattern begins with '*'.
    if !parts[0].is_empty() {
        if !event.starts_with(parts[0]) { return false; }
        cursor = parts[0].len();
    }

    // Each intermediate segment must appear somewhere after the cursor.
    if parts.len() >= 3 {
        for seg in &parts[1..parts.len() - 1] {
            if seg.is_empty() { continue; }
            match event[cursor..].find(seg) {
                Some(i) => cursor += i + seg.len(),
                None => return false,
            }
        }
    }

    // Anchor the last segment at the end unless the pattern ends with '*'.
    let last = parts[parts.len() - 1];
    if !last.is_empty() {
        if event.len() < cursor + last.len() { return false; }
        return event[cursor..].ends_with(last);
    }
    true
}

/// Fire a named hook in the given Lua state.
///
/// `context_json` is deserialised to a Lua table and passed as the first
/// argument to every registered handler. Handlers that return an error are
/// logged but do not stop subsequent handlers from running.
///
/// Subscription keys in `__arbor_hooks__` may contain `*` — these are matched
/// as glob patterns against the fired event name.
pub fn fire(lua: &Lua, hook: &str, context_json: &str) -> Result<()> {
    let registry: Table = lua
        .globals()
        .get("__arbor_hooks__")
        .map_err(|e| AppError::Plugin(e.to_string()))?;

    // Deserialise the JSON context string into a native Lua table so handlers
    // receive `ctx.field` instead of having to pattern-match a JSON string.
    let ctx: mlua::Value = match serde_json::from_str::<serde_json::Value>(context_json) {
        Ok(v) => lua.to_value(&v)
            .unwrap_or(mlua::Value::Table(lua.create_table().unwrap_or_else(|_| {
                lua.create_table().expect("failed to create fallback table")
            }))),
        Err(_) => mlua::Value::Table(
            lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?
        ),
    };

    // Collect matching handler lists first so we don't mutate the registry
    // mid-iteration (a handler calling arbor.events.on inside itself is legal).
    let mut matched: Vec<Table> = Vec::new();
    for pair in registry.pairs::<mlua::Value, Table>() {
        let (key, handlers) = match pair {
            Ok(kv) => kv,
            Err(_) => continue,
        };
        let pattern = match key {
            mlua::Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
            _ => continue,
        };
        if pattern.is_empty() { continue; }
        if matches_pattern(&pattern, hook) {
            matched.push(handlers);
        }
    }

    for handlers in matched {
        for pair in handlers.sequence_values::<mlua::Function>() {
            let func = match pair {
                Ok(f) => f,
                Err(_) => continue,
            };
            if let Err(e) = func.call::<mlua::Value>(ctx.clone()) {
                tracing::warn!(target: "plugin", "hook '{hook}' handler error: {e}");
            }
        }
    }

    Ok(())
}

/// Fire a hook the same way as `fire`, but capture every handler's
/// return value into the supplied collector.
///
/// Used by hooks with a "veto" semantic (e.g. `on_pre_commit`) where the
/// host needs to know whether any handler asked to abort the operation.
/// Handlers that return nothing contribute `mlua::Value::Nil` to the
/// collector. Handler errors are logged like in `fire` and treated as
/// a non-veto (the convention is: refuse to block on a buggy plugin).
pub fn fire_collecting(
    lua:          &Lua,
    hook:         &str,
    context_json: &str,
    out:          &mut Vec<mlua::Value>,
) -> Result<()> {
    let registry: Table = lua
        .globals()
        .get("__arbor_hooks__")
        .map_err(|e| AppError::Plugin(e.to_string()))?;

    let ctx: mlua::Value = match serde_json::from_str::<serde_json::Value>(context_json) {
        Ok(v) => lua.to_value(&v)
            .unwrap_or(mlua::Value::Table(lua.create_table().unwrap_or_else(|_| {
                lua.create_table().expect("failed to create fallback table")
            }))),
        Err(_) => mlua::Value::Table(
            lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?
        ),
    };

    let mut matched: Vec<Table> = Vec::new();
    for pair in registry.pairs::<mlua::Value, Table>() {
        let (key, handlers) = match pair { Ok(kv) => kv, Err(_) => continue };
        let pattern = match key {
            mlua::Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
            _ => continue,
        };
        if pattern.is_empty() { continue; }
        if matches_pattern(&pattern, hook) { matched.push(handlers); }
    }

    for handlers in matched {
        for pair in handlers.sequence_values::<mlua::Function>() {
            let func = match pair { Ok(f) => f, Err(_) => continue };
            match func.call::<mlua::Value>(ctx.clone()) {
                Ok(v)  => out.push(v),
                Err(e) => {
                    tracing::warn!(target: "plugin", "hook '{hook}' handler error: {e}");
                }
            }
        }
    }
    Ok(())
}
