//! `scenes` domain — clip-launcher scene metadata + fire.
//!
//! `scene(...)` declarations are collected during evaluation (the lang crate's
//! `EvalOutput::scenes`) and stashed in the last-good evaluation. The launcher
//! panel reads them through `merula_scenes` to lay out its grid: one row per scene,
//! one column per base track. Firing a scene/clip (`merula_launch`) re-stages the
//! tracks with the chosen clips substituted into the same-named base track.
//!
//! W1 ports the **pure metadata + lookup** core — the DTOs and the
//! scene→base-track index mapping ([`scene_infos`]) with tests — plus the two
//! handlers. The scene metadata + the launch override are both derived from the
//! last-good evaluation ([`MerulaState::latest`]); until the W3 eval domain stages
//! a typed `Latest` into it (the W0 slot is raw JSON, which a `Tracks<ControlMap>`
//! cannot round-trip through) there is nothing staged, so `merula_scenes` returns
//! the empty grid and `merula_launch` is a no-op. `merula_launch`'s live restage
//! goes through the W3 audio session.

use serde::{Deserialize, Serialize};

use merula::prelude::Scene;

use crate::session;
use crate::state::MerulaState;

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

/// The `merula_scenes` result: the base track names (launcher columns, in mixer
/// order) and the declared scenes (rows). Empty until an arrangement with
/// `scene(...)` has been evaluated.
#[derive(Debug, Clone, Serialize)]
pub struct Scenes {
    pub tracks: Vec<String>,
    pub scenes: Vec<SceneInfo>,
}

/// Resolve each scene's clips against the base track names → the launcher DTOs.
/// Pure: `track_names` is the mixer-order base track list, `scenes_src` the
/// declared scenes from the evaluation. A clip whose target track name has no base
/// track gets `track_index = None` (inert). The core behind `merula_scenes`,
/// shared with the W3 eval domain.
pub(crate) fn scene_infos(track_names: &[String], scenes_src: &[Scene]) -> Vec<SceneInfo> {
    let index_of = |name: &str| -> Option<u32> {
        track_names.iter().position(|n| n == name).map(|i| i as u32)
    };
    scenes_src
        .iter()
        .map(|s| SceneInfo {
            name: s.name.clone(),
            clips: s
                .clips
                .iter()
                .map(|c| SceneClip { track: c.name.clone(), track_index: index_of(&c.name) })
                .collect(),
        })
        .collect()
}

/// Return the launchable scenes of the last-evaluated arrangement. Reads the
/// process-global typed [`Latest`](crate::session::Latest) that `merula_eval`
/// stages (base track names in mixer order + the declared scenes), resolving each
/// scene's clips through the same [`scene_infos`] core. Off the audio thread; safe
/// to call while playing. Empty until something has been evaluated.
#[arbor_rpc::handler]
fn merula_scenes(_ctx: &MerulaState) -> Result<Scenes, String> {
    let resolved = session::with_latest(|l| {
        let track_names: Vec<String> = l.tracks.tracks.iter().map(|t| t.name.clone()).collect();
        let scenes = scene_infos(&track_names, &l.scenes);
        Scenes { tracks: track_names, scenes }
    });
    Ok(resolved.unwrap_or(Scenes { tracks: Vec::new(), scenes: Vec::new() }))
}

/// One entry of a launch selection: base track `track` should play the clip that
/// scene `scene` declares for it (instead of its base pattern). Tracks absent from
/// the selection keep their base pattern.
#[derive(Debug, Clone, Deserialize)]
pub struct ClipSelection {
    pub track: u32,
    pub scene: String,
}

/// Fire the clip launcher's current selection: re-stage the last-evaluated tracks
/// with the chosen scenes' clips substituted into their same-named base tracks. An
/// empty selection restores every track to its base — i.e. "stop all".
///
/// A no-op when nothing is evaluated yet (the W1 state) or when no session is live;
/// the W3 eval/audio domains build the override from the typed `Latest` and stage
/// it on the live session.
#[arbor_rpc::handler]
fn merula_launch(ctx: &MerulaState, selection: Vec<ClipSelection>) -> Result<(), String> {
    let _ = selection;
    let _staged = ctx.latest().is_some();
    // W3: build the override tracks from the typed `Latest` (substitute each
    // selected scene's same-named clip into its base track) and stage it on the
    // live session via the audio control channel.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use merula::prelude::track;

    /// A scene's clips resolve to the column index of the same-named base track;
    /// a clip targeting an unknown track is inert (`track_index = None`).
    #[test]
    fn scene_clip_indices_resolve_by_name() {
        let names = vec!["drums".to_string(), "bass".to_string(), "lead".to_string()];
        // A scene "B" with a clip on `bass` and a clip on a non-existent `keys`.
        let scene = Scene {
            name: "B".to_string(),
            clips: vec![track("bass", merula::prelude::silence()), track("keys", merula::prelude::silence())],
        };
        let infos = scene_infos(&names, std::slice::from_ref(&scene));
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].name, "B");
        assert_eq!(infos[0].clips[0].track, "bass");
        assert_eq!(infos[0].clips[0].track_index, Some(1), "bass is column 1");
        assert_eq!(infos[0].clips[1].track, "keys");
        assert_eq!(infos[0].clips[1].track_index, None, "unknown track → inert clip");
    }

    /// No scenes → no scene rows.
    #[test]
    fn no_scenes_is_empty() {
        let names = vec!["drums".to_string()];
        assert!(scene_infos(&names, &[]).is_empty());
    }
}
