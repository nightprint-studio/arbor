import { invoke } from '@tauri-apps/api/core';
import type { WorktreeLink, AliasEntry, AliasGroup } from '../types/linkedWorktree';

// ── Read ────────────────────────────────────────────────────────────────────

export const listWorktreeLinks = () =>
  invoke<WorktreeLink[]>('list_worktree_links');

export const getWorktreeLink = (id: string) =>
  invoke<WorktreeLink | null>('get_worktree_link', { id });

export const getWorktreeLinkForRepo = (repoId: string) =>
  invoke<WorktreeLink | null>('get_worktree_link_for_repo', { repoId });

// ── Write ───────────────────────────────────────────────────────────────────

export const createWorktreeLink = (name: string, initialRepoIds: string[]) =>
  invoke<WorktreeLink>('create_worktree_link', { name, initialRepoIds });

export const deleteWorktreeLink = (id: string) =>
  invoke<void>('delete_worktree_link', { id });

export const renameWorktreeLink = (id: string, name: string) =>
  invoke<void>('rename_worktree_link', { id, name });

export const addWorktreeLinkMember = (linkId: string, repoId: string) =>
  invoke<void>('add_worktree_link_member', { linkId, repoId });

export const removeWorktreeLinkMember = (linkId: string, repoId: string) =>
  invoke<void>('remove_worktree_link_member', { linkId, repoId });

export const setWorktreeLinkSyncEnabled = (linkId: string, enabled: boolean) =>
  invoke<void>('set_worktree_link_sync_enabled', { linkId, enabled });

export const setWorktreeLinkMemberSyncEnabled = (linkId: string, repoId: string, enabled: boolean) =>
  invoke<void>('set_worktree_link_member_sync_enabled', { linkId, repoId, enabled });

// ── Aliases ─────────────────────────────────────────────────────────────────

export const addAliasGroup = (linkId: string, members: AliasEntry[]) =>
  invoke<AliasGroup>('add_alias_group', { linkId, members });

export const updateAliasGroup = (linkId: string, groupId: string, members: AliasEntry[]) =>
  invoke<void>('update_alias_group', { linkId, groupId, members });

export const removeAliasGroup = (linkId: string, groupId: string) =>
  invoke<void>('remove_alias_group', { linkId, groupId });
