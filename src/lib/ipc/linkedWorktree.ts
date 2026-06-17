import { corvus } from './rpc';
import type { WorktreeLink, AliasEntry, AliasGroup } from '../types/linkedWorktree';

// ── Read ────────────────────────────────────────────────────────────────────

export const listWorktreeLinks = () =>
  corvus<WorktreeLink[]>('list_worktree_links');

export const getWorktreeLink = (id: string) =>
  corvus<WorktreeLink | null>('get_worktree_link', { id });

export const getWorktreeLinkForRepo = (repoId: string) =>
  corvus<WorktreeLink | null>('get_worktree_link_for_repo', { repo_id: repoId });

// ── Write ───────────────────────────────────────────────────────────────────

export const createWorktreeLink = (name: string, initialRepoIds: string[]) =>
  corvus<WorktreeLink>('create_worktree_link', { name, initial_repo_ids: initialRepoIds });

export const deleteWorktreeLink = (id: string) =>
  corvus<void>('delete_worktree_link', { id });

export const renameWorktreeLink = (id: string, name: string) =>
  corvus<void>('rename_worktree_link', { id, name });

export const addWorktreeLinkMember = (linkId: string, repoId: string) =>
  corvus<void>('add_worktree_link_member', { link_id: linkId, repo_id: repoId });

export const removeWorktreeLinkMember = (linkId: string, repoId: string) =>
  corvus<void>('remove_worktree_link_member', { link_id: linkId, repo_id: repoId });

export const setWorktreeLinkSyncEnabled = (linkId: string, enabled: boolean) =>
  corvus<void>('set_worktree_link_sync_enabled', { link_id: linkId, enabled });

export const setWorktreeLinkMemberSyncEnabled = (linkId: string, repoId: string, enabled: boolean) =>
  corvus<void>('set_worktree_link_member_sync_enabled', { link_id: linkId, repo_id: repoId, enabled });

// ── Aliases ─────────────────────────────────────────────────────────────────

export const addAliasGroup = (linkId: string, members: AliasEntry[]) =>
  corvus<AliasGroup>('add_alias_group', { link_id: linkId, members });

export const updateAliasGroup = (linkId: string, groupId: string, members: AliasEntry[]) =>
  corvus<void>('update_alias_group', { link_id: linkId, group_id: groupId, members });

export const removeAliasGroup = (linkId: string, groupId: string) =>
  corvus<void>('remove_alias_group', { link_id: linkId, group_id: groupId });
