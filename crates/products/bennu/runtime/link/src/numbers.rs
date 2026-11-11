//! **The world as numbers**: shaping of the answers of `bevy_game_region` and `bevy_game_props`, and the two tools that read a map **file**
//! (`map_scheda`, `map_percorso`) without any game running. Pure: no wire, no Tauri.
//!
//! The analysis itself is fulcrum's (`fulcrum-map-scan`, the same crate the game runs for a region and the Studio runs for `mappa scheda`):
//! this only turns its types into the small JSON an assistant reads. A summary is a few hundred bytes to a couple of kilobytes; a 1280×720
//! screenshot of the same map is some 1200 tokens and answers none of the questions with a number.

use std::path::{Path, PathBuf};

use fulcrum_dev_channel::message::PropsDump;
use fulcrum_map_scan::prelude::*;
use serde_json::{json, Value};

use crate::args::{MapPercorsoArgs, MapSchedaArgs};

/// Round to 3 decimals so the answer stays short.
fn r3(v: f32) -> Value {
    if !v.is_finite() {
        return Value::Null;
    }
    let x = (f64::from(v) * 1000.0).round() / 1000.0;
    json!(if x == 0.0 { 0.0 } else { x })
}

/// The parts of a summary a caller asked for, by name (empty = all).
pub fn parts_of(names: &[String]) -> Result<Vec<Part>, String> {
    names
        .iter()
        .map(|n| Part::parse(n.trim()).ok_or_else(|| format!("`{n}` is not a part of the summary: counts, layers, heights, sections, voids, marks")))
        .collect()
}

/// A summary as the compact JSON object of `mappa scheda --formato json`, with the one-sentence `sentence` added.
pub fn summary_json_value(s: &Summary) -> Value {
    let mut v = summary_value(s);
    if let Value::Object(m) = &mut v {
        m.insert("sentence".into(), json!(s.sentence));
    }
    v
}

/// `{sentence, props: [{source, name, id, cell, yaw, min, max, size}], notes}`: boxes in metres; `size` is `max − min`.
pub fn props_json(d: &PropsDump) -> Value {
    let tallest = d.entries.iter().filter(|e| e.max[1] > e.min[1]).max_by(|a, b| (a.max[1] - a.min[1]).total_cmp(&(b.max[1] - b.min[1])));
    let scripted = d.entries.iter().filter(|e| e.source == "script").count();
    let mut sentence = if d.entries.is_empty() {
        String::from("no props")
    } else {
        format!("{} props ({scripted} placed by mods, {} blocks)", d.entries.len(), d.entries.len() - scripted)
    };
    if let Some(t) = tallest {
        sentence.push_str(&format!(", tallest: {} {:.1} m", t.name, t.max[1] - t.min[1]));
    }
    let props: Vec<Value> = d
        .entries
        .iter()
        .map(|e| {
            let known = e.max != e.min;
            let mut o = json!({
                "source": e.source, "name": e.name, "cell": e.cell, "yaw": r3(e.yaw),
                "pos": [r3(e.pos[0]), r3(e.pos[1]), r3(e.pos[2])],
            });
            if !e.id.is_empty() {
                o["id"] = json!(e.id);
            }
            if known {
                o["min"] = json!([r3(e.min[0]), r3(e.min[1]), r3(e.min[2])]);
                o["max"] = json!([r3(e.max[0]), r3(e.max[1]), r3(e.max[2])]);
                o["size"] = json!([r3(e.max[0] - e.min[0]), r3(e.max[1] - e.min[1]), r3(e.max[2] - e.min[2])]);
            }
            o
        })
        .collect();
    json!({ "sentence": sentence, "props": props, "notes": d.notes })
}

// ── Map files ────────────────────────────────────────────────────────────────

fn map_path(path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path.trim());
    if !p.is_absolute() {
        return Err(format!("`{path}`: give an absolute path to a `.fmap.ron` (or to the cooked `.fmap` next to it)"));
    }
    if !(path.ends_with(".fmap.ron") || path.ends_with(".fmap")) {
        return Err(format!("`{path}` is not a map: it should end in `.fmap.ron` or `.fmap`"));
    }
    Ok(p.to_path_buf())
}

fn options(parts: &[String], cell: Option<u32>) -> Result<Options, String> {
    let mut o = Options { parts: parts_of(parts)?, ..Options::default() };
    if let Some(c) = cell {
        o.cell = c;
    }
    Ok(o)
}

/// `map_scheda`: the summary of a map file, or — with `compare` — what changed from `path` (before) to the other map or saved summary (after).
pub fn map_scheda(args: &MapSchedaArgs) -> Result<Value, String> {
    let opt = options(&args.parts, args.cell)?;
    let s = fulcrum_map_scan::doc::terrain_of_file(&map_path(&args.path)?)?.summary(&opt);
    let Some(other) = args.compare.as_deref().map(str::trim).filter(|c| !c.is_empty()) else {
        return Ok(summary_json_value(&s));
    };
    let after: Summary = if other.ends_with(".json") {
        let text = std::fs::read_to_string(other).map_err(|e| format!("{other}: {e}"))?;
        serde_json::from_str(&text).map_err(|e| format!("{other}: not a saved summary ({e})"))?
    } else {
        fulcrum_map_scan::doc::terrain_of_file(&map_path(other)?)?.summary(&opt)
    };
    let d = compare(&s, &after);
    let mut v = diff_value(&d);
    if let Value::Object(m) = &mut v {
        m.insert("sentence".into(), json!(d.sentence));
    }
    Ok(v)
}

/// `map_percorso`: can the player walk from A to B, with the game's physics; the impossible steps if not.
pub fn map_percorso(args: &MapPercorsoArgs) -> Result<Value, String> {
    let d = Motion::default();
    let m = Motion { jump: args.jump.unwrap_or(d.jump), run: args.run.unwrap_or(d.run), step: args.step.unwrap_or(d.step), pad: args.pad.unwrap_or(d.pad), height: d.height };
    if m.jump <= 0.0 || m.run <= 0.0 {
        return Err(String::from("`jump` and `run` must be positive"));
    }
    let t = fulcrum_map_scan::doc::terrain_of_file(&map_path(&args.path)?)?;
    let r = plan(&t, args.from.trim(), args.to.trim(), &m)?;
    let mut v = report_value_with(&r, args.polyline.unwrap_or(false));
    if let Value::Object(o) = &mut v {
        o.insert("sentence".into(), json!(r.sentence));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fulcrum_dev_channel::message::PropEntry;

    /// Two platforms, a gap of `gap` cells between them, a start on the first and a finish on the second.
    fn write_map(name: &str, gap: i32) -> String {
        let mut cells = String::new();
        for x in 0..3 {
            cells.push_str(&format!("({x}, 0, 0): Cell(kind: \"stone\", height: 4),\n"));
        }
        for x in (3 + gap)..(6 + gap) {
            cells.push_str(&format!("({x}, 0, 0): Cell(kind: \"stone\", height: 4),\n"));
        }
        let finish = 4 + gap;
        let text = format!(
            r#"MapDoc(
    id: "{name}", name: "{name}", schema: "voriz", schema_print: "", format: 7,
    cells: {{ {cells} }},
    props: {{
        1: Prop(kind: "", pos: (1, 4, 0), flags: ["piece"], footprint: Empty, archetype: Some("mark"), components: [ComponentData(kind: "mark", fields: {{"key": text("start"), "what": text("t")}})]),
        2: Prop(kind: "", pos: ({finish}, 4, 0), flags: ["piece"], footprint: Empty, archetype: Some("mark"), components: [ComponentData(kind: "mark", fields: {{"key": text("finish"), "what": text("t")}})]),
    }},
    voxel: true,
)"#
        );
        let path = std::env::temp_dir().join(format!("bennu-link-numbers-{}-{name}.fmap.ron", std::process::id()));
        std::fs::write(&path, text).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn a_map_file_summary_has_the_sentence_and_the_kinds() {
        let p = write_map("scheda", 2);
        let v = map_scheda(&MapSchedaArgs { path: p.clone(), ..Default::default() }).unwrap();
        assert_eq!(v["kinds"]["stone"], json!(6));
        assert_eq!(v["marks"]["start"], json!([1.0, 1.0, 0.0]));
        assert!(v["sentence"].as_str().unwrap().contains("2 segni"), "{v}");
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn a_path_that_is_not_a_map_never_reaches_the_disk() {
        assert!(map_scheda(&MapSchedaArgs { path: "relative.fmap.ron".into(), ..Default::default() }).unwrap_err().contains("absolute"));
        assert!(map_scheda(&MapSchedaArgs { path: "/etc/passwd".into(), ..Default::default() }).unwrap_err().contains("not a map"));
        assert!(map_scheda(&MapSchedaArgs { path: "/tmp/x.fmap.ron".into(), parts: vec!["nope".into()], ..Default::default() }).unwrap_err().contains("not a part"));
    }

    #[test]
    fn the_route_tells_a_possible_gap_from_an_impossible_one() {
        let ok = write_map("ok", 3);
        let bad = write_map("bad", 5);
        let a = |path: &str| MapPercorsoArgs { path: path.into(), from: "start".into(), to: "finish".into(), ..Default::default() };
        let r = map_percorso(&a(&ok)).unwrap();
        assert_eq!(r["reachable"], json!(true), "{r}");
        let r = map_percorso(&a(&bad)).unwrap();
        assert_eq!(r["reachable"], json!(false), "{r}");
        assert!(r["sentence"].as_str().unwrap().contains("NON percorribile"), "{r}");
        // A longer reach makes the same gap possible: the answer depends on the numbers, so the guard is not empty.
        let r = map_percorso(&MapPercorsoArgs { run: Some(6.0), ..a(&bad) }).unwrap();
        assert_eq!(r["reachable"], json!(true), "{r}");
        // La polilinea c'è solo se la si chiede.
        assert!(r["legs"][0].get("polyline").is_none(), "{r}");
        let r = map_percorso(&MapPercorsoArgs { run: Some(6.0), polyline: Some(true), ..a(&bad) }).unwrap();
        assert!(r["legs"][0]["polyline"].as_array().is_some_and(|p| p.len() >= 2), "{r}");
        let _ = (std::fs::remove_file(ok), std::fs::remove_file(bad));
    }

    #[test]
    fn two_maps_are_compared_before_to_after() {
        let (a, b) = (write_map("cmp-a", 2), write_map("cmp-b", 3));
        let v = map_scheda(&MapSchedaArgs { path: a.clone(), compare: Some(b.clone()), ..Default::default() }).unwrap();
        assert_eq!(v["identical"], json!(false), "{v}");
        assert!(v["sentence"].as_str().unwrap().contains("misure"), "{v}");
        let _ = (std::fs::remove_file(a), std::fs::remove_file(b));
    }

    #[test]
    fn props_say_where_they_are_and_how_tall() {
        let tall = PropEntry { source: "script".into(), name: "stone".into(), id: "cozy:goal".into(), cell: [3, 64, -2], pos: [3.5, 64.0, -1.5], yaw: 90.0, min: [3.0, 64.0, -2.0], max: [4.0, 67.0, -1.0] };
        let block = PropEntry { source: "block".into(), name: "vase".into(), id: String::new(), cell: [0, 1, 0], pos: [0.5, 1.0, 0.5], yaw: 0.0, min: [0.0; 3], max: [0.0; 3] };
        let v = props_json(&PropsDump { entries: vec![tall, block], notes: vec!["no mods".into()] });
        assert_eq!(v["props"][0]["size"], json!([1.0, 3.0, 1.0]));
        assert!(v["props"][1].get("size").is_none(), "a block with unknown geometry has no box");
        assert!(v["sentence"].as_str().unwrap().contains("tallest: stone 3.0 m"), "{v}");
        assert_eq!(props_json(&PropsDump { entries: vec![], notes: vec![] })["sentence"], "no props");
    }
}
