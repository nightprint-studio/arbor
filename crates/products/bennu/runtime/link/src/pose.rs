//! The numeric pose of a node or an entity, shaped for an assistant (pure: no wire, no Tauri).
//!
//! The game answers with a [`PoseDump`] (the pose now) or a [`PoseSeriesDump`] (N frames); this turns them into small
//! JSON and, for a series, adds the **summary** that `fulcrum-trace` computes (peak, overshoot, settling, reversals,
//! whether it returns to its start, sanity warnings). The summary is what answers *"does the animation do what it
//! should?"* in a few hundred bytes, which is the point: a screenshot of the same motion costs thousands of tokens.

use fulcrum_dev_channel::message::{PoseDump, PoseKind, PoseSeriesDump};
use fulcrum_trace::{summarize, Channel, ChannelKind, Series};
use serde_json::{json, Map, Value};

fn kind_name(k: PoseKind) -> &'static str {
    match k {
        PoseKind::Ui => "ui",
        PoseKind::Entity => "entity",
    }
}

/// Round to 3 decimals so the answer stays short (and `-0` never shows).
fn r3(v: f32) -> Value {
    if !v.is_finite() {
        return Value::Null;
    }
    let x = (f64::from(v) * 1000.0).round() / 1000.0;
    json!(if x == 0.0 { 0.0 } else { x })
}

/// `{target, kind, pose: {key: value}, anims: [{name, t, length, waiting}], notes}`.
pub fn pose_json(d: &PoseDump) -> Value {
    let anims: Vec<Value> = d.anims.iter().map(|a| json!({ "name": a.name, "t": r3(a.t), "length": r3(a.length), "waiting": a.waiting })).collect();
    let pose: Map<String, Value> = d.keys.iter().zip(&d.values).map(|(k, v)| (k.clone(), r3(*v))).collect();
    json!({
        "target": d.target,
        "kind": kind_name(d.kind),
        "pose": pose,
        "anims": anims,
        "notes": d.notes,
    })
}

/// What a key measures, so the summary can talk about it (an opacity that ends at zero hides the node).
fn kind_of_key(k: &str) -> ChannelKind {
    match k {
        "sx" | "sy" | "sz" => ChannelKind::Scale,
        "rot" | "yaw" | "pitch" | "roll" => ChannelKind::Angle,
        "alpha" => ChannelKind::Opacity,
        "vis" => ChannelKind::Generic,
        _ => ChannelKind::Position,
    }
}

/// The word for a key in the summary headline.
fn name_of_key(k: &str) -> &str {
    match k {
        "sx" => "scala x",
        "sy" => "scala y",
        "sz" => "scala z",
        "rot" => "rotazione",
        "alpha" => "opacità",
        other => other,
    }
}

/// The samples as a `fulcrum-trace` series (time = seconds since the request).
pub fn series_of(d: &PoseSeriesDump) -> Series {
    let times: Vec<f32> = d.samples.iter().map(|c| c.secs).collect();
    let mut s = Series::new(format!("{} (in the game, {} samples)", d.target, d.samples.len()), times);
    for (i, k) in d.keys.iter().enumerate() {
        s = s.with(Channel::new(k, name_of_key(k), kind_of_key(k), "", d.samples.iter().map(|c| c.values.get(i).copied().unwrap_or(f32::NAN)).collect()));
    }
    s.notes = d.notes.clone();
    s
}

/// `{target, kind, keys, rows: [[frame, secs, v…]], anims, summary: {headline, channels, warnings}, notes}`; the channels that never move are
/// listed once under `still` and left out of the summary's per-channel lines.
pub fn series_json(d: &PoseSeriesDump) -> Value {
    if d.samples.is_empty() {
        return json!({ "target": d.target, "kind": kind_name(d.kind), "rows": [], "notes": d.notes, "error": "no samples" });
    }
    let series = series_of(d);
    let sum = summarize(&series);
    let rows: Vec<Value> = d
        .samples
        .iter()
        .map(|c| {
            let mut row = vec![json!(c.frame), r3(c.secs)];
            row.extend(c.values.iter().map(|v| r3(*v)));
            Value::Array(row)
        })
        .collect();
    let still: Vec<&str> = sum.channels.iter().filter(|c| c.flat).map(|c| c.key.as_str()).collect();
    let moving: Vec<Value> = sum
        .channels
        .iter()
        .filter(|c| !c.flat)
        .map(|c| {
            json!({
                "key": c.key, "from": r3(c.initial), "to": r3(c.last), "min": r3(c.min.1), "max": r3(c.max.1),
                "peak": { "t": r3(c.peak.0), "v": r3(c.peak.1) },
                "overshoot_pct": c.overshoot.map(|o| r3(o.0)), "settle": r3(c.settle), "reversals": c.reversals,
                "returns_to_start": c.returns_to_start, "max_speed": r3(c.max_speed.0),
            })
        })
        .collect();
    let columns: Vec<String> = ["frame", "secs"].into_iter().map(String::from).chain(d.keys.iter().cloned()).collect();
    let warnings: Vec<String> = sum.warnings.iter().map(|w| w.text()).collect();
    json!({
        "target": d.target,
        "kind": kind_name(d.kind),
        "columns": columns,
        "rows": rows,
        "anims": d.anims,
        "still": still,
        "summary": { "headline": sum.headline, "settle": r3(sum.settle), "channels": moving, "warnings": warnings },
        "notes": d.notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fulcrum_dev_channel::message::{PoseAnim, PoseSample};

    fn series() -> PoseSeriesDump {
        // A banner that drops in: scale 2 → 1 in 0.3 s, then still; opacity fixed at 1.
        let samples = (0..8)
            .map(|i| {
                let t = i as f32 * 0.05;
                let s = 1.0 + (1.0 - (t / 0.3).min(1.0)).powi(2);
                PoseSample { frame: i * 3, secs: t, values: vec![s, 1.0] }
            })
            .collect();
        PoseSeriesDump { target: "hud/banner".into(), kind: PoseKind::Ui, keys: vec!["sx".into(), "alpha".into()], samples, anims: vec!["preset/banner-drop".into()], notes: vec![] }
    }

    #[test]
    fn a_series_carries_the_table_and_the_summary() {
        let j = series_json(&series());
        assert_eq!(j["rows"].as_array().unwrap().len(), 8);
        assert_eq!(j["columns"], json!(["frame", "secs", "sx", "alpha"]));
        assert_eq!(j["still"], json!(["alpha"]));
        assert_eq!(j["summary"]["channels"][0]["key"], "sx");
        assert_eq!(j["summary"]["channels"][0]["from"], json!(2.0));
        assert_eq!(j["summary"]["channels"][0]["to"], json!(1.0));
        assert!(j["summary"]["headline"].as_str().unwrap().contains("scala"), "{j}");
    }

    #[test]
    fn the_pose_now_is_a_small_object() {
        let d = PoseDump {
            target: "hud/banner".into(),
            kind: PoseKind::Ui,
            keys: vec!["tx".into(), "alpha".into()],
            values: vec![-0.0001, 0.5],
            anims: vec![PoseAnim { name: "a".into(), t: 0.4, length: 2.8, waiting: false }],
            notes: vec![],
        };
        let j = pose_json(&d);
        assert_eq!(j["pose"]["tx"], json!(0.0));
        assert_eq!(j["pose"]["alpha"], json!(0.5));
        assert_eq!(j["anims"][0]["name"], "a");
    }

    #[test]
    fn an_empty_series_says_so_instead_of_inventing_a_summary() {
        let d = PoseSeriesDump { target: "x/y".into(), kind: PoseKind::Ui, keys: vec![], samples: vec![], anims: vec![], notes: vec!["gone".into()] };
        assert_eq!(series_json(&d)["error"], "no samples");
    }
}
