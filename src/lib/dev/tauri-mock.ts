/**
 * Dev-only stand-in for the Tauri runtime, so the frontend can run in a plain browser.
 *
 * Why it exists: Arbor is a Tauri app and every screen talks to a Rust backend through
 * `window.__TAURI_INTERNALS__`. Without it a browser renders error states everywhere, so
 * nothing could be screenshotted or compared. This installs a fake of that bridge: commands
 * are answered from fixtures, events can be emitted from the console, window calls are inert.
 *
 * It is loaded ONLY from `src/routes/+layout.ts`, behind `import.meta.env.DEV` and the
 * `?mock` query flag, so it never ships and never touches a real Tauri window.
 *
 * Unknown commands resolve `null` and are recorded in `window.__mock.missing`, which is the
 * work list for extending the fixtures: boot a screen, read the list, add the answers.
 */
import { fixtures, type FixtureHandler } from './fixtures';

type Cb = (payload: unknown) => void;

interface MockState {
  /** Commands nobody answered, with how many times they were asked and one sample of args. */
  missing: Record<string, { count: number; sample: unknown }>;
  /** Every command in call order (capped), for debugging a flow. */
  calls: { cmd: string; args: unknown }[];
  /** Push an event to whoever called `listen(event, …)`. */
  emit: (event: string, payload?: unknown) => void;
  label: string;
}

declare global {
  interface Window {
    __mock?: MockState;
  }
}

const CALL_LOG_CAP = 500;

export function installTauriMock(label: string): void {
  let nextCallback = 1;
  let nextEventId = 1;
  const callbacks = new Map<number, Cb>();
  /** event name → (eventId → callback id) */
  const listeners = new Map<string, Map<number, number>>();

  const state: MockState = {
    missing: {},
    calls: [],
    label,
    emit(event, payload) {
      const subs = listeners.get(event);
      if (!subs) return;
      for (const [id, cbId] of subs) callbacks.get(cbId)?.({ event, id, payload });
    },
  };
  window.__mock = state;

  const answer = async (cmd: string, args: Record<string, unknown> = {}): Promise<unknown> => {
    if (state.calls.length < CALL_LOG_CAP) state.calls.push({ cmd, args });

    // Event plugin: the only commands with real behaviour, because `listen` is how
    // every store gets its push updates.
    if (cmd === 'plugin:event|listen') {
      const event = String(args.event);
      const id = nextEventId++;
      if (!listeners.has(event)) listeners.set(event, new Map());
      listeners.get(event)!.set(id, Number(args.handler));
      return id;
    }
    if (cmd === 'plugin:event|unlisten') {
      listeners.get(String(args.event))?.delete(Number(args.eventId));
      return null;
    }
    if (cmd === 'plugin:event|emit' || cmd === 'plugin:event|emit_to') {
      state.emit(String(args.event), args.payload);
      return null;
    }

    // Backend calls are all `rpc(program, method, params)`; key them as "program.method".
    const key = cmd === 'rpc' ? `${String(args.program)}.${String(args.method)}` : cmd;
    const handler: FixtureHandler | undefined = fixtures[key];
    if (handler !== undefined) {
      const params = (cmd === 'rpc' ? (args.params as Record<string, unknown>) : args) ?? {};
      return typeof handler === 'function' ? handler(params) : handler;
    }

    const seen = state.missing[key];
    if (seen) seen.count++;
    else state.missing[key] = { count: 1, sample: cmd === 'rpc' ? args.params : args };
    return null;
  };

  // Assigned through a loose view: the real Tauri typings declare these two on Window with
  // their own shapes, which a faithful fake does not need to satisfy.
  const w = window as unknown as Record<string, unknown>;
  w.__TAURI_INTERNALS__ = {
    invoke: (cmd: string, args?: Record<string, unknown>) => answer(cmd, args),
    transformCallback: (cb: Cb) => {
      const id = nextCallback++;
      callbacks.set(id, cb);
      return id;
    },
    unregisterCallback: (id: number) => void callbacks.delete(id),
    convertFileSrc: (path: string) => path,
    metadata: {
      currentWindow: { label },
      currentWebview: { windowLabel: label, label },
      windows: [{ label }],
      webviews: [{ windowLabel: label, label }],
    },
  };
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: (event: string, id: number) => void listeners.get(event)?.delete(id),
  };
}
