/**
 * The Bevy Remote console: call any method the connected game lists, see the raw answer.
 *
 * History lives for the session only (it is a scratch pad, not a setting). The method list is the
 * game's own `rpc.discover`, fetched when the console opens against a new connection.
 */

import { brpCall, brpMethods } from '$lib/ipc/bennu/brp';
import { parseParams, pushHistory, type ConsoleEntry } from '$lib/components/bennu/brp/brp-console';
import { toJson } from '$lib/components/bennu/brp/brp-format';

const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

function createBrpConsoleStore() {
  let methods = $state<string[]>([]);
  let history = $state<ConsoleEntry[]>([]);
  let running = $state(false);
  /** The composer's fields, here so the history can load a past call into them. */
  let method = $state('');
  let params = $state('');
  /** The last answer, as pretty JSON; `error` when the game (or the params) refused. */
  let output = $state<{ method: string; text: string; error: boolean } | null>(null);
  let methodsFor = -1;

  /** `generation` identifies the connection the list belongs to: a restarted game may offer other methods. */
  async function loadMethods(generation: number) {
    if (methodsFor === generation) return;
    methodsFor = generation;
    try {
      methods = await brpMethods();
    } catch {
      methods = [];
      methodsFor = -1;
    }
  }

  async function run(method: string, paramsText: string) {
    const name = method.trim();
    if (!name || running) return;
    const parsed = parseParams(paramsText);
    if (!parsed.ok) {
      output = { method: name, text: parsed.error, error: true };
      return;
    }
    running = true;
    try {
      const result = await brpCall(name, parsed.value);
      output = { method: name, text: toJson(result), error: false };
      history = pushHistory(history, { method: name, params: paramsText.trim() });
    } catch (e) {
      output = { method: name, text: message(e), error: true };
    } finally {
      running = false;
    }
  }

  return {
    get method() { return method; },
    set method(v: string) { method = v; },
    get params() { return params; },
    set params(v: string) { params = v; },
    get methods() { return methods; },
    get history() { return history; },
    get running() { return running; },
    get output() { return output; },
    loadMethods,
    run,
    clearHistory() { history = []; },
    clearOutput() { output = null; },
  };
}

export const brpConsoleStore = createBrpConsoleStore();
