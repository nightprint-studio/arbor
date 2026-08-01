/**
 * Checkboxes across the vault: finding them, and ticking one safely.
 *
 * **Why this reads notes instead of asking the index.** `garrulus-vault` already
 * parses every note's tasks — `Note.tasks`, `Note.open_tasks()` — but nothing
 * exposes them across the vault: there is no `garrulus_tasks` handler and no
 * handler that enumerates notes. So the panel does what it can with what exists,
 * which is read the notes and look. That is expensive by construction, and it is
 * why the scan is something the user starts rather than something that happens:
 * the same reasoning that makes `rebuildIndex` a button and not a timer.
 *
 * The parsing mirrors `garrulus-parse`'s reader (`- [ ]`, `- [x]`, `- [X]`, after
 * any bullet or ordered marker) so that what this panel calls a task is what the
 * rest of the product calls a task.
 */

import { readNote, search, writeNote } from '$lib/ipc/garrulus';
import { mapWithLimit } from './facets';

/**
 * The query that means "every note".
 *
 * `garrulus_search` answers the empty string with an empty list deliberately, and
 * no handler enumerates a vault. A query made only of a `sort:` term constrains
 * nothing — sorting is not filtering — so the index hands back every note, in
 * title order. That is a property of the grammar rather than a trick
 * (`index/src/query.rs`): `sort:title` is a legitimate query, and this is what it
 * means.
 */
const EVERY_NOTE = 'sort:title';

/** How many notes a scan will read before it stops and says so. A personal vault
 *  is thousands of notes and each one is a round trip; past this the panel is
 *  costing more than it is telling. */
export const SCAN_CAP = 1500;

/** How many reads are in flight at once. */
const IN_FLIGHT = 8;

/** One checkbox, and enough to go back to it. */
export interface VaultTask {
  /** The note's id — a vault-relative path unless the note declares a `uid`. */
  note: string;
  /** The note's title, as the index resolved it. */
  title: string;
  /** Zero-based index of the line in the note's source. */
  line: number;
  /** The task text, without the bullet and the checkbox. */
  text: string;
  done: boolean;
}

/** What a scan found, and what it could not reach. */
export interface TaskScan {
  tasks: VaultTask[];
  /** Notes whose source was read. */
  read: number;
  /**
   * Notes the scan could not read.
   *
   * Almost always a note identified by a frontmatter `uid` rather than by its
   * path: the index answers with the id, and only a path can be read. Counted
   * rather than hidden, so a panel that is missing tasks says so.
   */
  skipped: number;
  /** The vault is larger than {@link SCAN_CAP} and the scan stopped early. */
  capped: boolean;
  /** The caller asked it to stop; `tasks` is what had been found by then. */
  cancelled: boolean;
}

/**
 * A markdown task line: any bullet or ordered marker, then a checkbox.
 *
 * The capture groups are the indent+marker, the box's contents, and the rest of
 * the line — so a rewrite can put back everything it did not mean to change.
 */
const TASK_LINE = /^(\s*(?:[-*+]|\d+[.)])\s+\[)([ xX])(\].*)$/;

/** The text of a task line, with the marker and the box removed. */
function taskText(line: string): string {
  const rest = TASK_LINE.exec(line)?.[3] ?? '';
  return rest.slice(1).trim();
}

/**
 * Every checkbox in one note's source.
 *
 * Line-based, and therefore blind to fences: a `- [ ]` written *inside* a code
 * block is listed here where the real parser would not list it. The consequence
 * is bounded — the row shows a line that genuinely says that, and ticking it
 * flips the box on that exact line and nothing else — and the alternative is a
 * second markdown parser in the frontend, which is the thing
 * `docs/garrulus-design.md` §3.4 exists to avoid. This goes away with the first
 * backend handler that answers with the vault's parsed tasks.
 */
export function parseTasks(source: string, note: string, title: string): VaultTask[] {
  const out: VaultTask[] = [];
  // Split on '\n' only: a CRLF file keeps its '\r' at the end of each line, so
  // rejoining with '\n' puts the file back byte for byte.
  const lines = source.split('\n');
  lines.forEach((line, i) => {
    const m = TASK_LINE.exec(line);
    if (!m) return;
    out.push({
      note,
      title,
      line: i,
      text: taskText(line),
      done: m[2] !== ' ',
    });
  });
  return out;
}

/**
 * Rewrite one task's checkbox in `source`.
 *
 * Returns `null` when the line is no longer the task it was — the note was edited
 * on this machine or arrived from the other one since the scan. Refusing is the
 * whole point: writing a checkbox onto whatever now occupies line 41 is how a
 * note quietly loses a sentence.
 */
export function applyTaskState(source: string, task: VaultTask, done: boolean): string | null {
  const lines = source.split('\n');
  const line = lines[task.line];
  if (line === undefined) return null;

  const m = TASK_LINE.exec(line);
  if (!m) return null;
  if (taskText(line) !== task.text) return null;

  lines[task.line] = `${m[1]}${done ? 'x' : ' '}${m[3]}`;
  return lines.join('\n');
}

/** What a scan is told, and how it reports back. */
export interface ScanOptions {
  /** Called after each note, so the panel can say how far along it is. */
  onProgress?: (done: number, total: number) => void;
  /** Polled between notes; `true` stops the scan and returns what it has. */
  cancelled?: () => boolean;
}

/**
 * Read the vault and collect every checkbox in it.
 *
 * Reads only: `garrulus_search` once for the note list, then one
 * `garrulus_read_note` per note. It writes nothing, and it is never called except
 * from a click.
 */
export async function scanVaultTasks(options: ScanOptions = {}): Promise<TaskScan> {
  const { onProgress, cancelled } = options;

  const hits = await search(EVERY_NOTE);
  const capped = hits.length > SCAN_CAP;
  const notes = capped ? hits.slice(0, SCAN_CAP) : hits;

  let done = 0;
  let skipped = 0;
  let stopped = false;

  const perNote = await mapWithLimit(notes, IN_FLIGHT, async (hit) => {
    if (stopped || cancelled?.()) {
      stopped = true;
      return [] as VaultTask[];
    }
    try {
      const note = await readNote(hit.id);
      return parseTasks(note.text, hit.id, hit.title || hit.id);
    } catch {
      skipped++;
      return [] as VaultTask[];
    } finally {
      done++;
      onProgress?.(done, notes.length);
    }
  });

  return {
    tasks: perNote.flat(),
    read: done - skipped,
    skipped,
    capped,
    cancelled: stopped,
  };
}

/**
 * Tick or untick one task, in the note it lives in.
 *
 * Re-reads the note first and refuses when the line has moved, so a stale scan
 * can never write over an edit made since. This is a write, and it happens
 * because a checkbox was clicked — which is the only reason anything in Garrulus
 * writes (`docs/garrulus-design.md` §4.2).
 */
export async function setTaskState(task: VaultTask, done: boolean): Promise<boolean> {
  const note = await readNote(task.note);
  const next = applyTaskState(note.text, task, done);
  if (next === null) return false;
  await writeNote(task.note, next);
  return true;
}
