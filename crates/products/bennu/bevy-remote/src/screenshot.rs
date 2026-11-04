//! Screenshots over BRP.
//!
//! A capture finishes some frames after it is asked for, and a BRP method answers in the frame it is
//! called, so it is two calls: `arbor.screenshot` starts the capture and returns an `id`;
//! `arbor.screenshot.fetch { id }` answers `{ ready: false }` until the PNG exists, then returns it as
//! base64 and forgets it. The PNG is encoded by [`crate::png`] — capped in bytes and optionally in width —
//! so one reply can never be unbounded.

use std::collections::{BTreeMap, HashSet};

use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::PrimaryWindow;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::base64;
use crate::png::{Order, Shot, encode_capped};

/// The most bytes of PNG one reply carries (≈ 5.3 MB once base64'd).
pub const DEFAULT_MAX_BYTES: usize = 4_000_000;
/// Finished screenshots nobody fetched are dropped beyond this many (oldest first).
const KEEP: usize = 4;

#[derive(Resource, Default)]
pub struct Shots {
    next_id: u64,
    pending: HashSet<u64>,
    done: BTreeMap<u64, Result<Shot, String>>,
}

impl Shots {
    fn finish(&mut self, id: u64, result: Result<Shot, String>) {
        self.pending.remove(&id);
        self.done.insert(id, result);
        while self.done.len() > KEEP {
            self.done.pop_first();
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct ShotArgs {
    /// A window entity (as BRP prints entities); default the primary window.
    #[serde(default)]
    pub window: Option<u64>,
    /// Shrink to at most this width, in pixels (whole-factor box average).
    #[serde(default)]
    pub max_width: Option<u32>,
    /// Byte cap of the PNG; default and ceiling 4 MB.
    #[serde(default)]
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct FetchArgs {
    pub id: u64,
}

pub fn start(world: &mut World, args: ShotArgs) -> Result<Value, String> {
    let target = match args.window {
        Some(bits) => Entity::from_bits(bits),
        None => world
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .iter(world)
            .next()
            .ok_or("the game has no primary window")?,
    };
    let (max_width, max_bytes) = (args.max_width.unwrap_or(0), args.max_bytes.unwrap_or(DEFAULT_MAX_BYTES).min(DEFAULT_MAX_BYTES));
    let id = {
        let mut shots = world.get_resource_or_insert_with(Shots::default);
        shots.next_id += 1;
        let id = shots.next_id;
        shots.pending.insert(id);
        id
    };
    world.spawn(Screenshot::window(target)).observe(move |captured: On<ScreenshotCaptured>, mut shots: ResMut<Shots>| {
        let image = &captured.event().image;
        let order = match image.texture_descriptor.format {
            TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => Some(Order::Rgba),
            TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => Some(Order::Bgra),
            _ => None,
        };
        let result = match (order, image.data.as_ref()) {
            (Some(order), Some(data)) => encode_capped(image.width(), image.height(), data, order, max_width, max_bytes),
            (None, _) => Err(format!("the frame is {:?}, not an 8-bit RGBA/BGRA format", image.texture_descriptor.format)),
            (_, None) => Err("the captured frame has no pixel data".to_string()),
        };
        shots.finish(id, result);
    });
    Ok(json!({ "id": id }))
}

pub fn fetch(world: &mut World, args: FetchArgs) -> Result<Value, String> {
    let Some(mut shots) = world.get_resource_mut::<Shots>() else { return Err("no screenshot was requested".into()) };
    match shots.done.remove(&args.id) {
        Some(Ok(shot)) => Ok(json!({
            "ready": true,
            "format": "png",
            "width": shot.width,
            "height": shot.height,
            "source_width": shot.source_width,
            "source_height": shot.source_height,
            "bytes": shot.png.len(),
            "png_base64": base64::encode(&shot.png),
        })),
        Some(Err(e)) => Err(e),
        None if shots.pending.contains(&args.id) => Ok(json!({ "ready": false })),
        None => Err(format!("no screenshot with id {} (fetched already, or never requested)", args.id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finished_shot_is_fetched_once() {
        let mut world = World::new();
        let mut shots = Shots::default();
        shots.pending.insert(1);
        shots.finish(1, Ok(Shot { width: 1, height: 1, source_width: 2, source_height: 2, png: vec![1, 2, 3] }));
        world.insert_resource(shots);
        let first = fetch(&mut world, FetchArgs { id: 1 }).unwrap();
        assert_eq!(first["ready"], true);
        assert_eq!(first["png_base64"], "AQID");
        assert!(fetch(&mut world, FetchArgs { id: 1 }).is_err());
    }

    #[test]
    fn a_pending_shot_says_not_ready() {
        let mut world = World::new();
        let mut shots = Shots::default();
        shots.pending.insert(7);
        world.insert_resource(shots);
        assert_eq!(fetch(&mut world, FetchArgs { id: 7 }).unwrap()["ready"], false);
    }

    #[test]
    fn only_the_newest_few_unfetched_shots_are_kept() {
        let mut shots = Shots::default();
        for id in 1..=(KEEP as u64 + 2) {
            shots.finish(id, Err("x".into()));
        }
        assert_eq!(shots.done.len(), KEEP);
        assert!(!shots.done.contains_key(&1));
    }
}
