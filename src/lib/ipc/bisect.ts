import { invoke } from '@tauri-apps/api/core';
import type { BisectState, BisectSession } from '../types/git';

export const getBisectState = (tabId: string) =>
  invoke<BisectState>('get_bisect_state', { tabId });

export const bisectStart = (tabId: string) =>
  invoke<BisectState>('bisect_start', { tabId });

export const bisectMark = (tabId: string, hash: string, mark: 'good' | 'bad' | 'skip') =>
  invoke<BisectState>('bisect_mark', { tabId, hash, mark });

export const bisectReset = (tabId: string) =>
  invoke<void>('bisect_reset', { tabId });

export const bisectUndoLastMark = (tabId: string) =>
  invoke<BisectState>('bisect_undo_last_mark', { tabId });

export const listBisectSessions = (tabId: string) =>
  invoke<BisectSession[]>('list_bisect_sessions', { tabId });

export const saveBisectSession = (tabId: string, badHashes: string[], goodHashes: string[], name?: string) =>
  invoke<BisectSession>('save_bisect_session', { tabId, badHashes, goodHashes, name });

export const saveBisectResult = (tabId: string, badHashes: string[], goodHashes: string[], resultHash: string, resultMessage: string | null) =>
  invoke<BisectSession>('save_bisect_result', { tabId, badHashes, goodHashes, resultHash, resultMessage });

export const resumeBisectSession = (tabId: string, sessionId: string) =>
  invoke<BisectState>('resume_bisect_session', { tabId, sessionId });

export const renameBisectSession = (tabId: string, sessionId: string, newName: string) =>
  invoke<BisectSession>('rename_bisect_session', { tabId, sessionId, newName });

export const deleteBisectSession = (tabId: string, sessionId: string) =>
  invoke<void>('delete_bisect_session', { tabId, sessionId });
