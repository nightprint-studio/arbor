// Cancelling a cloud transfer, from the download-progress modal.
//
// This file used to wrap twenty commands — secrets, listings, transfers, streams. Those
// belonged to a cloud panel built into the frontend; the panel is a plugin now and reaches
// the shell through `arbor.cloud.*` instead. Keeping the wrappers would have kept a second
// route to the same operations alive, and that route does not go through the wasm provider
// the plugin's calls do.
//
// Cancellation stays here because it is genuinely the frontend's: the modal owns the button,
// and the flag lives in the shell's state rather than in a bucket.

import { platform } from '$lib/ipc/rpc';

export const cloudCancel = (streamId: string) =>
  platform<void>('cloud_cancel', { stream_id: streamId });

export const cloudIsCancelled = (streamId: string) =>
  platform<boolean>('cloud_is_cancelled', { stream_id: streamId });
