import type { CommitNote, NoteRemoteStatus } from '../../types/corvus/git';
import { corvus } from '../rpc';

// ── Queries ───────────────────────────────────────────────────────────────────

/** List all notes for a commit across every namespace. remote_status is always 'unknown'. */
export const listCommitNotes = (tabId: string, commitOid: string) =>
  corvus<CommitNote[]>('list_commit_notes', { tab_id: tabId, commit_oid: commitOid });

/** Check remote sync status for one namespace. Called lazily when modal opens. */
export const checkNoteRemoteStatus = (tabId: string, commitOid: string, namespace: string) =>
  corvus<NoteRemoteStatus>('check_note_remote_status', { tab_id: tabId, commit_oid: commitOid, namespace });

// ── Mutations ─────────────────────────────────────────────────────────────────

/** Create or overwrite a note. */
export const saveCommitNote = (tabId: string, commitOid: string, namespace: string, content: string) =>
  corvus<void>('save_commit_note', { tab_id: tabId, commit_oid: commitOid, namespace, content });

/** Delete a note for a specific namespace. */
export const deleteCommitNote = (tabId: string, commitOid: string, namespace: string) =>
  corvus<void>('delete_commit_note', { tab_id: tabId, commit_oid: commitOid, namespace });

/** Push refs/notes/<namespace> to origin. */
export const pushNoteNamespace = (tabId: string, namespace: string) =>
  corvus<void>('push_note_namespace', { tab_id: tabId, namespace });
