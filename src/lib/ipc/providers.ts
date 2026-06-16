import { invoke } from '@tauri-apps/api/core';
import type { ProviderDescriptor, AuthStatus, OAuthStart } from '$lib/types/providers';

// Generic, by-id provider-connection IPC. Two parallel command sets (issue
// trackers, git hosts) with an identical shape — the FE drives connect /
// disconnect / OAuth / status for ANY provider through these, passing the
// provider `id`. No per-provider command anywhere in the frontend.

/** The uniform connection surface a settings section hands to the generic card.
 *  `issueProviders` / `gitProviders` below are the two domain implementations. */
export interface ProviderConnectionService {
  list(): Promise<ProviderDescriptor[]>;
  authStatus(id: string): Promise<AuthStatus>;
  connectFields(id: string, methodId: string, fields: Record<string, string>): Promise<void>;
  startOauth(id: string, methodId: string): Promise<OAuthStart>;
  disconnect(id: string): Promise<void>;
}

// ── Issue trackers ─────────────────────────────────────────────────────────
export const issueProviders: ProviderConnectionService = {
  list:          () => invoke('list_issue_providers'),
  authStatus:    (id) => invoke('issue_provider_auth_status', { id }),
  connectFields: (id, methodId, fields) => invoke('issue_provider_connect_fields', { id, methodId, fields }),
  startOauth:    (id, methodId) => invoke('issue_provider_start_oauth', { id, methodId }),
  disconnect:    (id) => invoke('issue_provider_disconnect', { id }),
};

// ── Git hosts ──────────────────────────────────────────────────────────────
export const gitProviders: ProviderConnectionService = {
  list:          () => invoke('list_git_providers'),
  authStatus:    (id) => invoke('git_provider_auth_status', { id }),
  connectFields: (id, methodId, fields) => invoke('git_provider_connect_fields', { id, methodId, fields }),
  startOauth:    (id, methodId) => invoke('git_provider_start_oauth', { id, methodId }),
  disconnect:    (id) => invoke('git_provider_disconnect', { id }),
};

/** The single Tauri event every provider's OAuth flow emits on completion.
 *  The generic card subscribes once and routes by `id`, so two concurrent
 *  OAuth logins each update their own card. */
export const PROVIDER_OAUTH_DONE_EVENT = 'arbor://provider-oauth-done';
