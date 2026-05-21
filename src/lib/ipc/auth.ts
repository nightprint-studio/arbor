import { invoke } from '@tauri-apps/api/core';


// ── Credential store ─────────────────────────────────────────────────────────

export const saveCredential = (host: string, username: string, password: string) =>
  invoke<void>('save_credential', { host, username, password });

export const getCredential = (host: string, username: string) =>
  invoke<string | null>('get_credential', { host, username });

export const deleteCredential = (host: string, username: string) =>
  invoke<void>('delete_credential', { host, username });

// ── Default (host-based) credentials — used by fetch/push automatically ──────

/** Save the default credential for a host/URL. Used automatically during network ops. */
export const saveDefaultCredential = (urlOrHost: string, username: string, password: string) =>
  invoke<void>('save_default_credential', { urlOrHost, username, password });

/** Returns true if a default credential is stored for the given host/URL. */
export const hasDefaultCredential = (urlOrHost: string) =>
  invoke<boolean>('has_default_credential', { urlOrHost });

/** Delete the default credential for a host/URL. */
export const deleteDefaultCredential = (urlOrHost: string) =>
  invoke<void>('delete_default_credential', { urlOrHost });

// ── Provider user identity (shared by GitHub + GitLab) ──────────────────────

/** Authenticated user on a Git provider — mirrors `ProviderUser` in Rust. */
export interface ProviderUser {
  id:         string;
  login:      string;
  name:       string | null;
  email:      string | null;
  avatar_url: string | null;
  web_url:    string | null;
}

// ── GitHub OAuth (Device Authorization Grant — RFC 8628) ────────────────────

/** Device flow info returned by the GitHub authorization endpoint.
 *  The UI must display `userCode` and direct the user to `verificationUri`. */
export interface DeviceFlowInfo {
  device_code:      string;
  user_code:        string;
  verification_uri: string;
  /** Seconds until the device code expires. */
  expires_in:       number;
  /** Suggested polling interval in seconds (handled by the backend). */
  interval:         number;
}

/** Start the GitHub Device Authorization Grant flow.  Returns the verification
 *  info the UI must display (user_code + verification_uri).  Listen for the
 *  `arbor://github-oauth-done` Tauri event (payload: `null` on success, error
 *  string on failure) to know when the user completes authorisation. */
export const startGithubDeviceFlow = () =>
  invoke<DeviceFlowInfo>('start_github_device_flow');

export const getGithubStatus = () =>
  invoke<boolean>('get_github_status');

export const getGithubUser = () =>
  invoke<ProviderUser | null>('get_github_user');

export const disconnectGithub = () =>
  invoke<void>('disconnect_github');

// ── GitLab OAuth (Authorization Code + PKCE) ────────────────────────────────

/** Start the GitLab OAuth flow. Returns the authorization URL to open in the browser.
 *  Listen for the `arbor://gitlab-oauth-done` Tauri event (payload: boolean) to know when done. */
export const startGitlabOAuth = () =>
  invoke<string>('start_gitlab_oauth');

export const getGitlabStatus = () =>
  invoke<boolean>('get_gitlab_status');

export const getGitlabUser = () =>
  invoke<ProviderUser | null>('get_gitlab_user');

export const disconnectGitlab = () =>
  invoke<void>('disconnect_gitlab');

// ── Linear OAuth (Authorization Code + PKCE) ────────────────────────────────

/** Start the Linear OAuth flow. Returns the authorization URL to open in the browser.
 *  Listen for the `arbor://linear-oauth-done` Tauri event (payload: boolean) to know when done. */
export const startLinearOAuth = () =>
  invoke<string>('start_linear_oauth');

/** Returns true if a Linear token (PAT or OAuth) is present in the keychain. */
export const getLinearOAuthStatus = () =>
  invoke<boolean>('get_linear_oauth_status');

/** Remove the Linear token from the keychain. */
export const disconnectLinearOAuth = () =>
  invoke<void>('disconnect_linear_oauth');

// ── Jira OAuth (Authorization Code + PKCE) + Basic Auth ─────────────────────

/** Start Jira OAuth 2.0 (3LO) + PKCE flow.
 *  Returns the authorization URL. Listen for `arbor://jira-oauth-done` (bool) when done. */
export const startJiraOAuth = () =>
  invoke<string>('start_jira_oauth');

/** Returns true if Jira credentials (OAuth or Basic Auth) are in the keychain. */
export const getJiraOAuthStatus = () =>
  invoke<boolean>('get_jira_oauth_status');

/** Remove all Jira credentials from the keychain. */
export const disconnectJira = () =>
  invoke<void>('disconnect_jira');

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
  invoke<OAuthOverrides>('get_oauth_overrides');

export const setOAuthOverrides = (overrides: OAuthOverrides) =>
  invoke<void>('set_oauth_overrides', { overrides });

export const getOAuthDefaults = () =>
  invoke<OAuthDefaults>('get_oauth_defaults');
