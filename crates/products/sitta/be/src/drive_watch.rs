//! Removable-media watcher: samples the mounted volumes on a dedicated thread and
//! emits [`DRIVES_CHANGED_TOPIC`] only when the set actually changes, so the
//! explorer's Devices list updates the moment a USB drive is plugged or removed.
//!
//! A plain OS thread (not a runtime worker): the sample is a cheap, probe-free
//! read (`arbor_fs::mounts`), but on Unix it touches the filesystem, and nothing
//! blocking may ever sit on the serve-loop's runtime. The first sample is a silent
//! baseline, so no event can precede the `Hello` frame.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use arbor_fs::prelude::{diff_mounts, list_mounts};
use arbor_ipc::prelude::EventSink;
use serde_json::json;

/// Event topic the shell re-emits to the explorer window(s).
pub const DRIVES_CHANGED_TOPIC: &str = "arbor://drives-changed";

const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Start the watcher. The thread lives as long as the process (the shell closing
/// the pipe ends the process).
pub fn spawn(sink: Arc<dyn EventSink>) {
    let _ = thread::Builder::new().name("sitta-drive-watch".into()).spawn(move || {
        let mut prev = list_mounts();
        loop {
            thread::sleep(POLL_INTERVAL);
            let next = list_mounts();
            let diff = diff_mounts(&prev, &next);
            if !diff.is_empty() {
                sink.emit(DRIVES_CHANGED_TOPIC, json!({ "added": diff.added, "removed": diff.removed }));
                prev = next;
            }
        }
    });
}
