/**
 * The debug sessions the assistant left running for its conversations, for the Debug panel's
 * "started by the assistant … ends in N min" line and its Stop button — and the changes it made to the program
 * it debugs ("assistant actions"). The backend owns the truth (a session outlives the run that opened it); this
 * only mirrors it from events and one initial listing.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { ASSISTANT_DEBUG_ACTION_EVENT, listKeptDebugSessions, stopKeptDebugSession, type KeptDebugSession } from '$lib/ipc/bennu/assistant';
import { withAction, type DebugAction } from '$lib/components/bennu/assistant/assistant-debug-actions';

function createAssistantDebugStore() {
  let sessions = $state<KeptDebugSession[]>([]);
  let actions = $state<DebugAction[]>([]);
  let unlisten: UnlistenFn[] = [];
  let started = false;

  function apply(s: KeptDebugSession) {
    const rest = sessions.filter((k) => k.conversation !== s.conversation);
    sessions = s.state === 'ended' ? rest : [...rest, s];
  }

  return {
    get sessions() { return sessions; },
    /** What the assistant changed in the program it debugs, oldest first. */
    get actions() { return actions; },
    /** Listen once; then list what was already parked before this window looked. */
    async start() {
      if (started) return;
      started = true;
      unlisten = [
        await listen<KeptDebugSession>('arbor://bennu/assistant-debug', (e) => apply(e.payload)),
        await listen<DebugAction>(ASSISTANT_DEBUG_ACTION_EVENT, (e) => { actions = withAction(actions, e.payload); }),
      ];
      try { sessions = await listKeptDebugSessions(); } catch { /* nothing parked, or the backend is not up yet */ }
    },
    stopListening() { unlisten.forEach((u) => u()); unlisten = []; started = false; },
    async stop(conversation: string) {
      try { await stopKeptDebugSession(conversation); } catch { /* already over */ }
      sessions = sessions.filter((k) => k.conversation !== conversation);
    },
  };
}

export const assistantDebugStore = createAssistantDebugStore();
