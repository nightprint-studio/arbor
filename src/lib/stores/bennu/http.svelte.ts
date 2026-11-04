/**
 * Bennu › HTTP files — `.http` / `.rest` requests run from the editor, through Hirundo's engine.
 *
 * Bennu owns the editor and the panel; the parsing, the variables (the file's `@name = …`, the selected
 * environment of `http-client.env.json`, the dynamic ones), the protection of a production-like environment and
 * the sending are `hirundo-be`'s, borrowed like Bennu borrows Picus. This store holds what the panel shows: the
 * environment picked, the requests of the open file (for the gutter), the last answer with its checks, and a
 * history of what was sent.
 */
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { httpCheck, httpEnvs, httpOutline, httpRun, testsRun, type HttpDiag, type HttpEnvBrief, type HttpOutlineEntry } from '$lib/ipc/hirundo';
import { ensureBackend } from '$lib/ipc/rpc';
import { setEnvironmentVariables } from '$lib/components/bennu/http/http-completion';
import type { ResponseData, TestReport } from '$lib/types/hirundo';
import { bennuUiStore } from './ui.svelte';

export const isHttpFile = (path: string | null): boolean => !!path && /\.(http|rest)$/i.test(path);

export interface HttpHistoryEntry {
  at: number;
  file: string;
  name: string;
  method: string;
  url: string;
  status: number | null;
  time_ms: number | null;
}

export interface PendingRun {
  text: string;
  line: number;
  dir: string | null;
  name: string;
  env: string;
}

const MAX_HISTORY = 100;
let counter = 0;

const dirOf = (path: string) => path.replace(/[\\/][^\\/]*$/, '') || null;

/** Character columns of a line → UTF-8 byte offsets into the whole text (what the editor's diagnostics use). */
export function diagnosticsToBytes(text: string, diags: HttpDiag[]): { from: number; to: number; severity: 'error' | 'warning'; message: string }[] {
  const lines = text.split('\n');
  const enc = new TextEncoder();
  const lineStart: number[] = [];
  let at = 0;
  for (const l of lines) { lineStart.push(at); at += enc.encode(l).length + 1; }
  const col = (line: number, c: number) => (lineStart[line - 1] ?? 0) + enc.encode((lines[line - 1] ?? '').slice(0, c)).length;
  return diags.map((d) => ({ from: col(d.line, d.start), to: col(d.line, d.end), severity: d.error ? 'error' : 'warning', message: d.message }));
}

function createBennuHttp() {
  let envs = $state<HttpEnvBrief[]>([]);
  let env = $state<string | null>(null);
  let outline = $state<HttpOutlineEntry[]>([]);
  let diagnostics = $state<{ from: number; to: number; severity: 'error' | 'warning'; message: string }[]>([]);
  let running = $state(false);
  let file = $state<string | null>(null);
  let name = $state('');
  let response = $state<ResponseData | null>(null);
  let report = $state<TestReport | null>(null);
  let error = $state<string | null>(null);
  let unresolved = $state<string[]>([]);
  let history = $state<HttpHistoryEntry[]>([]);
  let pending = $state<PendingRun | null>(null);
  let ready: Promise<void> | null = null;
  /** Bumped by the palette: the editor runs the request at its caret. */
  let runAtCaretTick = $state(0);

  const protectedEnv = $derived(envs.find((e) => e.name === env)?.protected ?? false);

  function backend(): Promise<void> {
    ready ??= ensureBackend('hirundo').catch((e) => { ready = null; throw e; });
    return ready;
  }

  /** The environments beside a file; keeps the selection when it is still there, else the first. */
  async function loadEnvs(path: string) {
    try {
      await backend();
      envs = await httpEnvs(dirOf(path));
      if (!envs.some((e) => e.name === env)) env = envs[0]?.name ?? null;
      setEnvironmentVariables(envs.find((e) => e.name === env)?.vars ?? []);
    } catch { envs = []; env = null; }
  }

  /** Re-read the requests (for the gutter) and the problems of the open text. */
  async function refresh(path: string, text: string) {
    try {
      await backend();
      const [o, d] = await Promise.all([httpOutline(text), httpCheck(text, dirOf(path), env)]);
      outline = o;
      diagnostics = diagnosticsToBytes(text, d);
    } catch { /* the engine is not up yet: the next edit asks again */ }
  }

  async function run(path: string, text: string, line: number, confirmed = false) {
    if (running) return;
    running = true;
    error = null; unresolved = []; report = null;
    file = path;
    if (!(bennuUiStore.rightPanel === 'http')) bennuUiStore.toggleRight('http');
    try {
      await backend();
      const dir = dirOf(path);
      const result = await httpRun({ text, line, dir, env, request_id: `http-${++counter}`, confirmed });
      if (result.outcome === 'unresolved') {
        name = result.name; response = null; unresolved = result.names;
      } else if (result.outcome === 'needs_confirmation') {
        name = result.name; pending = { text, line, dir, name: result.name, env: result.env };
      } else {
        name = result.name;
        response = result.response;
        if (result.tests.trim()) report = await testsRun(result.tests, result.response).catch(() => null);
        for (const w of result.warnings) toastStore.show(w, 'info');
        history.unshift({ at: Date.now(), file: path, name: result.name, method: result.method, url: result.url, status: result.response.status, time_ms: result.response.time_ms });
        if (history.length > MAX_HISTORY) history.length = MAX_HISTORY;
      }
    } catch (e) {
      response = null;
      error = String(e).includes('cancelled') ? null : String(e);
    } finally {
      running = false;
    }
  }

  /** Every request of the file, in order; stops at the first one that cannot be sent. */
  async function runAll(path: string, text: string) {
    await refresh(path, text);
    for (const o of outline) {
      await run(path, text, o.request_line);
      if (error || unresolved.length || pending) break;
    }
  }

  return {
    get envs() { return envs; },
    get env() { return env; },
    get outline() { return outline; },
    get diagnostics() { return diagnostics; },
    get running() { return running; },
    get file() { return file; },
    get name() { return name; },
    get response() { return response; },
    get report() { return report; },
    get error() { return error; },
    get unresolved() { return unresolved; },
    get history() { return history; },
    get pending() { return pending; },
    get protectedEnv() { return protectedEnv; },
    get runAtCaretTick() { return runAtCaretTick; },
    requestRunAtCaret() { runAtCaretTick += 1; },
    loadEnvs, refresh, run, runAll,
    selectEnv(next: string | null) {
      env = next;
      setEnvironmentVariables(envs.find((e) => e.name === next)?.vars ?? []);
    },
    async confirmPending() {
      const p = pending;
      pending = null;
      if (p && file) await run(file, p.text, p.line, true);
    },
    dismissPending() { pending = null; },
    clearHistory() { history = []; },
  };
}

export const bennuHttp = createBennuHttp();
