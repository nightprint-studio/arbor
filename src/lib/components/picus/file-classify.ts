/**
 * Saying what **one file** is — the sibling of `folder-classify.ts`, one level
 * down.
 *
 * The engine belongs to the folder, and in a tidy repository it stays there:
 * essentially every file inherits, and nothing here ever runs. This exists for
 * the untidy ones, where a single directory holds `4_12_ORA.sql` beside
 * `4_12_POS.sql` and can say nothing true about either — a folder declaration
 * would be a lie about half its contents, and a name rule is the wrong shape for
 * a one-off.
 *
 * The answers are `engine-choices.ts`', unchanged: a file is classified with the
 * same four engines a folder is, plus "inherit from the folder", which clears the
 * declaration. There is deliberately **no role here** — a role is what a
 * directory of scripts is *for*, and the file beside another in the same
 * directory is for the same thing.
 *
 * ## What the tree row shows, and what it deliberately does not
 *
 * {@link fileRowShowsEngine} is the judgement, in one place, because it is the
 * kind of rule that otherwise gets re-decided per call site until half the rows
 * disagree. A chip on all five hundred file rows is noise that repeats the folder
 * header five hundred times; no chip at all hides the classification that decides
 * how the file is parsed. So a file row carries a chip exactly when it says
 * something its folder does not.
 *
 * ## And the offer, which is where file names get dangerous
 *
 * Classifying `4_12_POS.sql` says something about one file, and there are usually
 * ten more. The same alias machinery answers for it — with `applies_to` opened up
 * to file names — but the offer is held to a higher bar here than for folders,
 * because a file name is a sentence: `ORA` is Italian for *now*, `POS` sits
 * inside `POSIZIONI`, and a repository has hundreds of file names to a dozen
 * folder names. Hence {@link fileAliasCandidates}: the rule can only ever be
 * about **one word of the name the user is looking at**, chosen by them.
 */

import { Database, FileCog } from 'lucide-svelte';
import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { picusProjectStore } from '$lib/stores/picus/project.svelte';
import { picusUiStore } from '$lib/stores/picus/ui.svelte';
import { CLEAR_ID, engineMenuItems } from './engine-choices';
import {
  aliasScope,
  declaredFileEngine,
  engineLabel,
  fileDeclaresEngine,
  fileEngine,
  fileEngineIsUnknown,
  scopeCoversFiles,
  type FolderEngine,
  type FolderNode,
  type ScriptFile,
} from '$lib/types/picus';

/** What "inherit" means for a file — its folder, not "above". */
const INHERIT_LABEL = 'Inherit from the folder';

// ── What a file row shows ────────────────────────────────────────────────────

/**
 * Does this file's row carry an engine chip?
 *
 * Only when the row says something the folder header does not, which is two
 * cases and no others:
 *
 *  • **it declares its own engine** — the whole reason file-level classification
 *    exists, and the one row in the folder whose engine is not the folder's;
 *  • **it has no engine while a sibling does** — the odd one out in a directory
 *    somebody has started sorting by file name. Without the chip it looks
 *    identical to the classified files around it, and it is the only one nothing
 *    is generated into.
 *
 * Everything else inherits silently. A chip on every row of a five-hundred-file
 * repository is the folder's own chip, repeated five hundred times, and a badge
 * that is always present is a badge nobody reads.
 */
export function fileRowShowsEngine(file: ScriptFile, folder: FolderNode): boolean {
  if (fileDeclaresEngine(file)) return true;
  if (!fileEngineIsUnknown(file)) return false;
  return folder.files.some((sibling) => !fileEngineIsUnknown(sibling));
}

// ── The row menu ─────────────────────────────────────────────────────────────

/**
 * What a file row offers: the engine, and the dialog.
 *
 * The folder's own entry stays available beside this in the tree's menu — most
 * of the time the right correction really is the folder's, and burying it would
 * push people toward per-file declarations they do not need.
 */
export function fileClassifyItems(file: ScriptFile, folder: FolderNode): MenuItem[] {
  return [
    {
      id: 'engine',
      label: 'Engine of this file',
      icon: Database,
      children: engineMenuItems({
        declared: declaredFileEngine(file),
        effective: fileEngine(file),
        from: folder.path,
        inheritLabel: INHERIT_LABEL,
      }),
    },
    { id: 'file-classify-sep', label: '', separator: true },
    { id: 'classify-file', label: `Classify ${file.name}…`, icon: FileCog },
  ];
}

/** Turn a menu id into a write. Returns `true` when the id was one of ours. */
export async function runFileClassifyId(file: ScriptFile, id: string): Promise<boolean> {
  const [kind, value] = id.split(':');
  if (kind !== 'dialect') return false;
  await classifyFile(file, value === CLEAR_ID ? null : (value as FolderEngine));
  return true;
}

// ── The write ────────────────────────────────────────────────────────────────

/**
 * Declare (or clear) one file's engine, and say so.
 *
 * Returns `true` on success. The toast quotes the file the backend says it wrote
 * for the same reason every other write in Picus does: this puts something into
 * the user's own repository, and a tool that does that says where.
 */
export async function classifyFile(
  file: ScriptFile,
  engine: FolderEngine | null,
): Promise<boolean> {
  const message = await picusProjectStore.classifyFile(file.path, engine);
  if (message) {
    toastStore.show(`${file.path} could not be classified — ${message}`, 'error');
    return false;
  }
  // Classifying from a dialog or the palette leaves the tree wherever it was;
  // opening the folder makes the new chip checkable rather than a claim.
  picusProjectStore.revealFile(file.path);
  const where = picusProjectStore.configPath;
  const said = engine ? engineLabel(engine) : "its folder's engine";
  toastStore.show(`${file.name} → ${said}${where ? `. Saved in ${where}` : ''}`, 'success');
  void offerFileAliasFor(file, engine);
  return true;
}

// ── "…and every file with POS in its name" ───────────────────────────────────

/**
 * The words of a name, exactly as `picus-project` splits them.
 *
 * Alphanumeric runs, lowercased, with a break at every letter/digit boundary:
 * `4_12_ORA` → `["4", "12", "ora"]`. Mirrored here **only** to decide which
 * words are worth *offering* — never to decide anything that is shown as true.
 * Every count the user sees comes from the backend, by the rule the alias itself
 * will use; if this copy ever drifts, the worst it can do is suggest a word the
 * offer then reports as reaching nothing, which is visible and self-correcting.
 *
 * Candidates stay a **single word** so the question reduces to "is this word one
 * of the name's words", which is the whole of the matching rule for a one-word
 * alias.
 */
function nameWords(name: string): string[] {
  const out: string[] = [];
  let current = '';
  let digits = false;
  for (const c of name) {
    if (!/[\p{L}\p{N}]/u.test(c)) {
      if (current) { out.push(current); current = ''; }
      continue;
    }
    const isDigit = /\p{N}/u.test(c);
    if (current && isDigit !== digits) { out.push(current); current = ''; }
    digits = isDigit;
    current += c;
  }
  if (current) out.push(current);
  return out;
}

/** `4_12_POS.sql` → `4_12_POS`. A dotfile is all extension and no name. */
function stemOf(fileName: string): string {
  const cut = fileName.lastIndexOf('.');
  return cut > 0 ? fileName.slice(0, cut) : fileName;
}

/**
 * The words of this file's name that could carry a meaning, in the file's own
 * spelling.
 *
 * Pure numbers are dropped — `4` and `12` are the version, not the engine — and
 * the extension never takes part, so `.sql` can never become an alias called
 * `SQL`. Original case is kept because it is what the user would type, though
 * matching ignores it.
 */
export function fileAliasCandidates(fileName: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const word of nameWords(stemOf(fileName))) {
    if (!/[\p{L}]/u.test(word)) continue;
    const key = word.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(word);
  }
  return out;
}

/**
 * Roughly how many files a word would reach — for **ranking candidates only**.
 *
 * Deliberately not the number the offer shows. That one comes from the backend
 * (`picusProjectStore.filesNamed`), by the same rule the alias will use, because
 * it is the number the user's decision rests on. This one ranks several words at
 * once while the menu is being built, where a round trip per candidate would be
 * absurd and where being one off changes nothing: the word it picks is shown,
 * and the user can pick another.
 *
 * So the mirrored word-splitting below decides *what to suggest* and never *what
 * is true*. Kept to one word for the same reason — see {@link nameWords}.
 */
function estimatedReach(word: string): string[] {
  const needle = word.trim().toLowerCase();
  if (!needle) return [];
  return picusProjectStore.allFiles
    .filter((f) => nameWords(stemOf(f.name)).some((w) => w.toLowerCase() === needle))
    .map((f) => f.path);
}

/**
 * Which word of the name to propose, if any.
 *
 * Ranked, best first:
 *  1. a word this repository **already declares** — extending `POS` to file
 *     names is a smaller, better-understood change than inventing a rule;
 *  2. a short word — engine markers are `ORA`, `POS`, `PG`, `MSQ`, `DB2`, and a
 *     long word is far more likely to be part of what the script *does*;
 *  3. the word that recurs across the most files — a marker repeats, a subject
 *     does not.
 *
 * Words that appear in only one file are not proposed at all: one file is not a
 * rule, it is the thing that was just done.
 */
export function suggestedAliasWord(fileName: string): string {
  let best = '';
  let bestScore = -1;
  for (const word of fileAliasCandidates(fileName)) {
    const reach = estimatedReach(word).length;
    if (reach < 2) continue;
    const score =
      (picusProjectStore.aliasFor(word) ? 400 : 0)
      + (word.length <= 5 ? 200 : 0)
      + Math.min(reach, 199);
    if (score > bestScore) { bestScore = score; best = word; }
  }
  return best;
}

/**
 * Ask whether what was just said about one file should hold for a **word of its
 * name**.
 *
 * Fire-and-forget and after the fact, exactly like the folder offer: the file is
 * already classified, so declining costs the user nothing they just did.
 *
 * Not raised when:
 *  • the change cleared the declaration — "inherit" is not a meaning a name can
 *    carry;
 *  • no word of the name recurs anywhere else;
 *  • the project already says exactly this about the word **and already looks
 *    for it in file names**;
 *  • the user declined this name earlier in the session.
 */
export async function offerFileAliasFor(
  file: ScriptFile,
  engine: FolderEngine | null,
): Promise<void> {
  if (!engine) return;

  const name = suggestedAliasWord(file.name);
  if (!name) return;

  const existing = picusProjectStore.aliasFor(name);
  if (
    existing
    && (existing.engine ?? null) === engine
    && (existing.role ?? null) === null
    && scopeCoversFiles(aliasScope(existing))
  ) return;

  // Both asked of the backend, by the rule the alias itself will use: the files
  // because the offer's whole safety property is that the number beside it is
  // true, and the folders so the offer can say whether folders of that name
  // exist too — which is the basis for choosing "folders and files" over "files".
  const [filePaths, folderPaths] = await Promise.all([
    picusProjectStore.filesNamed(name),
    picusProjectStore.foldersNamed(name),
  ]);
  // One file is not a rule, it is the thing that was just done.
  if (filePaths.length < 2) return;

  picusUiStore.offerAlias({
    kind: 'file',
    name,
    engine,
    role: null,
    folderPaths,
    filePaths,
    origin: file.path,
    /** Every other word of the name, so the user can correct the guess. */
    alternatives: fileAliasCandidates(file.name).filter((w) => w !== name),
  });
}
