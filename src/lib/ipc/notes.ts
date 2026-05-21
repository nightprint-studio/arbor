import { invoke } from '@tauri-apps/api/core';
import type { CommitNote, NoteRemoteStatus } from '../types/git';

// ── Queries ───────────────────────────────────────────────────────────────────

/** List all notes for a commit across every namespace. remote_status is always 'unknown'. */
export const listCommitNotes = (tabId: string, commitOid: string) =>
  invoke<CommitNote[]>('list_commit_notes', { tabId, commitOid });

/** Check remote sync status for one namespace. Called lazily when modal opens. */
export const checkNoteRemoteStatus = (tabId: string, commitOid: string, namespace: string) =>
  invoke<NoteRemoteStatus>('check_note_remote_status', { tabId, commitOid, namespace });

// ── Mutations ─────────────────────────────────────────────────────────────────

/** Create or overwrite a note. */
export const saveCommitNote = (tabId: string, commitOid: string, namespace: string, content: string) =>
  invoke<void>('save_commit_note', { tabId, commitOid, namespace, content });

/** Delete a note for a specific namespace. */
export const deleteCommitNote = (tabId: string, commitOid: string, namespace: string) =>
  invoke<void>('delete_commit_note', { tabId, commitOid, namespace });

/** Push refs/notes/<namespace> to origin. */
export const pushNoteNamespace = (tabId: string, namespace: string) =>
  invoke<void>('push_note_namespace', { tabId, namespace });
