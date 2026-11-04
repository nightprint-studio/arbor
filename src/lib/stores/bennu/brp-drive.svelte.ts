/**
 * Driving the running game from the inspector: the screenshot, the input (with recording and replay),
 * the statistics and the presence indicator — everything that needs the `arbor-bevy-remote` plugin in
 * the game.
 *
 * `available` is learned from `arbor.info`: `false` carries the backend's sentence about what to add,
 * which the views show in place of their controls.
 */

import {
  brpAction, brpDriveInfo, brpGamepad, brpInputCancel, brpKey, brpMouse, brpPresence, brpScreenshot, brpStats, brpText,
  type BrpDriveInfo, type BrpKeyArgs, type BrpMouseArgs, type BrpPadArgs, type BrpScreenshot, type BrpStats, type BrpTextArgs,
} from '$lib/ipc/bennu/brp';
import { gaps, parseScript, recorder, scriptToJson, type ScriptCall, type ScriptStep } from '$lib/components/bennu/brp/brp-script';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';

const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));
/** How many frame times the stats view asks for each poll. */
const STATS_HISTORY = 240;

function createBrpDriveStore() {
  let info = $state<BrpDriveInfo | null>(null);
  let unavailable = $state<string | null>(null);
  let shot = $state<BrpScreenshot | null>(null);
  let shooting = $state(false);
  let stats = $state<BrpStats | null>(null);
  let statsError = $state<string | null>(null);

  // Input recording / replay.
  let recording = $state(false);
  let steps = $state<ScriptStep[]>([]);
  let replaying = $state(false);
  let replayAt = $state(-1);
  let rec: ReturnType<typeof recorder> | null = null;
  let replayToken = 0;

  async function loadInfo(force = false) {
    if (info && !force) return;
    try {
      info = await brpDriveInfo();
      unavailable = null;
    } catch (e) {
      info = null;
      unavailable = message(e);
    }
  }

  function reset() {
    info = null;
    unavailable = null;
    shot = null;
    stats = null;
    statsError = null;
    stopRecording();
    replayToken++;
    replaying = false;
  }

  async function capture(maxWidth?: number): Promise<BrpScreenshot | null> {
    if (shooting) return shot;
    shooting = true;
    try {
      shot = await brpScreenshot(maxWidth);
      unavailable = null;
    } catch (e) {
      unavailable = message(e);
    } finally {
      shooting = false;
    }
    return shot;
  }

  async function pollStats() {
    try {
      stats = await brpStats(STATS_HISTORY);
      statsError = null;
    } catch (e) {
      statsError = message(e);
    }
  }

  // ── Input ──

  const senders: Record<ScriptCall, (a: Record<string, unknown>) => Promise<unknown>> = {
    key: (a) => brpKey(a as BrpKeyArgs),
    text: (a) => brpText(a as BrpTextArgs),
    mouse: (a) => brpMouse(a as BrpMouseArgs),
    gamepad: (a) => brpGamepad(a as BrpPadArgs),
  };

  /** Send one input call, and record it when a recording is running. A failure is a toast, not a throw. */
  async function send(call: ScriptCall, args: Record<string, unknown>): Promise<boolean> {
    try {
      await senders[call](args);
      rec?.add(call, args, performance.now());
      if (rec) steps = [...rec.steps];
      return true;
    } catch (e) {
      toastStore.show(message(e), 'error');
      return false;
    }
  }

  function startRecording() {
    rec = recorder(performance.now());
    steps = [];
    recording = true;
  }

  function stopRecording() {
    rec = null;
    recording = false;
  }

  async function replay(): Promise<void> {
    if (replaying || steps.length === 0) return;
    const token = ++replayToken;
    replaying = true;
    const waits = gaps(steps);
    try {
      for (const [i, step] of steps.entries()) {
        if (waits[i] > 0) await new Promise((r) => setTimeout(r, waits[i]));
        if (token !== replayToken) return;
        replayAt = i;
        await senders[step.call](step.args);
      }
    } catch (e) {
      toastStore.show(`Replay stopped: ${message(e)}`, 'error');
    } finally {
      if (token === replayToken) {
        replaying = false;
        replayAt = -1;
      }
    }
  }

  async function stopReplay() {
    replayToken++;
    replaying = false;
    replayAt = -1;
    try {
      await brpInputCancel();
    } catch { /* the game is gone: nothing is held */ }
  }

  function loadScript(text: string): string | null {
    const r = parseScript(text);
    if (!r.ok) return r.error;
    steps = r.steps;
    return null;
  }

  return {
    get info() { return info; },
    get unavailable() { return unavailable; },
    get shot() { return shot; },
    get shooting() { return shooting; },
    get stats() { return stats; },
    get statsError() { return statsError; },
    get recording() { return recording; },
    get steps() { return steps; },
    get replaying() { return replaying; },
    get replayAt() { return replayAt; },
    loadInfo, reset, capture, pollStats,
    send, startRecording, stopRecording, replay, stopReplay, loadScript,
    clearScript() { steps = []; },
    scriptJson: () => scriptToJson(steps),
    runAction: (name: string, arg?: unknown) => brpAction(name, arg),
    setPresence: brpPresence,
  };
}

export const brpDriveStore = createBrpDriveStore();
