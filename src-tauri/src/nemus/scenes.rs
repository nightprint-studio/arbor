//! Clip-launcher scene metadata for the front end.
//!
//! `scene(...)` declarations are collected during evaluation (the lang crate's
//! `EvalOutput::scenes`) and stashed in [`super::Latest`]. The launcher panel
//! reads them through [`nemus_scenes`] to lay out its grid: one row per scene,
//! one column per base track. Firing a scene/clip (a later slice) re-stages the
//! tracks with the chosen clips substituted into the same-named base track.

use serde::{Deserialize, Serialize};
use tauri::State;

use super::NemusState;
use crate::error::AppError;

/// One clip in a scene: the base track it targets (by name) and the resolved
/// column index — `None` when no base track carries that name (an inert clip the
/// launcher can show greyed-out / warn about).
#[derive(Debug, Clone, Serialize)]
pub struct SceneClip {
    pub track: String,
    pub track_index: Option<u32>,
}

/// One launchable scene: its label (the launcher row) and the clips it fires.
#[derive(Debug, Clone, Serialize)]
pub struct SceneInfo {
    pub name: String,
    pub clips: Vec<SceneClip>,
}

/// The `nemus_scenes` result: the base track names (launcher columns, in mixer
/// order) and the declared scenes (rows). Empty until an arrangement with
/// `scene(...)` has been evaluated.
#[derive(Debug, Clone, Serialize)]
pub struct Scenes {
    pub tracks: Vec<String>,
    pub scenes: Vec<SceneInfo>,
}

/// Return the launchable scenes of the last-evaluated arrangement. Off the audio
/// thread; safe to call while playing. The arrangement is snapshotted under the
/// lock and the lock dropped before the (small) DTO is built.
#[tauri::command]
pub async fn nemus_scenes(nemus: State<'_, NemusState>) -> Result<Scenes, AppError> {
    let (track_names, scenes_src) = {
        let latest = nemus.latest.lock().unwrap_or_else(|e| e.into_inner());
        match latest.as_ref() {
            Some(l) => (
                l.tracks.tracks.iter().map(|t| t.name.clone()).collect::<Vec<String>>(),
                l.scenes.clone(),
            ),
            None => return Ok(Scenes { tracks: Vec::new(), scenes: Vec::new() }),
        }
    };

    let index_of =
        |name: &str| -> Option<u32> { track_names.iter().position(|n| n == name).map(|i| i as u32) };

    let scenes = scenes_src
        .iter()
        .map(|s| SceneInfo {
            name: s.name.clone(),
            clips: s
                .clips
                .iter()
                .map(|c| SceneClip { track: c.name.clone(), track_index: index_of(&c.name) })
                .collect(),
        })
        .collect();

    Ok(Scenes { tracks: track_names, scenes })
}

/// One entry of a launch selection: base track `track` should play the clip that
/// scene `scene` declares for it (instead of its base pattern). Tracks absent
/// from the selection keep their base pattern.
#[derive(Debug, Clone, Deserialize)]
pub struct ClipSelection {
    pub track: u32,
    pub scene: String,
}

/// Fire the clip launcher's current selection: re-stage the last-evaluated tracks
/// with the chosen scenes' clips substituted into their same-named base tracks,
/// quantized to the next cycle boundary (the `SetTracks` path). An empty selection
/// restores every track to its base — i.e. "stop all".
///
/// Uses the cached evaluation (`Latest`), so there's no re-parse and no source to
/// pass; the FE owns the selection and re-sends the whole picture on every
/// launch/stop. A no-op when nothing is evaluated yet, or when no session is live
/// (the override is applied the next time playback stages tracks).
#[tauri::command]
pub async fn nemus_launch(
    nemus: State<'_, NemusState>,
    selection: Vec<ClipSelection>,
) -> Result<(), AppError> {
    // Build the override tracks under the lock from the cached evaluation, then
    // release it before staging (which awaits an off-thread sample decode).
    let staged = {
        let latest = nemus.latest.lock().unwrap_or_else(|e| e.into_inner());
        let Some(l) = latest.as_ref() else { return Ok(()) };
        let mut tracks = l.tracks.clone();
        for sel in &selection {
            let Some(base) = tracks.tracks.get_mut(sel.track as usize) else { continue };
            // The clip that targets this track by name within the chosen scene.
            let clip = l
                .scenes
                .iter()
                .find(|s| s.name == sel.scene)
                .and_then(|s| s.clips.iter().find(|c| c.name == base.name));
            if let Some(clip) = clip {
                // Replace the whole channel with the clip (its name already matches),
                // so the override carries the clip's pattern and drops the base's
                // arrangement section bands — the clip is a variation, not the song.
                *base = clip.clone();
            }
        }
        (tracks, l.cps, l.tempo.clone())
    };

    let cfg = super::nemus_config();
    nemus.stage_tracks(&cfg, staged.0, staged.1, staged.2).await;
    Ok(())
}
