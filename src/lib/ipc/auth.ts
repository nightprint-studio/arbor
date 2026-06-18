import { corvus, platform } from './rpc';


// ── Credential store ─────────────────────────────────────────────────────────

export const saveCredential = (host: string, username: string, password: string) =>
  corvus<void>('save_credential', { host, username, password });

export const getCredential = (host: string, username: string) =>
  corvus<string | null>('get_credential', { host, username });

export const deleteCredential = (host: string, username: string) =>
  corvus<void>('delete_credential', { host, username });

// ── Default (host-based) credentials — used by fetch/push automatically ──────

/** Save the default credential for a host/URL. Used automatically during network ops. */
export const saveDefaultCredential = (urlOrHost: string, username: string, password: string) =>
  corvus<void>('save_default_credential', { url_or_host: urlOrHost, username, password });

/** Returns true if a default credential is stored for the given host/URL. */
export const hasDefaultCredential = (urlOrHost: string) =>
  corvus<boolean>('has_default_credential', { url_or_host: urlOrHost });

/** Delete the default credential for a host/URL. */
export const deleteDefaultCredential = (urlOrHost: string) =>
  corvus<void>('delete_default_credential', { url_or_host: urlOrHost });

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
