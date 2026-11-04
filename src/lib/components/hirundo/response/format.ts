/** Words for numbers on the response bar. Pure. */

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  const s = ms / 1000;
  return s < 10 ? `${s.toFixed(2)} s` : s < 60 ? `${s.toFixed(1)} s` : `${Math.floor(s / 60)} min ${Math.round(s % 60)} s`;
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(n < 10 * 1024 ? 1 : 0)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

export type StatusTone = 'success' | 'info' | 'warning' | 'error';

/** 2xx success, 3xx info (a redirect is not a problem), 4xx the caller's fault, 5xx the server's. */
export function statusTone(status: number): StatusTone {
  return status >= 500 ? 'error' : status >= 400 ? 'warning' : status >= 300 ? 'info' : 'success';
}
