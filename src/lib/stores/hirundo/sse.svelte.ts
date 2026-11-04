/**
 * Hirundo event streams — the window's side of Server-Sent Events.
 *
 * `hirundo-be` runs the stream and reports everything it does as `hirundo:sse` events; this store listens, keeps
 * the last few thousand in order, and offers pause (events keep arriving into a buffer, nothing on screen moves),
 * clear, a filter by event name, and the log as text. Starting a stream returns at once; stopping is `Esc`.
 * One stream at a time: starting another ends the first.
 */
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { sseStart, sseStop } from '$lib/ipc/hirundo';
import type { SseMessage } from '$lib/types/hirundo';
import { hirundoStore, type RequestTab } from './workspace.svelte';

/** Messages kept; the oldest go first. */
const MAX_KEPT = 2000;

export type StreamState = 'idle' | 'connecting' | 'open' | 'reconnecting' | 'closed';

let counter = 0;

function createHirundoSse() {
  let items = $state<SseMessage[]>([]);
  let buffered = $state<SseMessage[]>([]);
  let streamId = $state<string | null>(null);
  let state = $state<StreamState>('idle');
  let paused = $state(false);
  let lastEventId = $state<string | null>(null);
  /** Event names hidden by the filter. */
  let hidden = $state<string[]>([]);
  let title = $state('');
  let confirm = $state<{ tab: RequestTab; method: string; url: string } | null>(null);
  let unlisten: UnlistenFn | null = null;

  const names = $derived([...new Set(items.filter((m) => m.kind === 'event').map((m) => m.event ?? 'message'))]);
  const shown = $derived(items.filter((m) => m.kind !== 'event' || !hidden.includes(m.event ?? 'message')));
  const events = $derived(items.filter((m) => m.kind === 'event').length);

  function push(m: SseMessage) {
    items.push(m);
    if (items.length > MAX_KEPT) items.splice(0, items.length - MAX_KEPT);
  }

  function onMessage(m: SseMessage) {
    if (m.stream_id !== streamId) return;
    if (m.kind === 'event') {
      if (m.id) lastEventId = m.id;
      if (paused) { buffered.push(m); return; }
    } else if (m.kind === 'connecting' || m.kind === 'open' || m.kind === 'reconnecting' || m.kind === 'closed') {
      state = m.kind;
    }
    push(m);
  }

  async function listening() {
    if (!unlisten) unlisten = await listen<SseMessage>('hirundo:sse', (e) => onMessage(e.payload));
  }

  /** Listen to a request as an event stream. `resume` carries on after the last event seen. */
  async function start(tab: RequestTab, opts: { confirmed?: boolean; resume?: boolean } = {}) {
    if (!hirundoStore.project) return;
    await listening();
    if (streamId && state !== 'closed' && state !== 'idle') await stop();
    if (!opts.resume) { items = []; buffered = []; lastEventId = null; }
    streamId = `sse-${++counter}`;
    title = `${tab.request.method} ${tab.request.url}`;
    state = 'connecting';
    try {
      const r = await sseStart(hirundoStore.target(tab), streamId, opts.confirmed ?? false, opts.resume ? lastEventId : null);
      if (r.outcome === 'unresolved') {
        state = 'idle';
        toastStore.show(`Not started — no value for ${[...r.names, ...r.missing_secrets].join(', ')}.`, 'warning');
      } else if (r.outcome === 'needs_confirmation') {
        state = 'idle';
        confirm = { tab, method: r.method, url: r.url };
      }
    } catch (e) {
      state = 'idle';
      toastStore.show(String(e), 'error');
    }
  }

  async function stop() {
    if (!streamId) return;
    try { await sseStop(streamId); } catch { /* it had already ended */ }
  }

  function setPaused(on: boolean) {
    paused = on;
    if (!on) {
      for (const m of buffered) push(m);
      buffered = [];
    }
  }

  function toggleName(name: string) {
    hidden = hidden.includes(name) ? hidden.filter((n) => n !== name) : [...hidden, name];
  }

  /** The log as text, one event per line as JSON — the shape tools expect. */
  function log(): string {
    return items
      .filter((m) => m.kind === 'event')
      .map((m) => JSON.stringify({ at: new Date(m.at_ms).toISOString(), id: m.id ?? null, event: m.event, data: m.data }))
      .join('\n');
  }

  return {
    get items() { return shown; },
    get state() { return state; },
    get paused() { return paused; },
    get buffered() { return buffered.length; },
    get names() { return names; },
    get hidden() { return hidden; },
    get events() { return events; },
    get title() { return title; },
    get lastEventId() { return lastEventId; },
    get confirm() { return confirm; },
    get running() { return state === 'connecting' || state === 'open' || state === 'reconnecting'; },
    start, stop, setPaused, toggleName, log,
    clear() { items = []; buffered = []; },
    dismissConfirm() { confirm = null; },
  };
}

export const hirundoSse = createHirundoSse();
