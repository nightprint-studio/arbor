import type { BisectState, BisectSession } from '../types/git';
import { corvus } from './rpc';

export const getBisectState = (tabId: string) =>
  corvus<BisectState>('get_bisect_state', { tab_id: tabId });

export const bisectStart = (tabId: string) =>
  corvus<BisectState>('bisect_start', { tab_id: tabId });

export const bisectMark = (tabId: string, hash: string, mark: 'good' | 'bad' | 'skip') =>
  corvus<BisectState>('bisect_mark', { tab_id: tabId, hash, mark });

export const bisectReset = (tabId: string) =>
  corvus<void>('bisect_reset', { tab_id: tabId });

export const bisectUndoLastMark = (tabId: string) =>
  corvus<BisectState>('bisect_undo_last_mark', { tab_id: tabId });

export const listBisectSessions = (tabId: string) =>
  corvus<BisectSession[]>('list_bisect_sessions', { tab_id: tabId });

export const saveBisectSession = (tabId: string, badHashes: string[], goodHashes: string[], name?: string) =>
  corvus<BisectSession>('save_bisect_session', { tab_id: tabId, bad_hashes: badHashes, good_hashes: goodHashes, name });

export const saveBisectResult = (tabId: string, badHashes: string[], goodHashes: string[], resultHash: string, resultMessage: string | null) =>
  corvus<BisectSession>('save_bisect_result', { tab_id: tabId, bad_hashes: badHashes, good_hashes: goodHashes, result_hash: resultHash, result_message: resultMessage });

export const resumeBisectSession = (tabId: string, sessionId: string) =>
  corvus<BisectState>('resume_bisect_session', { tab_id: tabId, session_id: sessionId });

export const renameBisectSession = (tabId: string, sessionId: string, newName: string) =>
  corvus<BisectSession>('rename_bisect_session', { tab_id: tabId, session_id: sessionId, new_name: newName });

export const deleteBisectSession = (tabId: string, sessionId: string) =>
  corvus<void>('delete_bisect_session', { tab_id: tabId, session_id: sessionId });
