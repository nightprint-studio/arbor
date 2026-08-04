/**
 * Bennu index store — real indexing status for the footer, the Go-to-Class cache,
 * and a progressive "Indexing…" job card in the feedback overlay.
 *
 * The backend builds the project index on a background thread when a project opens
 * and emits `arbor://bennu/index-progress` events (`{ root, phase, state }`, with a
 * terminal `phase:"ready"`). This store:
 *   • tracks `indexing` / current `phase` so the footer shows the truth (not a
 *     hard-coded "Indexed"),
 *   • drives a multi-step Operation card (project → references → config) routed to
 *     the Bennu window,
 *   • caches the class list per root so Go-to-Class is instant (invalidated when the
 *     index rebuilds),
 *   • polls `bennu_index_stats` as a safety net (in case a start/ready event is
 *     missed on a cold-start race) and to surface the live type count.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { SvelteMap } from 'svelte/reactivity';
import { operationsStore } from '$lib/feedback/stores/operations.svelte';
import { indexStats as ipcIndexStats, classIndex as ipcClassIndex } from '$lib/ipc/bennu';
import { reindex as ipcReindex } from '$lib/ipc/bennu/nav';
import type { ClassEntry } from '$lib/types/bennu';

const OP_ID = 'bennu-index';
const PHASES: { key: string; label: string }[] = [
  { key: 'project', label: 'Project sources' },
  { key: 'dependencies', label: 'Dependencies' },
  { key: 'references', label: 'References index' },
  { key: 'config', label: 'Config graph' },
  { key: 'validation', label: 'Validation warm-up' },
];
function phaseLabel(key: string): string {
  return PHASES.find((p) => p.key === key)?.label ?? key;
}
function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

function createBennuIndexStore() {
  let indexing = $state(false);
  let phase = $state<string | null>(null);
  let typeCount = $state(0);
  // Bumped on EVERY index-progress event — including phase-end events that arrive AFTER
  // the (poll-driven) `indexing` flag has already flipped false. The config graph
  // (beans/actions/relations) finishes after the provider's `ready`, so a view keyed only
  // on `indexing` would miss it; the Index inspector re-fetches on this instead.
  let buildRevision = $state(0);
  // Live reference-walk progress (files done / total) — drives the operation card's detail
  // and a footer/status hint so a long walk on a big project visibly moves. Null when not
  // in a counted phase.
  let refProgress = $state<{ phase: string; done: number; total: number } | null>(null);
  let currentRoot: string | null = null;
  // Latched true once the current index cycle finishes (event `ready` or poll fallback).
  // Guards against a late/duplicate non-`ready` progress event — or an in-flight poll
  // response landing after `ready` — re-arming the spinner and sticking the footer on
  // "Indexing". Cleared by the next `onProjectOpen` / `reset`.
  let done = false;

  // Per-root class cache (Go-to-Class). Invalidated when the index rebuilds.
  const classCache = new SvelteMap<string, ClassEntry[]>();

  let attached = false;
  let unlisten: UnlistenFn | null = null;
  // The active safety-net poll timer + a token so a new project open cancels the old.
  let pollTimer: ReturnType<typeof setTimeout> | undefined;
  let pollToken = 0;
  // Hard-cap counter for the poll's last-resort finish: `ready` (event or `index_stats.ready`) is
  // the primary signal; the cap only rescues a BE that emits NOTHING at all (see `sawEvent`).
  let pollCount = 0;
  // Set once ANY index-progress event has arrived. The poll's hard-cap fallback is only for a BE
  // that emits no events — with a live event stream it must NOT fire, or a legitimately long
  // references phase (the O(N) reference walk, minutes on a big project) gets cut short and its step
  // never shows. When events flow, only a real `ready` (the terminal event, or `index_stats.ready` =
  // fully built) finishes the card.
  let sawEvent = false;

  function startJob(root: string) {
    operationsStore.start({
      id: OP_ID,
      title: 'Indexing project',
      subtitle: baseName(root),
      steps: PHASES.map((p) => ({ key: p.key, label: p.label })),
      current: 'project',
      target: 'bennu',
    });
  }

  function finishJob(ok: boolean) {
    operationsStore.finish(OP_ID, ok ? { summary: 'Index ready' } : { error: 'Indexing failed' });
  }

  /** Mark the index ready (from a `ready` event or the poll): stop the spinner, close
   *  the job, invalidate the class cache so Go-to fetches the fresh set. Latches `done`
   *  so late signals can't reopen it, and bumps `pollToken` so an in-flight poll can't
   *  reschedule itself past the finish. Idempotent. */
  function markReady(root: string | null) {
    if (done) return;
    done = true;
    indexing = false;
    phase = null;
    refProgress = null;
    pollToken += 1; // invalidate any in-flight poll response (it can't reschedule now)
    if (root) classCache.delete(root);
    finishJob(true);
    stopPoll();
    // A `ready` event can land before any poll returned a count → grab the final type
    // count once so the footer reads "Indexed · N". Best-effort, no token gate.
    if (root) void ipcIndexStats(root).then((s) => { typeCount = s.types; }).catch(() => {});
  }

  function stopPoll() {
    if (pollTimer) { clearTimeout(pollTimer); pollTimer = undefined; }
  }

  /** Arm a fresh indexing cycle for `root`: drop the class cache, show the job, reset
   *  the poll bookkeeping, and start the safety-net poll. Shared by `onProjectOpen`
   *  (project open) and `rebuild` (manual re-index) so both re-arm identically. */
  function beginCycle(root: string) {
    currentRoot = root;
    classCache.delete(root);
    done = false;
    indexing = true;
    phase = 'project';
    typeCount = 0;
    refProgress = null;
    startJob(root);
    pollToken += 1;
    pollCount = 0;
    sawEvent = false;
    stopPoll();
    // DELAY the first poll (don't fire it synchronously): re-opening an already-indexed project
    // leaves the BE's previous slot with `ready = true` until its fresh `open` resets it, and an
    // immediate poll could read that STALE `ready` and finish the card within milliseconds — the
    // "the step completes the moment I open it" bug. A short delay lets the fresh build reset
    // `ready = false` (and start emitting events) before we trust the stats.
    const token = pollToken;
    pollTimer = setTimeout(() => pollOnce(root, token), 1500);
  }

  function pollOnce(root: string, token: number) {
    void ipcIndexStats(root)
      .then((s) => {
        if (token !== pollToken) return;
        typeCount = s.types;
        if (s.ready) {
          markReady(root);
          return;
        }
        pollCount += 1;
        // `s.ready` (checked above) is the AUTHORITATIVE completion — the BE sets it only at the
        // very end (after the reference walk + config graph), and we read it every poll, so a missed
        // `ready` EVENT still finishes the card correctly. We deliberately do NOT finish on a "type
        // count stopped growing" heuristic: the count is fixed at the end of the PROJECT phase, long
        // before the (minutes-long) reference walk, so that heuristic flipped every step to "done"
        // prematurely whenever the event stream was missed. The only remaining fallback is a hard cap
        // for a BE that emits NOTHING at all (the listener never attached) — never reached once any
        // event has arrived, so a legitimately long walk is safe.
        if (!sawEvent && pollCount >= 40) {
          markReady(root);
          return;
        }
        pollTimer = setTimeout(() => pollOnce(root, token), 1500);
      })
      .catch(() => {
        if (token !== pollToken) return;
        // BE absent / demo — stop showing an indefinite job.
        indexing = false;
        phase = null;
        operationsStore.dismiss(OP_ID);
      });
  }

  return {
    get indexing() { return indexing; },
    get phase() { return phase; },
    get phaseLabel() { return phase ? phaseLabel(phase) : null; },
    get typeCount() { return typeCount; },
    get buildRevision() { return buildRevision; },
    /** Live reference-walk progress (`{ phase, done, total }`) or null — for a footer/status
     *  hint showing the walk moving on a big project. */
    get refProgress() { return refProgress; },

    /** Subscribe to index-progress events (once, from BennuWindow.onMount). Returns a
     *  detach fn. */
    async attach(): Promise<UnlistenFn> {
      if (attached) return () => {};
      attached = true;
      unlisten = await listen<{
        root: string; phase: string; state: string; done?: number; total?: number;
      }>(
        'arbor://bennu/index-progress',
        (e) => {
          const { root, phase: ph, state, done: doneN, total } = e.payload;
          // Events are live → disable the poll's no-event stop heuristics (see `sawEvent`),
          // so a long references phase isn't cut short by the type-count plateau / poll cap.
          sawEvent = true;
          // Bump on every event, BEFORE the `done` guard — a config/references phase can
          // still land after the poll flipped `indexing` false, and the inspector keys its
          // refresh on this so beans/actions/relations show up once their phase completes.
          //
          // ⚠ This ticks on EVERY event, including the reference walk's per-file `progress`
          // ones — thousands on a real project. It is a "something moved" signal, not a
          // "there is a new answer" one. Anything reading it must be debounced or coalesced:
          // an effect that fires an un-debounced request per tick sends one request per file
          // indexed, each on its own backend thread, and the backend stops answering
          // *everything* — which shows up as unrelated domains timing out and is miserable to
          // attribute. Use `indexing` going false when what you want is "the build settled".
          buildRevision += 1;
          if (ph === 'ready') { markReady(root); return; }
          // A non-`ready` event after the cycle finished must not reopen the spinner.
          if (done) return;
          indexing = true;
          phase = ph;
          // A `progress` event (the reference walk's files-done / total) refines the active
          // step's detail so the operation card shows real movement instead of a static
          // "References index"; other events just show the phase label.
          const hasCount = state === 'progress' && typeof doneN === 'number' && typeof total === 'number' && total > 0;
          refProgress = hasCount ? { phase: ph, done: doneN, total } : null;
          // The step's own label already names the phase — so the detail is JUST the count
          // (avoids "References index — References index · N/M"); a plain phase event has no
          // extra detail.
          const detail = hasCount
            ? `${doneN.toLocaleString()} / ${total.toLocaleString()} files`
            : null;
          operationsStore.update(OP_ID, { current: ph, activeDetail: detail });
        },
      );
      return () => { unlisten?.(); attached = false; };
    },

    /** Called when a (non-demo) project opens: show the indexing job + start the
     *  safety-net poll. Events refine the phase; the poll finishes it if a `ready`
     *  event is missed. */
    onProjectOpen(root: string) {
      beginCycle(root);
    },

    /** Manually invalidate + rebuild the whole project index (BE `bennu_reindex`) — the
     *  Index Inspector's "Rebuild" button + the "Rebuild index" palette verb. Drops the
     *  class cache immediately (so a stale/empty Go-to-Class set can't linger), fires the
     *  rebuild, then re-arms the indexing job + poll once the BE has swapped in the fresh
     *  (empty, not-ready) slot — so the poll never reads the OLD slot's stale `ready`
     *  stats and finishes early. The BE emits index-progress like an open; `ready`
     *  refreshes everything. Safe to call with no project (BE no-ops). */
    async rebuild(root: string): Promise<void> {
      // Instant feedback before the round-trip; `beginCycle` re-arms the real cycle after.
      indexing = true;
      phase = 'project';
      done = false;
      classCache.delete(root);
      await ipcReindex(root).catch(() => {});
      beginCycle(root);
    },

    /** Project closed / window teardown — clear the job + poll. */
    reset() {
      stopPoll();
      pollToken += 1;
      done = false;
      indexing = false;
      phase = null;
      currentRoot = null;
      operationsStore.dismiss(OP_ID);
    },

    /** The class list for Go-to-Class — cached per root (fetched once, refreshed when
     *  the index rebuilds). Only a NON-EMPTY result is cached / served: an empty array
     *  is truthy, so caching one (e.g. a transient empty scan right at open) would stick
     *  Go-to-Class on "no classes" forever while the inspector's un-cached fetch still
     *  shows them — so we re-fetch until classes actually come back. */
    async classesForRoot(root: string): Promise<ClassEntry[]> {
      const cached = classCache.get(root);
      if (cached && cached.length) return cached;
      const list = await ipcClassIndex(root);
      if (list.length) classCache.set(root, list);
      return list;
    },
  };
}

export const bennuIndexStore = createBennuIndexStore();
