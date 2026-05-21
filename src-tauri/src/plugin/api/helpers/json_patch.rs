//! Dotted-path walker for `arbor.fs.{json,yaml,toml}_set`.
//!
//! Accepts any of the following syntaxes:
//!   · `$.database.host`          — jq-style (leading `$.` stripped)
//!   · `database.host`            — dotted
//!   · `items.0.name`             — numeric segments → array index
//!   · `servers[1].host`          — bracket form also accepted
//!
//! Intermediate objects are created for missing string keys (numeric
//! segments on a non-array raise an error — ambiguous to auto-create).

pub(crate) fn set_json_at_path(
    root: &mut serde_json::Value,
    path: &str,
    value: serde_json::Value,
) -> std::result::Result<(), String> {
    // Normalise: strip leading `$.`, `$`, or `/`; convert `[i]` → `.i`.
    let mut norm = path.trim().to_string();
    if norm.starts_with("$.") { norm.drain(..2); }
    else if norm.starts_with('$') || norm.starts_with('/') { norm.drain(..1); }
    // `a[0].b` → `a.0.b`
    let norm = norm.replace(']', "").replace('[', ".");
    let segments: Vec<&str> = norm.split('.').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return Err("empty path".into());
    }
    walk_and_set(root, &segments, value)
}

fn walk_and_set(
    node: &mut serde_json::Value,
    segs: &[&str],
    value: serde_json::Value,
) -> std::result::Result<(), String> {
    use serde_json::Value;
    if segs.is_empty() { return Err("unreachable empty segments".into()); }
    let (head, tail) = (segs[0], &segs[1..]);
    let is_numeric = head.parse::<usize>().is_ok();

    if tail.is_empty() {
        // Leaf: set value.
        match node {
            Value::Object(map) if !is_numeric => { map.insert(head.to_string(), value); Ok(()) }
            Value::Array(arr)  if is_numeric  => {
                let idx: usize = head.parse().unwrap();
                while arr.len() <= idx { arr.push(Value::Null); }
                arr[idx] = value;
                Ok(())
            }
            Value::Null => {
                // Promote to object / array based on segment kind.
                if is_numeric {
                    let idx: usize = head.parse().unwrap();
                    let mut arr = Vec::with_capacity(idx + 1);
                    while arr.len() <= idx { arr.push(Value::Null); }
                    arr[idx] = value;
                    *node = Value::Array(arr);
                } else {
                    let mut m = serde_json::Map::new();
                    m.insert(head.to_string(), value);
                    *node = Value::Object(m);
                }
                Ok(())
            }
            _ => Err(format!("cannot set key '{head}' on {:?}", node_kind(node))),
        }
    } else {
        // Descend, auto-create missing containers.
        match node {
            Value::Object(map) if !is_numeric => {
                let child = map.entry(head.to_string()).or_insert(Value::Null);
                walk_and_set(child, tail, value)
            }
            Value::Array(arr) if is_numeric => {
                let idx: usize = head.parse().unwrap();
                while arr.len() <= idx { arr.push(Value::Null); }
                walk_and_set(&mut arr[idx], tail, value)
            }
            Value::Null => {
                if is_numeric {
                    *node = Value::Array(Vec::new());
                } else {
                    *node = Value::Object(serde_json::Map::new());
                }
                walk_and_set(node, segs, value)
            }
            _ => Err(format!("cannot descend into key '{head}' of {:?}", node_kind(node))),
        }
    }
}

fn node_kind(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null    => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}
