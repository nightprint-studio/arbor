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

use merula::prelude::{ControlMap, Scene, Tracks};

use merula_core::session;
use merula_core::prelude::MerulaState;

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
/// process-global typed [`Latest`](merula_core::session::Latest) that `merula_eval`
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

/// One entry of a launch selection: base track `track` (by mixer-order index) should
/// play the clip that scene `scene` declares for it (instead of its base pattern).
/// Tracks absent from the selection keep their base pattern.
#[derive(Debug, Clone, Deserialize)]
pub struct ClipSelection {
    pub track: u32,
    pub scene: String,
}

/// Build the override arrangement: clone the base tracks, then for each selection
/// substitute the chosen scene's same-named clip's pattern into the base track at
/// that index. Pure (no audio, no state) so it can be unit-tested; a selection whose
/// index, scene or clip doesn't resolve is skipped, leaving that base track intact.
pub(crate) fn apply_selection(
    base: &Tracks<ControlMap>,
    scenes: &[Scene],
    selection: &[ClipSelection],
) -> Tracks<ControlMap> {
    let mut out = base.clone();
    for sel in selection {
        let Some(base_track) = out.tracks.get(sel.track as usize) else {
            continue; // index past the mixer — a stale selection
        };
        let track_name = base_track.name.clone();
        // The clip is stored as a single-channel track named after the base track it
        // targets, so we match it by that name within the chosen scene.
        let clip = scenes
            .iter()
            .find(|s| s.name == sel.scene)
            .and_then(|s| s.clips.iter().find(|c| c.name == track_name));
        if let Some(clip) = clip {
            out.tracks[sel.track as usize].pattern = clip.pattern.clone();
        }
    }
    out
}

/// Fire the clip launcher's current selection: re-stage the last-evaluated tracks
/// with the chosen scenes' clips substituted into their same-named base tracks. An
/// empty selection restores every track to its base — i.e. "stop all".
///
/// A no-op when nothing has been evaluated yet or when no session is live. Uses the
/// same staging path as `merula_eval`, so it decodes any voice the clips introduce
/// (a different `.inst(...)`) off the RT thread and swaps the arrangement in without
/// interrupting playback.
#[arbor_rpc::handler]
async fn merula_launch(ctx: &MerulaState, selection: Vec<ClipSelection>) -> Result<(), String> {
    let cfg = merula_core::config::load();
    let staged = session::with_latest(|l| {
        (
            apply_selection(&l.tracks, &l.scenes, &selection),
            l.cps,
            l.tempo.clone(),
        )
    });
    let Some((tracks, cps, tempo)) = staged else {
        return Ok(()); // nothing evaluated yet
    };
    crate::audio_cmds::stage_tracks(ctx, &cfg, tracks, cps, tempo).await;
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

    use merula::prelude::{pure, tracks, ControlMap, Time, TimeSpan};

    /// The `sound` marker of a track's pattern at cycle 0 — a cheap way to tell which
    /// pattern (base vs clip) a track is currently carrying.
    fn marker(t: &merula::prelude::Track<ControlMap>) -> Option<String> {
        t.pattern
            .query(TimeSpan::new(Time::ZERO, Time::int(1)))
            .first()
            .and_then(|h| h.value.sound.clone())
    }

    /// The regression this whole change fixes: firing a clip must actually swap that
    /// track's pattern for the scene's same-named clip, and leave the others alone.
    #[test]
    fn selection_substitutes_the_named_clip() {
        // Two base tracks; each clip is stored as a track named after its base track.
        let base = tracks(vec![
            track("chords", pure(ControlMap::sound("piano_base"))),
            track("lead", pure(ControlMap::sound("recorder_base"))),
        ]);
        let scenes = vec![Scene {
            name: "harp".to_string(),
            clips: vec![track("lead", pure(ControlMap::sound("harp_clip")))],
        }];

        // Fire the "harp" scene on the `lead` track (index 1).
        let out = apply_selection(&base, &scenes, &[ClipSelection { track: 1, scene: "harp".into() }]);
        assert_eq!(marker(&out.tracks[1]).as_deref(), Some("harp_clip"), "lead swapped to the clip");
        assert_eq!(marker(&out.tracks[0]).as_deref(), Some("piano_base"), "chords untouched");
    }

    /// An empty selection restores every track to its base ("stop all"); an unknown
    /// scene or an out-of-range index is skipped rather than erroring.
    #[test]
    fn selection_edge_cases_leave_base_intact() {
        let base = tracks(vec![track("lead", pure(ControlMap::sound("base")))]);
        let scenes = vec![Scene {
            name: "harp".to_string(),
            clips: vec![track("lead", pure(ControlMap::sound("harp")))],
        }];

        assert_eq!(marker(&apply_selection(&base, &scenes, &[]).tracks[0]).as_deref(), Some("base"));
        // Unknown scene name → no substitution.
        let bad_scene = apply_selection(&base, &scenes, &[ClipSelection { track: 0, scene: "nope".into() }]);
        assert_eq!(marker(&bad_scene.tracks[0]).as_deref(), Some("base"));
        // Index past the mixer → skipped, no panic.
        let bad_idx = apply_selection(&base, &scenes, &[ClipSelection { track: 9, scene: "harp".into() }]);
        assert_eq!(marker(&bad_idx.tracks[0]).as_deref(), Some("base"));
    }
}
