//! What the runtime itself holds, for every backend's `__memory` answer — and the sizing helpers a
//! product reporter would otherwise write again.
//!
//! The plugin host belongs to [`crate::App`], not to any product, so its lines are added here once
//! (see [`crate::App::run`]) rather than asked of five reporters that would each have to reach into
//! the host the same way.

use std::mem::size_of;
use std::sync::{Arc, Mutex};

use arbor_plugin_core::prelude::PluginHost;
use serde_json::Value;

use crate::dispatch::{MemoryItem, PROCESS_SCOPE};

/// One line per loaded plugin: the bytes its Lua VM has allocated.
///
/// **Exact**, which is rare in a breakdown: Lua allocates through a counting allocator, so this is
/// what the VM holds rather than an estimate of it.
///
/// The host is only *tried*: a reload or a hook fan-out holds its lock for as long as the plugins
/// take, and a report that waited for that would freeze the breakdown on the slowest plugin. A busy
/// host says so, and Refresh asks again.
pub(crate) fn plugin_items(host: &Arc<Mutex<PluginHost>>) -> Vec<MemoryItem> {
    let Ok(host) = host.try_lock() else {
        return vec![MemoryItem {
            bytes: None,
            count: None,
            ..MemoryItem::counted(PROCESS_SCOPE, "Plugins — busy right now, Refresh to measure", 0)
        }];
    };
    if host.plugins.is_empty() {
        return vec![MemoryItem::counted(PROCESS_SCOPE, "Plugins loaded", 0)];
    }
    host.plugins
        .iter()
        .map(|p| MemoryItem::exact(PROCESS_SCOPE, format!("Plugin {} — Lua", p.manifest.name), 1, p.lua.used_memory()))
        .collect()
}

/// Heap owned by a JSON value: every string's capacity, every array's slots, a slot per object
/// entry. An estimate — the map's own node overhead is not counted.
///
/// For caches held as JSON on purpose (a product state that stays free of the crate the typed value
/// lives in), where walking the value is the only way to size it that does not serialise it.
pub fn json_heap_estimate(value: &Value) -> usize {
    match value {
        Value::String(s) => s.capacity(),
        Value::Array(list) => {
            list.capacity() * size_of::<Value>() + list.iter().map(json_heap_estimate).sum::<usize>()
        }
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| size_of::<(String, Value)>() + k.capacity() + json_heap_estimate(v))
            .sum(),
        Value::Null | Value::Bool(_) | Value::Number(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_scalar_owns_no_heap_and_a_string_owns_its_text() {
        assert_eq!(json_heap_estimate(&json!(42)), 0);
        assert_eq!(json_heap_estimate(&Value::String("abcd".to_string())), 4);
    }

    #[test]
    fn containers_count_their_slots_and_their_children() {
        let value = json!({ "k": ["ab"] });
        let Value::Object(map) = &value else { unreachable!() };
        let Value::Array(list) = &map["k"] else { unreachable!() };
        let expected = size_of::<(String, Value)>() + 1 + list.capacity() * size_of::<Value>() + 2;
        assert_eq!(json_heap_estimate(&value), expected);
    }

    #[test]
    fn a_host_with_no_plugins_says_so_rather_than_answering_nothing() {
        let host = Arc::new(Mutex::new(PluginHost::new()));
        let items = plugin_items(&host);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].count, Some(0));
    }

    #[test]
    fn a_busy_host_is_not_waited_for() {
        let host = Arc::new(Mutex::new(PluginHost::new()));
        let _held = host.lock().unwrap();
        let items = plugin_items(&host);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].bytes, None);
    }
}
