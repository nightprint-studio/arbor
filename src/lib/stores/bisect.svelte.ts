import type { BisectState, BisectSession } from '../types/git';
import {
  getBisectState, bisectStart, bisectMark, bisectReset, bisectUndoLastMark,
  listBisectSessions, saveBisectSession, saveBisectResult,
  resumeBisectSession, renameBisectSession, deleteBisectSession,
} from '../ipc/bisect';

function createBisectStore() {
  let state    = $state<BisectState | null>(null);
  let sessions = $state<BisectSession[]>([]);

  async function load(tabId: string) {
    try { state = await getBisectState(tabId); } catch { state = null; }
  }

  async function loadSessions(tabId: string) {
    try { sessions = await listBisectSessions(tabId); } catch { sessions = []; }
  }

  async function start(tabId: string): Promise<BisectState> {
    const s = await bisectStart(tabId);
    state = s;
    return s;
  }

  async function mark(tabId: string, hash: string, markType: 'good' | 'bad' | 'skip'): Promise<BisectState> {
    const s = await bisectMark(tabId, hash, markType);
    state = s;
    // Auto-save when result is found
    if (s.result_hash) {
      try {
        const saved = await saveBisectResult(
          tabId, s.bad_hashes, s.good_hashes,
          s.result_hash, s.result_message ?? null,
        );
        sessions = [saved, ...sessions.filter(x => x.id !== saved.id)];
      } catch { /* non-critical */ }
    }
    return s;
  }

  async function reset(tabId: string) {
    await bisectReset(tabId);
    state = null;
  }

  async function undoLastMark(tabId: string): Promise<BisectState> {
    const s = await bisectUndoLastMark(tabId);
    state = s;
    return s;
  }

  async function saveAndPause(tabId: string, name?: string): Promise<BisectSession> {
    if (!state) throw new Error('No active bisect session');
    const saved = await saveBisectSession(
      tabId, state.bad_hashes, state.good_hashes, name,
    );
    sessions = [saved, ...sessions.filter(x => x.id !== saved.id)];
    // Reset local state (git bisect was reset by the command)
    state = null;
    return saved;
  }

  async function resume(tabId: string, sessionId: string): Promise<BisectState> {
    const s = await resumeBisectSession(tabId, sessionId);
    state = s;
    return s;
  }

  async function renameSession(tabId: string, sessionId: string, newName: string): Promise<void> {
    const updated = await renameBisectSession(tabId, sessionId, newName);
    sessions = sessions.map(s => s.id === sessionId ? updated : s);
  }

  async function deleteSession(tabId: string, sessionId: string): Promise<void> {
    await deleteBisectSession(tabId, sessionId);
    sessions = sessions.filter(s => s.id !== sessionId);
  }

  function clear() {
    state = null;
  }

  return {
    get state()    { return state; },
    get sessions() { return sessions; },
    load, loadSessions, start, mark, reset, undoLastMark,
    saveAndPause, resume, renameSession, deleteSession, clear,
  };
}

export const bisectStore = createBisectStore();
