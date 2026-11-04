/**
 * Turning what someone typed into a path a vault will accept.
 *
 * Pure on purpose: the dialogs that create and rename notes validate on every
 * keystroke, and a rule that lives in a component cannot be tested without
 * mounting one. The backend is still the authority (it refuses to overwrite and to
 * leave the vault) — this exists so the refusal arrives *beside the field*, before
 * the round trip, rather than as a toast after it.
 */

/** Characters no mainstream filesystem accepts in a file name. Windows is the
 *  strictest, and a vault synced to a Windows machine has to open there. */
const ILLEGAL_NAME_CHARS = /[\\/:*?"<>|\u0000-\u001f]/g;

const NOTE_EXTENSION = /\.(md|markdown)$/i;

/** Today's date as `YYYY-MM-DD`, in the machine's own timezone — "today" is the
 *  day on the user's wall clock, not in UTC. */
export function todayStamp(now: Date = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

/** A title as a file stem: illegal characters dropped, runs of whitespace
 *  collapsed, leading dots removed (a dot-file is hidden, and a hidden note is a
 *  lost one). The title's own capitalisation and spaces are kept — Obsidian
 *  vaults name files after their titles, and `[[My note]]` has to find one. */
export function stemFromTitle(title: string): string {
  return title
    .replace(ILLEGAL_NAME_CHARS, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/^\.+/, '')
    .trim();
}

/** What the user typed as a path, normalised: forward slashes, no leading or
 *  doubled separators, surrounding whitespace gone. */
export function normalizeNotePath(input: string): string {
  return input
    .trim()
    .replace(/\\/g, '/')
    .replace(/\/{2,}/g, '/')
    .replace(/^\/+/, '');
}

/** The path with a markdown extension, adding `.md` when there is none. */
export function withNoteExtension(path: string): string {
  return NOTE_EXTENSION.test(path) ? path : `${path}.md`;
}

/** The folder part of a vault-relative path, with a trailing slash; `''` at the root. */
export function folderOf(path: string): string {
  const slash = path.lastIndexOf('/');
  return slash === -1 ? '' : path.slice(0, slash + 1);
}

/** `[start, end)` of the file stem inside a path — what a rename dialog selects,
 *  so typing replaces the name and leaves the folder and extension alone. */
export function stemRange(path: string): [number, number] {
  const start = path.lastIndexOf('/') + 1;
  const dot = path.search(NOTE_EXTENSION);
  return [start, dot === -1 ? path.length : dot];
}

/**
 * Why `input` cannot be a new note's path, or `null` when it can.
 *
 * `taken` is the set of paths already in the vault; it is compared
 * case-insensitively, because a vault that is cloned onto a case-insensitive
 * filesystem would otherwise hold two notes that cannot coexist there.
 * `except` is the note being renamed, which may of course keep its own name.
 */
export function notePathProblem(
  input: string,
  taken: ReadonlySet<string>,
  except?: string,
): string | null {
  const path = normalizeNotePath(input);
  const stem = path.replace(NOTE_EXTENSION, '');
  if (!stem || stem.endsWith('/')) return 'Give the note a name.';

  const segments = path.split('/');
  if (segments.some((s) => s === '..' || s === '.')) return 'A note cannot sit outside the vault.';
  if (segments.some((s) => s.startsWith('.'))) {
    return 'A name starting with a dot is hidden — and so is its note.';
  }
  if (segments.some((s) => /[:*?"<>|\u0000-\u001f]/.test(s))) {
    return 'A file name cannot contain : * ? " < > |';
  }

  const wanted = withNoteExtension(path).toLowerCase();
  const own = except?.toLowerCase();
  for (const existing of taken) {
    const e = existing.toLowerCase();
    if (e === wanted && e !== own) return 'A note with that name is already there.';
  }
  return null;
}

/** A template's body without its `{{cursor}}` marker — the editor mounted on a
 *  note does not consume it, and it would otherwise be saved into the file. */
export function stripCursorMarker(text: string): string {
  return text.replace(/\{\{\s*cursor\s*\}\}/g, '');
}

/** A note type's icon (`bug`, `file-text`) as the key the shared icon table is
 *  indexed by (`Bug`, `FileText`). An already-Pascal name passes through. */
export function iconKey(name: string): string {
  return name
    .split(/[-_\s]+/)
    .filter(Boolean)
    .map((part) => part[0].toUpperCase() + part.slice(1))
    .join('');
}
