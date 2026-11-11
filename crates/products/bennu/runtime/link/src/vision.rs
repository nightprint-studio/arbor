//! The two **picture-free ways to look at a game**, shaped for an assistant: the UI tree and image analysis.
//!
//! An assistant that wants to know "is the pill on screen?" or "did the look change?" used to ask for a screenshot and read
//! 1280×720 pixels — thousands of tokens, most of them wallpaper. The `dev` channel (protocol 2) answers two better questions:
//!
//! * [`ui_json`] — the interface as data: node identity, resolved text, rectangle, visible, opacity, colours. No pixels at all;
//! * [`results_json`] — a handful of structured answers (`stats`, `palette`, `edges`, `diff`…) that the **game** computes on its own
//!   screenshot, so the picture never crosses the wire.
//!
//! This module is the pure part: parsing of the operation strings, shaping of the answers, and the analysis of a PNG on disk.
//! It knows no socket, so it is held by plain unit tests (and by the same code compiled in a scratch crate when the workspace
//! cannot be built).

use std::path::{Path, PathBuf};

use fulcrum_dev_channel::message::UiDump;
use fulcrum_vision::ops::{run_all, FileEnv, ImageOp, OpResult};
use fulcrum_vision::parse::parse_op;
use fulcrum_vision::region::Region;
use serde_json::{json, Value};

/// How many operations one call may ask for: the answer is meant to be small.
pub const MAX_OPS: usize = 12;

/// How much of each answer to hand back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    /// One sentence per operation. The default: it is what an assistant reads.
    Summary,
    /// The structured result only.
    Json,
    /// Both.
    Both,
}

impl Detail {
    pub fn parse(word: Option<&str>) -> Result<Detail, String> {
        match word.map(|w| w.trim().to_ascii_lowercase()).as_deref() {
            None | Some("") | Some("summary") => Ok(Detail::Summary),
            Some("json") => Ok(Detail::Json),
            Some("both") => Ok(Detail::Both),
            Some(other) => Err(format!("unknown detail `{other}`: use `summary`, `json` or `both`")),
        }
    }
}

/// An `f32` as a JSON number **without the widening noise**: `0.7_f32 as f64` is `0.699999988…`, and an assistant reads every digit.
fn num(v: f32) -> Value {
    json!(format!("{v}").parse::<f64>().unwrap_or(v as f64))
}

/// The UI tree as the tool answers it: one compact object per node, and no key that carries nothing (`v` only when hidden, `a` only
/// when not fully opaque; `flags` only when asked for, i.e. for the whole interface and not for one screen). `r` is `[x, y, w, h]` in logical window points; `viewport` and `scale` say how to turn it into screenshot pixels.
pub fn ui_json(d: &UiDump, with_flags: bool) -> Value {
    let nodes: Vec<Value> = d
        .nodes
        .iter()
        .map(|n| {
            let mut o = serde_json::Map::new();
            o.insert("n".into(), json!(format!("{}/{}", n.scope, n.id)));
            o.insert("r".into(), Value::Array(n.rect.iter().map(|v| num(*v)).collect()));
            if let Some(t) = &n.text {
                o.insert("t".into(), json!(t));
            }
            if !n.visible {
                o.insert("v".into(), json!(false));
            }
            if n.alpha < 0.995 {
                o.insert("a".into(), num(n.alpha));
            }
            if let Some(b) = &n.bg {
                o.insert("bg".into(), json!(b));
            }
            if let Some(i) = &n.ink {
                o.insert("ink".into(), json!(i));
            }
            Value::Object(o)
        })
        .collect();
    let mut out = json!({ "viewport": [num(d.viewport[0]), num(d.viewport[1])], "scale": num(d.scale), "count": nodes.len(), "nodes": nodes });
    // Le bandiere sono una riga lunga e uguale a ogni chiamata: servono per la schermata intera, non per un nodo solo.
    if with_flags {
        out["flags"] = json!(d.flags);
    }
    out
}

/// The operations of a call, parsed. `region` is the default for the ones without their own `@region`.
pub fn parse_ops(ops: &[String], region: Option<&str>) -> Result<Vec<ImageOp>, String> {
    let region = match region.map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => Region::parse(r).map_err(|e| e.to_string())?,
        None => Region::Full,
    };
    let default = ["stats".to_string()];
    let specs: &[String] = if ops.is_empty() { &default } else { ops };
    if specs.len() > MAX_OPS {
        return Err(format!("at most {MAX_OPS} operations per call, not {}", specs.len()));
    }
    specs.iter().map(|s| parse_op(s.trim(), &region).map_err(|e| e.to_string())).collect()
}

/// The answer of an analysis: `{results: [{op, summary?, data?}]}`.
pub fn results_json(ops: &[ImageOp], results: &[OpResult], detail: Detail) -> Value {
    let rows: Vec<Value> = ops
        .iter()
        .zip(results)
        .map(|(op, r)| {
            let mut o = serde_json::Map::new();
            o.insert("op".into(), json!(op.label()));
            if matches!(detail, Detail::Summary | Detail::Both) || matches!(r, OpResult::Error(_)) {
                o.insert("summary".into(), json!(r.summary()));
            }
            if matches!(detail, Detail::Json | Detail::Both) {
                // Dalla stringa, non da `to_value`: gli `f32` del risultato sono già arrotondati e la stringa li tiene tali.
                o.insert("data".into(), serde_json::from_str(&r.json()).unwrap_or(Value::Null));
            }
            Value::Object(o)
        })
        .collect();
    json!({ "results": rows })
}

/// Whether `path` is somewhere a scratch picture may be read from without a project root: an absolute `.png` in the system temporary folder.
pub fn scratch_path(path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err(format!("`{path}` is not absolute: give an absolute path to a .png in the temporary folder, or pass `root` for a project file"));
    }
    if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        return Err(format!("`{path}` is not a .png"));
    }
    let real = p.canonicalize().map_err(|e| format!("`{path}`: {e}"))?;
    let tmp = std::env::temp_dir().canonicalize().unwrap_or_else(|_| std::env::temp_dir());
    if real.starts_with(&tmp) || real.starts_with("/tmp") || real.starts_with("/private/tmp") {
        Ok(real)
    } else {
        Err(format!("`{path}` is outside the temporary folder ({}): pass `root` to read a project file", tmp.display()))
    }
}

/// Analyse a PNG on disk with the same operations the game runs for a screenshot. Blocking (it reads and computes): call it from `spawn_blocking`.
pub fn analyze_file(file: &Path, ops: &[ImageOp]) -> Result<Vec<OpResult>, String> {
    if ops.iter().any(|o| matches!(o.region, Region::Ui(_))) {
        return Err("a `ui:` region names a node of a running game: it needs `bevy_game_analyze`, not a file".into());
    }
    let img = fulcrum_vision::image::Image::load(file).map_err(|e| e.to_string())?;
    Ok(run_all(&img, ops, &FileEnv::default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fulcrum_dev_channel::message::UiNode;

    fn node(id: &str, visible: bool, alpha: f32) -> UiNode {
        UiNode { scope: "cozy".into(), id: id.into(), text: Some("Cozy 70%".into()), rect: [10.0, 20.0, 30.0, 40.0], visible, alpha, bg: Some("#101010".into()), ink: None }
    }

    #[test]
    fn the_tree_keeps_only_the_keys_that_say_something() {
        let d = UiDump { viewport: [1280.0, 720.0], scale: 2.0, flags: vec!["x".into()], nodes: vec![node("pill", true, 1.0), node("ghost", false, 0.5)] };
        let v = ui_json(&d, true);
        assert_eq!(ui_json(&d, false).get("flags"), None);
        assert_eq!(v["count"], 2);
        assert_eq!(v["nodes"][0], json!({ "n": "cozy/pill", "r": [10.0, 20.0, 30.0, 40.0], "t": "Cozy 70%", "bg": "#101010" }));
        assert_eq!(v["nodes"][1]["v"], json!(false));
        assert_eq!(v["nodes"][1]["a"], json!(0.5));
        assert_eq!(v["scale"], json!(2.0));
    }

    #[test]
    fn operations_parse_with_the_default_region_and_their_own() {
        let ops = parse_ops(&["stats".into(), "palette:3@ui:cozy/pill".into()], Some("0.7,0,1,0.15")).unwrap();
        assert_eq!(ops[0].region, Region::Frac { x0: 0.7, y0: 0.0, x1: 1.0, y1: 0.15 });
        assert_eq!(ops[1].region, Region::Ui("cozy/pill".into()));
        assert_eq!(parse_ops(&[], None).unwrap().len(), 1);
        assert!(parse_ops(&["boh".into()], None).unwrap_err().contains("sconosciuta"));
        assert!(parse_ops(&vec!["stats".to_string(); MAX_OPS + 1], None).unwrap_err().contains("at most"));
    }

    #[test]
    fn the_answer_is_one_sentence_per_operation_unless_asked_for_more() {
        let img = fulcrum_vision::image::Image::filled(8, 8, [92, 58, 40, 255]);
        let ops = parse_ops(&["stats".into(), "grid:2x2".into()], None).unwrap();
        let results = run_all(&img, &ops, &FileEnv::default());
        let s = results_json(&ops, &results, Detail::Summary);
        assert_eq!(s["results"].as_array().unwrap().len(), 2);
        assert!(s["results"][0]["summary"].as_str().unwrap().contains("cacao"));
        assert!(s["results"][0].get("data").is_none());
        let j = results_json(&ops, &results, Detail::Json);
        assert!(j["results"][0].get("summary").is_none());
        assert!(j["results"][0]["data"]["stats"]["mean"].is_string());
        assert_eq!(Detail::parse(Some("BOTH")).unwrap(), Detail::Both);
        assert!(Detail::parse(Some("x")).is_err());
    }

    #[test]
    fn a_failed_operation_always_says_why() {
        let img = fulcrum_vision::image::Image::filled(8, 8, [0, 0, 0, 255]);
        let ops = parse_ops(&["stats@100,100,5,5".into()], None).unwrap();
        let results = run_all(&img, &ops, &FileEnv::default());
        let j = results_json(&ops, &results, Detail::Json);
        assert!(j["results"][0]["summary"].as_str().unwrap().starts_with("errore"));
    }

    #[test]
    fn scratch_paths_must_be_absolute_pngs_in_the_temp_folder() {
        assert!(scratch_path("relative.png").unwrap_err().contains("not absolute"));
        assert!(scratch_path("/etc/passwd").unwrap_err().contains("not a .png"));
        let dir = std::env::temp_dir().join(format!("bennu-vision-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("a.png");
        std::fs::write(&png, fulcrum_vision::image::Image::filled(4, 4, [1, 2, 3, 255]).to_png_bytes().unwrap()).unwrap();
        let real = scratch_path(png.to_str().unwrap()).unwrap();
        let ops = parse_ops(&["stats".into()], None).unwrap();
        assert!(matches!(analyze_file(&real, &ops).unwrap()[0], OpResult::Stats(_)));
        assert!(analyze_file(&real, &parse_ops(&["stats@ui:a/b".into()], None).unwrap()).unwrap_err().contains("running game"));
        assert!(scratch_path("/Users/nobody/x.png").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
