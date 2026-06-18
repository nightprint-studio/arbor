import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { corvus } from '$lib/ipc/rpc';

/**
 * Frontend half of the streaming seam (`docs/streaming-seam.md`).
 *
 * A streaming command returns an id synchronously, then pushes a sequence of
 * one-way, id-correlated events on four derived topics
 * (`<base>-started/-chunk/-done/-error`). Each payload carries the common
 * envelope `{ stream_id, seq }`. This helper centralizes the four `listen()`
 * calls, the `stream_id` filtering, and the subscribe-before-invoke ordering so
 * a fast synchronous `started` can never outrun the listeners.
 */

/** Common envelope merged into every stream event payload. */
export interface StreamEnvelope {
  /** Correlates the started/chunk/done/error quartet (== job id where a job exists). */
  stream_id: string;
  /** Monotonic per stream; `started` is 0. Lets the FE detect drops / reordering. */
  seq:       number;
}

export type StartedPayload = StreamEnvelope & Record<string, unknown>;
export type DonePayload    = StreamEnvelope & Record<string, unknown>;
export type ErrorPayload   = StreamEnvelope & { error: string };

export interface StreamHandlers<Chunk> {
  onStarted?: (p: StartedPayload) => void;
  onChunk:    (p: Chunk & StreamEnvelope) => void;
  onDone?:    (p: DonePayload) => void;
  onError?:   (e: string, p: ErrorPayload) => void;
}

export interface StreamHandle {
  /** The id returned by the streaming command, correlating the quartet. */
  streamId: string;
  /** Request cancellation of an in-flight stream (no-op for pure-egress streams). */
  cancel:   () => Promise<void>;
  /** Detach all four listeners. Safe to call multiple times. */
  dispose:  () => void;
}

/**
 * Attach to a stream whose id is already known. Wires the four `listen()` calls
 * filtered by `streamId`; returns a `dispose()` that detaches them.
 *
 * Prefer {@link startStream} when the id comes from invoking the command — it
 * guarantees the listeners are live before the command runs.
 */
export function subscribeStream<Chunk>(
  base:     string,
  streamId: string,
  handlers: StreamHandlers<Chunk>,
): { dispose: () => void } {
  let cancelled = false;
  const unlisteners: UnlistenFn[] = [];

  const attach = (suffix: string, fn: (payload: any) => void) => {
    listen<any>(`${base}-${suffix}`, (e) => {
      const p = e.payload;
      // Filter by stream_id — multiple concurrent streams share the topic.
      if (p?.stream_id !== streamId) return;
      fn(p);
    }).then((un) => {
      if (cancelled) un();
      else unlisteners.push(un);
    });
  };

  attach('started', (p) => handlers.onStarted?.(p));
  attach('chunk',   (p) => handlers.onChunk(p));
  attach('done',    (p) => handlers.onDone?.(p));
  attach('error',   (p) => handlers.onError?.(p.error, p));

  return {
    dispose() {
      cancelled = true;
      for (const un of unlisteners) un();
      unlisteners.length = 0;
    },
  };
}

/**
 * Subscribe first, then invoke — so a synchronous `started` can't race the
 * listeners. Wires the four `listen()` calls, invokes the command, captures the
 * returned id, filters every event by it, and hands back `cancel()` /
 * `dispose()`.
 *
 * Listeners are attached to the topics immediately (before `streamId` is known)
 * and buffer events until the id resolves, then replay the matching ones — so no
 * event is dropped even if `started` fires before `invoke` returns.
 */
export async function startStream<Chunk>(
  base:       string,
  invokeArgs: { cmd: string; args?: Record<string, unknown> },
  handlers:   StreamHandlers<Chunk>,
): Promise<StreamHandle> {
  let cancelled = false;
  let streamId: string | null = null;
  const unlisteners: UnlistenFn[] = [];
  // Events that arrive before the id is known are buffered, then replayed once
  // we learn which id this call minted.
  const pending: Array<{ suffix: string; payload: any }> = [];

  const dispatch = (suffix: string, payload: any) => {
    switch (suffix) {
      case 'started': handlers.onStarted?.(payload); break;
      case 'chunk':   handlers.onChunk(payload); break;
      case 'done':    handlers.onDone?.(payload); break;
      case 'error':   handlers.onError?.(payload.error, payload); break;
    }
  };

  const route = (suffix: string, payload: any) => {
    if (streamId === null) {
      pending.push({ suffix, payload });
      return;
    }
    if (payload?.stream_id !== streamId) return;
    dispatch(suffix, payload);
  };

  const attach = (suffix: string) => {
    listen<any>(`${base}-${suffix}`, (e) => route(suffix, e.payload)).then((un) => {
      if (cancelled) un();
      else unlisteners.push(un);
    });
  };

  attach('started');
  attach('chunk');
  attach('done');
  attach('error');

  const id = await corvus<string>(invokeArgs.cmd, invokeArgs.args ?? {});
  streamId = id;

  // Replay anything that arrived for this id before we knew it.
  for (const { suffix, payload } of pending) {
    if (payload?.stream_id === streamId) dispatch(suffix, payload);
  }
  pending.length = 0;

  const dispose = () => {
    cancelled = true;
    for (const un of unlisteners) un();
    unlisteners.length = 0;
  };

  return {
    streamId: id,
    cancel: () => corvus<void>('cancel_stream', { stream_id: id }),
    dispose,
  };
}
