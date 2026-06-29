//! Audio output device listing + selection.
//!
//! Ported from the shell's `src-tauri/src/merula/mod.rs` `merula_audio_devices` /
//! `merula_set_output_device`. The cpal device enumeration is in-process now
//! (`merula::prelude::list_output_devices()` is Tauri-free), and persistence goes
//! through merula-be's own config (`config_cmds`). A live device switch is pushed
//! to the running session as a [`MerulaControl::SetOutputDevice`] — the audio
//! thread reopens the stream on the new device, preserving the playhead + play
//! state; no-op when stopped.

use serde::Serialize;

use crate::config_cmds;
use crate::control::MerulaControl;
use crate::session;
use crate::state::MerulaState;

/// One selectable audio output device, for the Settings picker.
#[derive(Debug, Clone, Serialize)]
pub struct AudioDeviceInfo {
    /// cpal device name — the stable id persisted + handed back when chosen.
    pub name: String,
    /// Whether this is the host's current default output device.
    pub is_default: bool,
}

/// List the host's audio output devices (name + whether it's the system default).
/// The default is always reachable by selecting "System default" (a `None` device).
#[arbor_rpc::handler]
fn merula_audio_devices(_ctx: &MerulaState) -> Result<Vec<AudioDeviceInfo>, String> {
    Ok(merula::prelude::list_output_devices()
        .into_iter()
        .map(|d| AudioDeviceInfo { name: d.name, is_default: d.is_default })
        .collect())
}

/// Choose the audio output device (by name; `None` = host default). Persists the
/// choice to the merula config and, when a session is live, switches it
/// immediately (reopening the stream, preserving the playhead + play state).
#[arbor_rpc::handler]
fn merula_set_output_device(ctx: &MerulaState, device: Option<String>) -> Result<(), String> {
    let mut cfg = config_cmds::load();
    cfg.output_device = device.clone();
    config_cmds::save(&cfg)?;
    let guard = ctx.session();
    session::send_if_live(&guard, MerulaControl::SetOutputDevice { device });
    Ok(())
}
