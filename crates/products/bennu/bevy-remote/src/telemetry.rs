//! Runtime statistics: frame-time percentiles from [`crate::stats`], every diagnostic the game has
//! registered (FPS, entity count, and whatever else `DiagnosticsPlugin`s add — system timings included),
//! and the window.

use bevy::diagnostic::DiagnosticsStore;
use bevy::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::stats::FrameHistory;

#[derive(Resource, Default)]
pub struct FrameTimes(pub FrameHistory);

/// Feeds the rolling window; runs every frame, costs one push.
pub fn record_frame(time: Res<Time<Real>>, mut history: ResMut<FrameTimes>) {
    history.0.push(time.delta_secs_f64() * 1000.0);
}

#[derive(Debug, Deserialize, Default)]
pub struct StatsArgs {
    /// Also return the last N frame times (ms), for a graph. Default 0, at most 600.
    #[serde(default)]
    pub history: Option<usize>,
}

pub fn read(world: &mut World, args: StatsArgs) -> Result<Value, String> {
    let (frame, recent) = match world.get_resource::<FrameTimes>() {
        Some(h) => (h.0.summary(), h.0.recent(args.history.unwrap_or(0).min(crate::stats::WINDOW))),
        None => (None, Vec::new()),
    };
    let diagnostics: Vec<Value> = world
        .get_resource::<DiagnosticsStore>()
        .map(|store| {
            store
                .iter()
                .filter(|d| d.is_enabled)
                .map(|d| json!({ "path": d.path().as_str(), "value": d.value(), "average": d.average(), "smoothed": d.smoothed(), "suffix": d.suffix }))
                .collect()
        })
        .unwrap_or_default();
    let window = world.query::<&Window>().iter(world).next().map(|w| {
        json!({
            "title": w.title,
            "width": w.width(),
            "height": w.height(),
            "physical_width": w.physical_width(),
            "physical_height": w.physical_height(),
            "scale_factor": w.scale_factor(),
            "focused": w.focused,
        })
    });
    Ok(json!({
        "uptime_s": world.get_resource::<Time<Real>>().map(|t| t.elapsed_secs_f64()),
        "frame": frame,
        "frame_ms_recent": recent,
        "diagnostics": diagnostics,
        "window": window,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_of_an_empty_world_are_well_formed() {
        let mut world = World::new();
        let v = read(&mut world, StatsArgs::default()).unwrap();
        assert!(v["frame"].is_null() && v["diagnostics"].as_array().unwrap().is_empty());
    }

    #[test]
    fn recorded_frames_show_up_in_the_summary() {
        let mut world = World::new();
        let mut times = FrameTimes::default();
        times.0.push(10.0);
        times.0.push(20.0);
        world.insert_resource(times);
        let v = read(&mut world, StatsArgs { history: Some(5) }).unwrap();
        assert_eq!(v["frame"]["frames"], 2);
        assert_eq!(v["frame_ms_recent"], json!([10.0, 20.0]));
    }
}
