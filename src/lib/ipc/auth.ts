import { platform } from './rpc';

// Credential read/write wrappers (`save_credential`, `get_credential`,
// `delete_credential` and their `*_default_credential` siblings) used to live
// here for the hand-entered credentials form. Nothing in the frontend enters
// credentials by hand any more — providers are connected through the generic
// `ProviderConnectionCard`, and the backend resolves and stores tokens itself
// during network operations. The RPC methods are still served; add a wrapper
// back if a UI ever needs to drive them directly.

// ── OAuth client-id overrides ───────────────────────────────────────────────

/** User-supplied OAuth client_id (and host, for GitLab) overrides. Empty
 *  fields fall back to the bundled defaults. The client_id is a public
 *  OAuth identifier and is stored in `~/.config/arbor/config.toml` in plain
 *  TOML — only access/refresh tokens go to the OS keychain. */
export interface OAuthOverrides {
  github: { client_id?: string | null };
  gitlab: { client_id?: string | null; base_host?: string | null };
  linear: { client_id?: string | null };
  jira:   { client_id?: string | null };
}

/** Bundled OAuth defaults — used as placeholder hints when an override is empty. */
export interface OAuthDefaults {
  github_client_id: string;
  gitlab_client_id: string;
  gitlab_base_host: string;
  linear_client_id: string;
  jira_client_id:   string;
}

export const getOAuthOverrides = () =>
  platform<OAuthOverrides>('get_oauth_overrides');

export const setOAuthOverrides = (overrides: OAuthOverrides) =>
  platform<void>('set_oauth_overrides', { overrides });

export const getOAuthDefaults = () =>
  platform<OAuthDefaults>('get_oauth_defaults');
