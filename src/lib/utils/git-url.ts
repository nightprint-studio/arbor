/**
 * Git remote URL utilities — mirror of `src-tauri/src/git/url.rs`.
 *
 * Frontend-side fuzzy normalisation is used by the deep-link dispatcher to
 * reason about URLs *before* the IPC round-trip to the backend's
 * `find_repo_by_remote_url` (which uses the same algorithm).  Keeping the
 * algorithm in lock-step with Rust is what guarantees that an `arbor://…`
 * URL surfaced in the UI matches the same registry entry the backend would
 * resolve it against.
 */

/**
 * Reduce any git remote URL to a canonical `host/owner/repo` key suitable
 * for fuzzy equality across schemes (https / ssh / scp-style), credentials,
 * `.git` suffix, trailing slashes and case differences.
 *
 * Returns `null` when the URL doesn't yield a `(host, path)` pair.
 *
 * Examples — all return `"github.com/foo/bar"`:
 *   https://github.com/foo/bar.git
 *   https://USER:tok@github.com/Foo/Bar.git/
 *   git@github.com:foo/bar.git
 *   ssh://git@github.com:22/foo/bar
 */
export function canonicalKey(input: string): string | null {
  const s = input.trim();
  if (!s) return null;

  // Drop scheme, userinfo, port → "host/path".
  let hostPath: string;
  const schemeIdx = s.indexOf('://');
  if (schemeIdx >= 0) {
    const afterScheme = s.slice(schemeIdx + 3);
    const atIdx = afterScheme.indexOf('@');
    hostPath = atIdx >= 0 ? afterScheme.slice(atIdx + 1) : afterScheme;
  } else {
    const atIdx = s.indexOf('@');
    if (atIdx >= 0) {
      // scp-style:  user@host:path
      const afterUser = s.slice(atIdx + 1);
      const colIdx = afterUser.indexOf(':');
      hostPath = colIdx >= 0
        ? `${afterUser.slice(0, colIdx)}/${afterUser.slice(colIdx + 1)}`
        : afterUser;
    } else {
      hostPath = s;
    }
  }

  const slashIdx = hostPath.indexOf('/');
  if (slashIdx < 0) return null;
  const hostWithPort = hostPath.slice(0, slashIdx);
  let path           = hostPath.slice(slashIdx + 1);

  const host = hostWithPort.split(':')[0]?.trim().toLowerCase() ?? '';
  if (!host) return null;

  // Strip leading slashes, .git suffix, trailing slashes.
  path = path.replace(/^\/+/, '').replace(/\/+$/, '');
  if (path.endsWith('.git')) path = path.slice(0, -4);
  if (!path) return null;

  return `${host}/${path.toLowerCase()}`;
}

/**
 * Derive a default folder name from a git URL: the last segment of the
 * canonical path, falling back to "repository".  Used to pre-fill the
 * destination folder in the clone-confirm modal.
 */
export function defaultRepoNameFromUrl(url: string): string {
  const key = canonicalKey(url);
  if (!key) return 'repository';
  const tail = key.split('/').pop();
  return tail && tail.length > 0 ? tail : 'repository';
}
