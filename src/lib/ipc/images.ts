import { corvus } from './rpc';

/** Inline-image provider — issue trackers (`linear`/`jira`) and code-review
 *  hosts (`github`/`gitlab`). Drives which credentials the backend attaches. */
export type ImageProvider = 'linear' | 'jira' | 'github' | 'gitlab';

/**
 * Fetch an image referenced inline by an issue/MR/PR body or comment, going
 * through the provider's authenticated HTTP path so private attachments resolve.
 * Returns a `data:<mime>;base64,<...>` URL ready to drop into an `<img src>`.
 *
 * `baseUrl` is only meaningful for GitLab (the instance origin, derived from the
 * MR web URL) — it resolves relative `/uploads/...` paths and gates the token.
 */
export function fetchRemoteImage(
  url: string,
  provider: ImageProvider,
  baseUrl?: string | null,
): Promise<string> {
  return corvus('fetch_remote_image', { url, provider, base_url: baseUrl ?? null });
}
