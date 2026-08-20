/**
 * Human-readable numbers and moments.
 *
 * Here rather than in a product's store because "1.4 MB" and "3 h ago" are not about
 * recordings, or history, or anything else — they are about reading. They started in
 * Tyto's recorder store, which is where the second consumer found them.
 */

/** `1 536` → `1.5 KB`. Whole units under 100, one decimal above 1 KB. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KB', 'MB', 'GB'];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i += 1; }
  return `${v.toFixed(v >= 100 ? 0 : 1)} ${units[i]}`;
}

/** How long ago `ts` (unix ms) was, in the coarsest unit that still says something. */
export function formatAgo(ts: number): string {
  const sec = Math.floor((Date.now() - ts) / 1000);
  if (sec < 60) return 'just now';
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min} min ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr} h ago`;
  const day = Math.floor(hr / 24);
  return `${day} d ago`;
}

/** `14:32` — the moment within its day, which is all a row needs once the day is a
 *  heading above it. */
export function clockTime(ts: number): string {
  return new Date(ts).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
}

/** The heading a list of moments groups under: `Today`, `Yesterday`, or the date.
 *  Computed from calendar days rather than from elapsed hours — 25 hours ago can be
 *  yesterday or the day before, and only the calendar knows which. */
export function dayLabel(ts: number): string {
  const d = new Date(ts);
  const today = new Date();
  const midnight = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((midnight(today) - midnight(d)) / 86_400_000);
  if (days <= 0) return 'Today';
  if (days === 1) return 'Yesterday';
  return d.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: days > 300 ? 'numeric' : undefined });
}
