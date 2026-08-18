/**
 * The debugger's frontend state: what the gutter draws, and what the Debug panel shows.
 *
 * Two halves that only meet at the backend.
 *
 * **Breakpoints** exist whether or not anything is running. They are a property of the
 * *project* — set in the gutter, persisted in `<root>/.arbor/bennu/config.toml` `[debug]`
 * beside the run configurations — and a launch installs whatever is there. So they hydrate
 * when a project opens and are written on every edit, with no session involved.
 *
 * **A session** is a run. Its id *is* the run id, which is what lets the console tab, Stop
 * and this panel talk about the same thing without any of them holding a reference to the
 * others. It arrives entirely through events: `debug-status` says where the session is,
 * `debug-paused` carries the stack when the program stops, `debug-breakpoints` says what the
 * VM made of each breakpoint (a class that had not loaded yet, a line with no code on it).
 *
 * Values are read **on demand and lazily**: the variables of the frame you select, the fields
 * of the object you expand. A stack of forty frames has one you are looking at, and fetching
 * the rest would be a round trip each for nothing.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { SvelteMap } from 'svelte/reactivity';
import { focusWindow } from '$lib/ipc/window';
import { bennuUiStore } from './ui.svelte';
import {
  getDebugConfig, setDebugConfig, debugResume, debugStep, debugDetach, debugMute,
  debugVariables, debugExpand, debugWatch, debugDump,
} from '$lib/ipc/bennu/debug';
import type {
  BreakpointDto, BreakpointStatusDto, DebugBreakpointsEvent, DebugConfigDto, DebugPauseDto,
  DebugStatusDto, DebugValueDto, ExceptionBreakpointDto, StackFrameDto, StepDepth,
} from '$lib/types/bennu/debug';

/** Backslashes to forward slashes — the one spelling every path is compared in here. The
 *  editor's comes from the OS and the backend's from the index, and on Windows they disagree
 *  about nothing else. */
export function canonFile(path: string): string {
  return path.replace(/\\/g, '/');
}

/** A breakpoint keyed for lookup: `file:line`, case-folded (Windows). */
function bpKey(file: string, line: number): string {
  return `${canonFile(file).toLowerCase()}:${line}`;
}

/**
 * One row of the variables tree.
 *
 * `children === null` means "not asked yet", which is not `[]` ("asked; there is nothing
 * inside"). Collapsing the two would make an object with no fields re-fetch on every expand.
 */
export interface VarNode {
  /** The path from the root — stable within one stop, which is as long as it needs to be:
   *  resuming invalidates every object handle anyway. */
  id: string;
  value: DebugValueDto;
  children: VarNode[] | null;
  loading: boolean;
  open: boolean;
  /** Why the expansion failed, when it did. */
  error: string;
}

/** A watch and its last evaluation against the selected frame. */
export interface Watch {
  expression: string;
  value: DebugValueDto | null;
  /** What went wrong. "No variable named `order` here" is the *normal* answer for a watch
   *  looked at from the wrong frame, so it belongs on the row rather than in a toast. */
  error: string;
}

/**
 * A value being read whole, in the inspect modal.
 *
 * A **snapshot**: the text was rendered against one stop and stays readable after the program runs
 * on, which is deliberate — you open it to read a value, and having it blank itself because something
 * continued in the background would lose the thing you were looking at.
 */
export interface Inspect {
  /** The row it was opened from — its name and type are the modal's title. */
  value: DebugValueDto;
  text: string;
  nodes: number;
  /** Whether a cap was hit. Shown, because a dump silently cut reads as complete. */
  truncated: boolean;
  loading: boolean;
  error: string;
}

function node(value: DebugValueDto, id: string): VarNode {
  return { id, value, children: null, loading: false, open: false, error: '' };
}

function createBennuDebugStore() {
  // ── the project's breakpoints (no session required) ─────────────────────────
  const configs = new SvelteMap<string, DebugConfigDto>();
  const hydrated = new Set<string>();
  /** What a live VM made of each breakpoint, keyed `file:line`. Emptied when a session ends —
   *  a verification is a fact about a running program, not about the project. */
  let statuses = $state<Map<string, BreakpointStatusDto>>(new Map());

  // ── the session ────────────────────────────────────────────────────────────
  let sessionId = $state<string | null>(null);
  let status = $state<DebugStatusDto['status'] | null>(null);
  let vm = $state('');
  let message = $state('');
  /** Which debugger is underneath. Drives the language-specific prose in the panel; a session with
   *  no engine reported is a JVM one. */
  let engine = $state<'jvm' | 'native'>('jvm');
  /** The standing caveat, when the adapter in use cannot render Rust's own types. */
  let note = $state('');
  let stopped = $state<DebugPauseDto | null>(null);
  let selectedFrame = $state(0);
  let variables = $state<VarNode[]>([]);
  let variablesLoading = $state(false);
  let watches = $state<Watch[]>([]);
  /** The value being read whole, or null when the modal is closed. */
  let inspect = $state<Inspect | null>(null);
  /** Breakpoints muted for THIS session — set and listed, but not installed. Deliberately not
   *  persisted: muting is something you do to finish the run in front of you, and a debugger
   *  that silently ignored your breakpoints tomorrow because of it would be a trap. */
  let muted = $state(false);

  let attached = false;
  let unlisteners: UnlistenFn[] = [];

  function emptyConfig(): DebugConfigDto {
    return { breakpoints: [], exceptions: [], watches: [] };
  }

  function configOf(root: string): DebugConfigDto {
    let cfg = configs.get(root);
    if (!cfg) {
      cfg = emptyConfig();
      configs.set(root, cfg);
    }
    return cfg;
  }

  /** Replace a root's config with a patched copy — a copy, so a `$derived` that ends in
   *  `configs.get(root)` sees a new identity and propagates. */
  function patch(root: string, next: Partial<DebugConfigDto>): void {
    configs.set(root, { ...configOf(root), ...next });
  }

  /**
   * Write the section back and push its live half to whatever is running.
   *
   * Not debounced, unlike the run configurations: toggling a breakpoint is one deliberate
   * click rather than a keystroke, and it has to reach a *running* program before the next
   * line executes. Best-effort — a failed write leaves the session's breakpoints in place.
   */
  async function persist(root: string): Promise<void> {
    try {
      await setDebugConfig(root, configOf(root));
    } catch {
      /* the in-memory set still applies; the next edit retries */
    }
  }

  /** The debounced write, for the one caller that fires on **typing**: a breakpoint following
   *  an edit down the file is not a decision, it is bookkeeping, and pressing Enter fifty times
   *  should not be fifty writes. */
  const moveTimers = new Map<string, ReturnType<typeof setTimeout>>();
  function persistSoon(root: string): void {
    clearTimeout(moveTimers.get(root));
    moveTimers.set(root, setTimeout(() => void persist(root), 400));
  }

  /** Everything the selected frame decides — its variables and every watch. Both are
   *  invalidated by choosing another frame and by resuming. */
  async function refresh(): Promise<void> {
    if (!sessionId || status !== 'paused') return;
    const id = sessionId;
    const frame = selectedFrame;
    variablesLoading = true;
    variables = [];
    try {
      const rows = await debugVariables(id, frame);
      // A stop that happened while this was in flight makes these rows a description of a
      // frame nobody is looking at any more.
      if (sessionId !== id || selectedFrame !== frame) return;
      variables = rows.map((v, i) => node(v, `${i}:${v.name}`));
    } catch (e) {
      if (sessionId === id && selectedFrame === frame) message = clean(e);
    } finally {
      variablesLoading = false;
    }
    for (const w of watches) await evaluate(w);
  }

  /** Evaluate one watch against the selected frame, in place. */
  async function evaluate(w: Watch): Promise<void> {
    if (!sessionId || status !== 'paused') {
      w.value = null;
      w.error = '';
      return;
    }
    try {
      w.value = await debugWatch(sessionId, selectedFrame, w.expression);
      w.error = '';
    } catch (e) {
      w.value = null;
      // The backend's message is the useful one: "no field named `total` on Customer" tells
      // you what to type next.
      w.error = clean(e);
    }
  }

  /** Forget everything that only makes sense while the program is standing still. Every frame
   *  id and every object handle the VM handed out is invalid the moment it resumes — keeping
   *  them would mean the next expand asks about a frame that no longer exists, which does not
   *  fail loudly, it answers about something else. */
  function forgetStop(): void {
    stopped = null;
    variables = [];
    selectedFrame = 0;
    for (const w of watches) {
      w.value = null;
      w.error = '';
    }
  }

  /**
   * Come to the front when the program stops — the way IntelliJ does.
   *
   * A breakpoint fires because of something happening in *another* window: the browser you
   * just clicked in, a terminal, a request from somewhere else entirely. The editor is the
   * only place the answer is, and leaving it behind whatever you were looking at means every
   * stop begins with hunting for the window. The console is raised with it, since a stop you
   * cannot see the stack of is a stop you have to go looking for twice.
   *
   * **Not on a step.** A step is something you did *here*, with the window already in front;
   * raising it again would be the one case where this is pure noise.
   */
  function surface(reason: DebugPauseDto['reason']): void {
    if (reason === 'step') return;
    bennuUiStore.showBottom('run');
    // Best-effort: focus is the operating system's to give, and failing to get it is not
    // worth an error over — the panel is open either way.
    void focusWindow(getCurrentWindow().label).catch(() => {});
  }

  function detach(): void {
    for (const f of unlisteners) f();
    unlisteners = [];
    attached = false;
  }

  return {
    // ── breakpoints ──────────────────────────────────────────────────────────
    /** Every breakpoint of a project. */
    breakpointsFor(root: string): BreakpointDto[] {
      return configs.get(root)?.breakpoints ?? [];
    },
    /** The breakpoints in one file — what a gutter renders. */
    breakpointsIn(root: string, file: string): BreakpointDto[] {
      const want = canonFile(file).toLowerCase();
      return (configs.get(root)?.breakpoints ?? []).filter(
        (b) => canonFile(b.file).toLowerCase() === want,
      );
    },
    /** What the running VM made of a breakpoint, if anything is running. */
    statusOf(file: string, line: number): BreakpointStatusDto | null {
      return statuses.get(bpKey(file, line)) ?? null;
    },
    exceptionsFor(root: string): ExceptionBreakpointDto[] {
      return configs.get(root)?.exceptions ?? [];
    },

    /** Read the project's persisted debug section. Idempotent per root — it is called from an
     *  effect that may re-run, and re-reading would throw away edits made since. */
    async load(root: string): Promise<void> {
      if (!root || hydrated.has(root)) return;
      hydrated.add(root);
      try {
        const cfg = await getDebugConfig(root);
        configs.set(root, {
          breakpoints: cfg.breakpoints ?? [],
          exceptions: cfg.exceptions ?? [],
          watches: cfg.watches ?? [],
        });
        watches = (cfg.watches ?? []).map((expression) => ({
          expression,
          value: null,
          error: '',
        }));
      } catch {
        configs.set(root, emptyConfig());
      }
    },

    /** Set or clear a breakpoint on a line — what clicking the gutter does. */
    toggleBreakpoint(root: string, file: string, line: number): void {
      const key = bpKey(file, line);
      const current = configOf(root).breakpoints;
      const next = current.some((b) => bpKey(b.file, b.line) === key)
        ? current.filter((b) => bpKey(b.file, b.line) !== key)
        : [...current, { file: canonFile(file), line, enabled: true }];
      patch(root, { breakpoints: next });
      void persist(root);
    },

    /** Keep a breakpoint but stop stopping at it — the alternative to deleting one you will
     *  want back in ten minutes. */
    setBreakpointEnabled(root: string, file: string, line: number, enabled: boolean): void {
      const key = bpKey(file, line);
      patch(root, {
        breakpoints: configOf(root).breakpoints.map((b) =>
          bpKey(b.file, b.line) === key ? { ...b, enabled } : b,
        ),
      });
      void persist(root);
    },

    removeBreakpoint(root: string, file: string, line: number): void {
      const key = bpKey(file, line);
      patch(root, {
        breakpoints: configOf(root).breakpoints.filter((b) => bpKey(b.file, b.line) !== key),
      });
      void persist(root);
    },

    /**
     * Editing moved some flagged lines in `file` — follow them.
     *
     * A breakpoint is remembered by line number, and a line number is only true until someone
     * types above it. The editor knows where each line went (CodeMirror maps the positions);
     * this applies the result. Two breakpoints landing on the same line collapse into one,
     * which is what deleting the lines between them means.
     */
    moveBreakpoints(root: string, file: string, moves: readonly { from: number; to: number }[]): void {
      if (!moves.length) return;
      const want = canonFile(file).toLowerCase();
      const by = new Map(moves.map((m) => [m.from, m.to]));
      const seen = new Set<string>();
      const next: BreakpointDto[] = [];
      for (const b of configOf(root).breakpoints) {
        const mine = canonFile(b.file).toLowerCase() === want;
        const moved = mine ? { ...b, line: by.get(b.line) ?? b.line } : b;
        const key = bpKey(moved.file, moved.line);
        if (seen.has(key)) continue;
        seen.add(key);
        next.push(moved);
      }
      patch(root, { breakpoints: next });
      persistSoon(root);
    },

    /** Drop every breakpoint in the project. */
    clearBreakpoints(root: string): void {
      patch(root, { breakpoints: [] });
      void persist(root);
    },

    setExceptions(root: string, exceptions: ExceptionBreakpointDto[]): void {
      patch(root, { exceptions });
      void persist(root);
    },

    // ── the session ──────────────────────────────────────────────────────────
    get sessionId() { return sessionId; },
    get status() { return status; },
    /** The VM's own description, for the panel's status line. */
    get vm() { return vm; },
    get message() { return message; },
    /** `jvm` or `native` — which language's watch rules the panel should describe. */
    get engine() { return engine; },
    /** A standing caveat about what this session can show, or empty. */
    get note() { return note; },
    /** Whether a debug session exists at all (however it is doing). */
    get live() { return sessionId !== null && status !== 'terminated'; },
    /** Whether the program is stopped right now — what enables the step buttons. */
    get paused() { return status === 'paused' && stopped !== null; },
    /** Whether this session's breakpoints are muted. */
    get muted() { return muted; },
    get stopped() { return stopped; },
    get frames(): StackFrameDto[] { return stopped?.frames ?? []; },
    get selectedFrame() { return selectedFrame; },
    get variables() { return variables; },
    get variablesLoading() { return variablesLoading; },
    get watches() { return watches; },
    /** The value being read whole in the inspect modal, or null. */
    get inspect() { return inspect; },
    /** The frame the panel is showing — what "go to source" and the watches use. */
    get currentFrame(): StackFrameDto | null {
      return stopped?.frames[selectedFrame] ?? null;
    },

    /** Attach the debugger's event listeners. Called once from BennuWindow.onMount; returns a
     *  detach fn for cleanup. Idempotent. */
    async attach(): Promise<UnlistenFn> {
      if (attached) return detach;
      attached = true;
      const add = (f: UnlistenFn) => unlisteners.push(f);

      add(
        await listen<DebugStatusDto>('arbor://bennu/debug-status', (e) => {
          const p = e.payload;
          sessionId = p.session_id;
          status = p.status;
          message = p.message;
          // Sent once, when the VM answers; later statuses leave it empty rather than
          // repeating it, so an empty one must not blank what is already known.
          if (p.vm) vm = p.vm;
          if (p.engine) engine = p.engine;
          // Unlike `vm`, this is sent on every status and an empty one is meaningful: it means the
          // caveat no longer applies.
          note = p.note ?? '';
          if (p.status !== 'paused') forgetStop();
          if (p.status === 'terminated') statuses = new Map();
          // A fresh session starts unmuted: muting is scoped to the run it was done in.
          if (p.status === 'starting') muted = false;
        }),
      );

      add(
        await listen<DebugPauseDto>('arbor://bennu/debug-paused', (e) => {
          stopped = e.payload;
          sessionId = e.payload.session_id;
          status = 'paused';
          // The innermost frame of THIS project, else the innermost of all. Landing on
          // `ArrayList.add` because that is frame 0 would be exactly where it stopped and
          // never what anyone wanted to look at.
          const own = e.payload.frames.findIndex((f) => f.project);
          selectedFrame = own === -1 ? 0 : own;
          void refresh();
          surface(e.payload.reason);
        }),
      );

      add(
        await listen<DebugBreakpointsEvent>('arbor://bennu/debug-breakpoints', (e) => {
          const next = new Map<string, BreakpointStatusDto>();
          for (const s of e.payload.breakpoints) next.set(bpKey(s.file, s.line), s);
          statuses = next;
        }),
      );

      return detach;
    },

    // ── running on ───────────────────────────────────────────────────────────
    async resume(): Promise<void> {
      if (!sessionId) return;
      try {
        await debugResume(sessionId);
      } catch (e) {
        message = clean(e);
      }
    },

    /** Mute or unmute this session's breakpoints. Optimistic: the button flips at once and
     *  reverts if the backend refuses, because a toggle that waits for a round trip to move
     *  reads as broken. */
    async toggleMute(): Promise<void> {
      if (!sessionId) return;
      const next = !muted;
      muted = next;
      try {
        await debugMute(sessionId, next);
      } catch (e) {
        muted = !next;
        message = clean(e);
      }
    },

    async step(depth: StepDepth): Promise<void> {
      if (!sessionId || status !== 'paused') return;
      try {
        await debugStep(sessionId, depth);
      } catch (e) {
        message = clean(e);
      }
    },

    /** Detach, leaving the program running. Stopping the *program* is the Run console's Stop —
     *  a server you attached to in order to look at one request should not die because you
     *  finished looking. */
    async detachSession(): Promise<void> {
      if (!sessionId) return;
      try {
        await debugDetach(sessionId);
      } catch {
        /* it is going away either way */
      }
      status = 'terminated';
      forgetStop();
      statuses = new Map();
    },

    /** Show another frame: its variables, and the watches re-evaluated against it. */
    selectFrame(index: number): void {
      if (index === selectedFrame) return;
      selectedFrame = index;
      void refresh();
    },

    /** Open or close a node of the variables tree, fetching its children the first time. */
    async toggleNode(target: VarNode): Promise<void> {
      target.open = !target.open;
      if (!target.open || target.children !== null || !target.value.object || !sessionId) return;
      target.loading = true;
      try {
        const rows = await debugExpand(sessionId, target.value.object);
        target.children = rows.map((v, i) => node(v, `${target.id}/${i}:${v.name}`));
        target.error = '';
      } catch (e) {
        target.children = [];
        target.error = clean(e);
      } finally {
        target.loading = false;
      }
    },

    /**
     * Read one value whole, as RON-shaped text.
     *
     * The way out of the lazy tree: a struct whose fields are structs is nineteen disclosure
     * triangles before it can be read. One round trip fetches the whole subtree.
     *
     * Opens the modal immediately with a spinner rather than after the answer, because the walk is a
     * round trip per node against a suspended program and a click that does nothing for a second
     * reads as a click that did nothing.
     */
    async inspectValue(value: DebugValueDto): Promise<void> {
      if (!sessionId) return;
      const id = sessionId;
      inspect = { value, text: '', nodes: 0, truncated: false, loading: true, error: '' };
      try {
        const dump = await debugDump(id, value);
        // The user may have closed it, or opened another row, while the walk was running — writing
        // this answer into a modal that is now about something else would show the wrong value under
        // the right title.
        if (inspect?.value !== value) return;
        inspect = { value, ...dump, loading: false, error: '' };
      } catch (e) {
        if (inspect?.value !== value) return;
        inspect = { value, text: '', nodes: 0, truncated: false, loading: false, error: clean(e) };
      }
    },

    closeInspect(): void {
      inspect = null;
    },

    // ── watches ──────────────────────────────────────────────────────────────
    /** Add a watch. A watch is a **path** — `order`, `order.customer.name`, `items[2]` — and
     *  the backend refuses anything else by name rather than approximating it. */
    async addWatch(root: string, expression: string): Promise<void> {
      const trimmed = expression.trim();
      if (!trimmed || watches.some((w) => w.expression === trimmed)) return;
      watches = [...watches, { expression: trimmed, value: null, error: '' }];
      patch(root, { watches: watches.map((w) => w.expression) });
      void persist(root);
      await evaluate(watches[watches.length - 1]);
    },

    removeWatch(root: string, expression: string): void {
      watches = watches.filter((w) => w.expression !== expression);
      patch(root, { watches: watches.map((w) => w.expression) });
      void persist(root);
    },
  };
}

/** An RPC rejection as a sentence. The seam carries errors as strings, and `Error: ` in front
 *  of one is this layer's noise, not the backend's message. */
function clean(e: unknown): string {
  return String(e instanceof Error ? e.message : e).replace(/^Error:\s*/, '');
}

export const bennuDebugStore = createBennuDebugStore();
