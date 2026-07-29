/**
 * Picus DML generator — three input sources, one intermediate model, N per-target
 * emissions.
 *
 * The shape of the whole feature: a form / a pasted set of INSERTs / a CSV all
 * collapse into `{ table, operation, keyColumns, rows }`, and from there the
 * source no longer matters. Each **target** is one file, with its own dialect and
 * its own rules, so the same logical change becomes a bare INSERT in the Oracle
 * init script and a guarded PL/SQL block in the update one.
 *
 * Rule coherence is enforced HERE, not in the markup, so every entry point
 * (form, palette, preset) gets the same behaviour:
 *   • a version guard requires the procedural block — switching it on switches
 *     the block on too;
 *   • switching the block off drops the version guard;
 *   • "copy these rules" only ever propagates between targets of the SAME role,
 *     never from initialisation to update.
 *
 * ## Emission and validation are the backend's
 *
 * Both `picus_emit` and `picus_validate_rows` live in `picus-emit`, which owns the
 * golden tests. That makes them **asynchronous**, and the preview deliberately has
 * no refresh button — so this store keeps a cache of the last emitted SQL per
 * target, refreshed by a debounced effect that watches the model and the enabled
 * targets. `sqlFor()` is then a **pure read** of that cache: materialising it
 * lazily inside a `$derived` would be a write during derivation
 * (`state_unsafe_mutation`), which is the same trap `queryStore` already paid for.
 *
 * A round trip is a few milliseconds, well under the threshold that earns a
 * spinner; what the UI does instead is dim a preview it knows to be stale.
 *
 * ## Writing is two calls, and the second proves it is writing what was reviewed
 *
 * `preview()` asks the backend for **the exact bytes that would land** in each
 * destination, each with a digest of the file as it was when they were computed.
 * `apply()` hands those digests back, and the backend refuses — naming the file —
 * if anything moved on disk in between. That refusal is the useful part, so it is
 * kept verbatim in `applyError` rather than collapsed into "the write failed".
 *
 * Emission and preview are different questions and neither replaces the other:
 * emission is "what SQL does this destination want", preview is "what does the
 * file look like once that SQL is spliced in".
 */

import {
  GENERIC_ENGINE,
  predicateIsEmpty,
  type Column,
  type DmlOperation,
  type DmlRow,
  type DmlSource,
  type FolderRole,
  type Predicate,
  type Target,
  type TargetGuards,
} from '$lib/types/picus';
import {
  emit as rpcEmit,
  validateRows as rpcValidateRows,
  type DmlModel,
  type ValueProblem,
} from '$lib/ipc/picus/db';
import {
  applyScripts,
  previewApply,
  scriptColumns,
  type ApplyResult,
  type PreviewFile,
} from '$lib/ipc/picus/scripts';
import type { DestinationSetInput, ResolvedDestination } from '$lib/ipc/picus/project';
import { parseCsv, proposeCsvMapping } from '$lib/utils/picus/csv';
import { parsePastedInserts } from '$lib/utils/picus/paste-sql';
import { picusProjectStore } from './project.svelte';
import { schemaStore } from './schema.svelte';
import { picusSettingsStore } from './settings.svelte';

/** A destination of a set that could not be armed, and the folder it named. */
export interface RefusedDestination {
  folder: string;
  reason: string;
}

/**
 * Coalescing window before a model change reaches the backend.
 *
 * Long enough that typing a value is one round trip rather than one per
 * keystroke, short enough that the preview reads as live — you look up from the
 * form and it is already right.
 */
const REFRESH_DEBOUNCE_MS = 200;

/** Per-role defaults (§4.4). A starting point — every target stays editable. */
export function presetForRole(role: FolderRole): { wrap: Target['wrap']; guards: TargetGuards } {
  if (role === 'update') {
    return {
      wrap: 'block',
      guards: { version: { from: '', to: '' }, skipIfPresent: true, requireObject: false, transactional: false },
    };
  }
  return {
    wrap: 'plain',
    guards: { version: null, skipIfPresent: false, requireObject: false, transactional: false },
  };
}

function createDmlStore() {
  let source = $state<DmlSource>('form');
  let table = $state('');
  let operation = $state<DmlOperation>('upsert');

  /**
   * The WHERE of an update or a delete.
   *
   * Held whatever the operation is, and only *sent* for the two that have one:
   * switching to `insert` to check something and back must not throw away a
   * clause somebody built condition by condition.
   */
  let whereClause = $state<Predicate>({ kind: 'group', join: 'and', of: [] });
  const usesWhere = $derived(operation === 'update' || operation === 'delete');

  /**
   * Form mode: the rows being composed, one value per column each.
   *
   * A list rather than a single record because "add three parameters" is the
   * ordinary case, not an advanced one, and doing it as three generations means
   * three blocks, three markers and three diffs for one logical change. The grid
   * shows **one at a time** — see `DmlValueGrid` for why that beats widening the
   * table into a spreadsheet.
   */
  let formRows = $state<Record<string, string>[]>([{}]);
  /** Which of them the grid is editing. Clamped on read, never on change. */
  let formCursor = $state(0);
  /** Explicit comparison-key selection; empty falls back to the primary key. */
  let keySelection = $state<Record<string, boolean>>({});

  let pasteText = $state('');
  let pasteErrors = $state<string[]>([]);
  let csvText = $state('');
  /** CSV header → table column. Explicit, with a name-match proposal. */
  let csvMapping = $state<Record<string, string>>({});

  /** Rows read from the paste/CSV sources. Form mode derives its single row. */
  let importedRows = $state<DmlRow[]>([]);
  /**
   * Columns the pasted statements named, when no live schema knows the table.
   *
   * The generator used to be unusable without a database connection: the column
   * set came only from `schemaStore`, so with nothing connected there were no
   * fields, no types and nothing to emit — in a product whose subject is scripts
   * on disk. A pasted `INSERT` already carries its columns, so it supplies them.
   * See `utils/picus/paste-sql.ts` for why the inferred types are faithful rather
   * than a guess about the schema.
   */
  let importedColumns = $state<Column[]>([]);
  /**
   * Columns read out of the repository's own statements, for a table no
   * connected database knows.
   *
   * The gap this closes: picking a table that only the scripts install left the
   * form with **no fields at all** — the table was selectable and then there was
   * nothing to type into, which is worse than not offering it. See
   * `ipc/picus/scripts.ts::scriptColumns` for where they come from.
   */
  let scriptSideColumns = $state<Column[]>([]);
  /** `table` the columns above were read for; anything else means they are stale. */
  let scriptColumnsFor = $state('');

  /** Destinations. Empty until the user adds one from the open repository. */
  let targets = $state<Target[]>([]);
  let expandedTargetId = $state<string | null>(null);
  let previewTargetId = $state<string>('');

  let generated = $state(false);
  let applied = $state(false);

  /**
   * The columns this generation works from.
   *
   * The live table's when a database is connected — its types carry the length
   * limits and NOT NULL flags validation reports on, and it knows the primary key.
   * Otherwise the ones the source itself named. Only real tables are looked up:
   * DML is written against something writable, so a view never shows up here even
   * though it has columns.
   */
  const columns = $derived<Column[]>(
    schemaStore.table(table)?.columns
      ?? (importedColumns.length ? importedColumns : scriptColumnsHere()),
  );

  /** The script-side columns, but only for the table they were read for. */
  function scriptColumnsHere(): Column[] {
    return scriptColumnsFor === table.toUpperCase() ? scriptSideColumns : [];
  }

  /**
   * The source dictates the table, so it is shown rather than asked for.
   *
   * A pasted `INSERT INTO PARAMETRI (…)` has already said which table it is
   * about. Asking again — through a dropdown that is empty unless a database is
   * connected — was asking the user to repeat themselves, and to do it through a
   * control that could not answer.
   */
  const tableIsFromSource = $derived(source === 'paste' && importedColumns.length > 0);

  const keyColumns = $derived.by<Column[]>(() => {
    const picked = columns.filter((c) => keySelection[c.name]);
    return picked.length ? picked : columns.filter((c) => c.primaryKey);
  });

  /** The form row being edited. Clamped here so a delete cannot leave it dangling. */
  const activeFormRow = $derived(Math.min(formCursor, Math.max(0, formRows.length - 1)));

  /** A form row nobody has typed anything into — not worth writing, not an error. */
  function isBlank(row: Record<string, string>): boolean {
    return !Object.values(row).some((v) => v.trim());
  }

  /**
   * Which form rows carry values, by index.
   *
   * A blank row is what an "add another" button leaves behind when the user
   * changes their mind, and emitting it would produce a statement of nothing but
   * NULLs. It is skipped rather than reported: nobody meant it.
   *
   * Kept as indices, not as rows, because everything the backend says about a row
   * comes back keyed by its position in `rows` — and that has to be translatable
   * back to the form row the user is looking at.
   */
  const filledFormRows = $derived(
    formRows.map((row, i) => (isBlank(row) ? -1 : i)).filter((i) => i >= 0),
  );

  const rows = $derived<DmlRow[]>(
    source === 'form'
      ? filledFormRows.map((i) =>
          columns.reduce<DmlRow>((acc, c) => { acc[c.name] = formRows[i][c.name] ?? ''; return acc; }, {}))
      : importedRows,
  );

  /** Where a form row sits in the model, or -1 when it is blank and not sent. */
  function modelIndexOf(formRow: number): number {
    return filledFormRows.indexOf(formRow);
  }

  const enabledTargets = $derived(targets.filter((t) => t.enabled));
  const csvHeaders = $derived(parseCsv(csvText).headers);

  const model = $derived<DmlModel>({
    table,
    operation,
    columns,
    keyColumns,
    rows,
    // Sent only when it describes something, and only for the two operations that
    // have a WHERE at all. An empty tree is not "match nothing" — it is "no
    // predicate", which is what the backend reads `undefined` as.
    whereClause: usesWhere && !predicateIsEmpty(whereClause) ? whereClause : undefined,
    lowercasePostgres: picusSettingsStore.lowercasePostgres,
    // Where the installed version is recorded — a per-project fact, not a
    // constant: the table, the column and whether a date is stamped at all all
    // differ between projects.
    versionTable: picusSettingsStore.versionTable,
  });

  // ── Backend-backed emission and validation ─────────────────────────────────

  /** Last emitted SQL, keyed by target id. Read by `sqlFor`, written only here. */
  let emittedSql = $state<Record<string, string>>({});
  /** Per target, why its rule set cannot all apply. Empty when everything holds. */
  let ruleConflicts = $state<Record<string, string>>({});
  /** The backend refused to emit at all — shown instead of a stale preview. */
  let emitError = $state<string | null>(null);
  /** Model+targets state the cache corresponds to; anything else is stale. */
  let emittedKey = $state('');

  let problems = $state<ValueProblem[]>([]);
  /** Model state `problems` corresponds to. Never assume an unchecked model is fine. */
  let validatedKey = $state('');
  /** A `picus_validate_rows` call is out. Distinct from "no verdict yet", which also
   *  covers the debounce window before one is even sent. */
  let validateInFlight = $state(false);

  /**
   * The key doubles as the dependency and as the change detector.
   *
   * `JSON.stringify` walks every field of the model and of each enabled target,
   * which is precisely the deep read the effect needs: `enabledTargets` alone
   * tracks `enabled` and the array identity, so toggling a *guard* would not
   * re-run an effect that merely read the list. It is also what makes a change
   * that does not alter the payload — expanding a row, switching the previewed
   * destination — cost nothing.
   */
  const modelKey = $derived(JSON.stringify(model));
  const targetsKey = $derived(JSON.stringify(enabledTargets));
  const emitKey = $derived(`${modelKey}\n${targetsKey}`);

  /** The preview is showing SQL for a state that is no longer the current one. */
  const previewStale = $derived(emittedKey !== emitKey);
  /** The current model has actually been checked — not merely never rejected. */
  const validated = $derived(validatedKey === modelKey);

  // Only the newest round trip may write the cache: a slow emission overtaken by
  // a faster one must not resurrect the SQL of a model the user has left behind.
  let emitSeq = 0;
  let validateSeq = 0;

  async function runEmit(key: string) {
    const seq = ++emitSeq;
    // Snapshots, not proxies: what crosses the IPC has to be plain data, and
    // taking it here pins it to the key this round trip is answering for.
    const payloadModel = $state.snapshot(model) as DmlModel;
    const payloadTargets = $state.snapshot(enabledTargets) as Target[];
    try {
      const results = await rpcEmit(payloadModel, payloadTargets);
      if (seq !== emitSeq) return;
      const sql: Record<string, string> = {};
      const conflicts: Record<string, string> = {};
      for (const r of results) {
        sql[r.targetId] = r.sql;
        if (r.ruleConflict) conflicts[r.targetId] = r.ruleConflict;
      }
      emittedSql = sql;
      ruleConflicts = conflicts;
      emitError = null;
    } catch (e) {
      if (seq !== emitSeq) return;
      emitError = String(e);
    }
    // Settled either way: the key advances so the preview stops reading as
    // "still working" when the backend has already answered, badly or not.
    emittedKey = key;
  }

  async function runValidate(key: string) {
    const seq = ++validateSeq;
    validateInFlight = true;
    const payloadModel = $state.snapshot(model) as DmlModel;
    try {
      const found = await rpcValidateRows(payloadModel);
      if (seq !== validateSeq) return;
      problems = found;
    } catch {
      if (seq !== validateSeq) return;
      // Fail open. A backend that is down or still coming up must not leave the
      // Generate button permanently dead — the emission alongside it fails
      // visibly, which is where an outage belongs.
      problems = [];
    }
    validatedKey = key;
    validateInFlight = false;
  }

  /**
   * The store is a window-lifetime singleton, so it owns its own effect root
   * rather than borrowing a component's: every consumer of the cache
   * (`SqlPreview`, the patch cards, the write action) would otherwise depend on
   * which of them happened to mount first.
   *
   * Each effect returns a cleanup that cancels its pending timer, which is the
   * whole of the debounce: any change to the payload discards the round trip that
   * was about to describe the previous one.
   */
  $effect.root(() => {
    $effect(() => {
      const key = emitKey;
      if (!enabledTargets.length) {
        // Nothing to emit is a normal state, not a request: answer it here rather
        // than paying a round trip to be told the same.
        emittedSql = {};
        ruleConflicts = {};
        emitError = null;
        emittedKey = key;
        return;
      }
      const timer = setTimeout(() => void runEmit(key), REFRESH_DEBOUNCE_MS);
      return () => clearTimeout(timer);
    });

    $effect(() => {
      const key = modelKey;
      if (!columns.length || !rows.length) {
        problems = [];
        validatedKey = key;
        return;
      }
      const timer = setTimeout(() => void runValidate(key), REFRESH_DEBOUNCE_MS);
      return () => clearTimeout(timer);
    });

    // A table only the scripts know: ask the repository what its statements write
    // into it. Not debounced — the table changes when somebody picks one, not as
    // they type — and skipped entirely the moment a live schema can answer, which
    // it should where it can: real types carry the length limits and the key.
    $effect(() => {
      const wanted = table.trim().toUpperCase();
      const root = picusProjectStore.root;
      if (!wanted || !root || schemaStore.table(table) || importedColumns.length) return;
      if (scriptColumnsFor === wanted) return;
      let live = true;
      void scriptColumns(root, table)
        .then((found) => {
          if (!live) return;
          scriptSideColumns = found;
          scriptColumnsFor = wanted;
        })
        .catch(() => {
          if (!live) return;
          // Settled either way: the key advances so a backend that is not up does
          // not leave this asking again on every keystroke.
          scriptSideColumns = [];
          scriptColumnsFor = wanted;
        });
      return () => { live = false; };
    });
  });

  // ── The write: preview, then apply what was previewed ──────────────────────

  /** The exact bytes each destination would receive. Empty until a preview lands. */
  let previewFiles = $state<PreviewFile[]>([]);
  /** Payload+root the preview corresponds to. Anything else means "ask again". */
  let previewedKey = $state('');
  let previewing = $state(false);
  let previewError = $state<string | null>(null);
  let applying = $state(false);
  /**
   * Why the last write was refused, **verbatim**.
   *
   * The backend's refusal names the file that changed since the preview, which is
   * the only thing that tells the user what to do next. Rewording it into
   * "the write failed" would throw away the message and keep the failure.
   */
  let applyError = $state<string | null>(null);

  /** The preview depends on the repository as well as on the payload. */
  const previewKey = $derived(`${picusProjectStore.root}\n${emitKey}`);
  const previewFresh = $derived(previewedKey === previewKey);

  async function runPreview(key: string): Promise<void> {
    previewing = true;
    previewError = null;
    const root = picusProjectStore.root;
    const payloadModel = $state.snapshot(model) as DmlModel;
    // `enabledTargets`, never `targets`: the write handlers prepare every target
    // they are given and do not consult `enabled` themselves, so sending the whole
    // list would write into destinations the user deliberately unchecked.
    const payloadTargets = $state.snapshot(enabledTargets) as Target[];
    try {
      const res = await previewApply(root, payloadModel, payloadTargets);
      previewFiles = res.files ?? [];
    } catch (e) {
      previewFiles = [];
      previewError = String(e);
    }
    // Settled either way — the key advances so a failure does not loop.
    previewedKey = key;
    previewing = false;
  }

  /**
   * Per-column messages for the form. Same shape the grid always read — one entry
   * per offending column — sourced now from the backend's verdict on row 0, which
   * in form mode is the only row there is.
   */
  const validation = $derived.by(() => {
    const out: Record<string, string> = {};
    if (source !== 'form') return out;
    const at = modelIndexOf(activeFormRow);
    if (at < 0) return out;
    for (const p of problems) {
      if (p.row === at) out[p.column] = p.reason;
    }
    return out;
  });

  /**
   * Rows that fail their column types, per row index.
   *
   * Filled for **every** source, form included. With one form row it was
   * redundant with `validation`; with several it is the only thing that says a
   * row you are not currently looking at is the one holding the bad value.
   */
  const rowIssues = $derived.by(() => {
    const out = new Map<number, string[]>();
    for (const p of problems) {
      // Keyed by the index the CALLER thinks in: the form's own row numbering
      // where the form is the source, the imported list's otherwise. The blank
      // rows the model skips are what makes those two differ.
      const key = source === 'form' ? (filledFormRows[p.row] ?? p.row) : p.row;
      const list = out.get(key);
      if (list) list.push(`${p.column}: ${p.reason}`);
      else out.set(key, [`${p.column}: ${p.reason}`]);
    }
    return out;
  });

  /** Coherence: a version guard cannot exist without the procedural block. */
  function normalise(t: Target) {
    if (t.wrap === 'plain') t.guards.version = null;
  }

  return {
    get source() { return source; },
    get table() { return table; },
    get operation() { return operation; },
    get whereClause() { return whereClause; },
    /** Does this operation have a WHERE at all? */
    get usesWhere() { return usesWhere; },
    /** True when the clause would replace the comparison key. */
    get hasWhere() { return usesWhere && !predicateIsEmpty(whereClause); },

    setWhereClause(next: Predicate) {
      whereClause = next;
      // The statement changes, so anything computed from it is about an earlier
      // one — the same rule every other edit here follows.
      generated = false;
      applied = false;
    },
    /** The form row currently being edited — what the value grid binds to. */
    get values() { return formRows[activeFormRow] ?? {}; },
    /** Every form row, for the strip that lets you walk them. */
    get formRows() { return formRows; },
    get formCursor() { return activeFormRow; },
    get keySelection() { return keySelection; },
    get columns() { return columns; },
    get keyColumns() { return keyColumns; },
    /** The source named the table, so it is shown rather than asked for. */
    get tableIsFromSource() { return tableIsFromSource; },
    /**
     * No live database knows this table, so the columns and their types were read
     * from SQL — the pasted statements, or the repository's own.
     *
     * Worth saying out loud in the interface: everything downstream behaves
     * slightly differently, and the user should know before they read the
     * generated block rather than after.
     */
    get columnsFromSource() { return !schemaStore.table(table) && columns.length > 0; },
    /** …and specifically from the repository, rather than from a paste. */
    get columnsFromScripts() {
      return !schemaStore.table(table) && !importedColumns.length && scriptColumnsHere().length > 0;
    },
    get rows() { return rows; },
    get model() { return model; },
    get validation() { return validation; },
    get rowIssues() { return rowIssues; },
    /** No verdict exists for the current model yet — a check is pending or in flight. */
    get validating() { return validateInFlight || !validated; },
    get pasteText() { return pasteText; },
    get pasteErrors() { return pasteErrors; },
    get csvText() { return csvText; },
    get csvMapping() { return csvMapping; },
    get importedRows() { return importedRows; },
    get targets() { return targets; },
    get enabledTargets() { return enabledTargets; },
    get expandedTargetId() { return expandedTargetId; },
    get previewTargetId() { return previewTargetId; },
    get generated() { return generated; },
    get applied() { return applied; },
    /** The cached SQL predates the current model — dim it, don't hide it. */
    get previewStale() { return previewStale; },
    /** Why the backend could not emit; `null` when the last attempt succeeded. */
    get emitError() { return emitError; },

    // ── The write ────────────────────────────────────────────────────────────

    /** The exact bytes each destination would receive, as the last preview saw them. */
    get previewFiles() { return previewFiles; },
    /**
     * The preview describes the current payload against the current repository.
     *
     * False means what is on screen predates the last edit — and `apply` refuses
     * on it, because a digest can only prove the file did not move, never that the
     * generation did not.
     */
    get previewFresh() { return previewFresh; },
    get previewing() { return previewing; },
    /** Why the preview could not be computed; `null` when it could. */
    get previewError() { return previewError; },
    get applying() { return applying; },
    /** The backend's refusal, word for word. `null` when the last write went through. */
    get applyError() { return applyError; },
    /** Dependency handle for a consumer that wants to re-preview when the payload moves. */
    get previewKey() { return previewKey; },
    /** Files the write would actually change — a no-op destination is not a change. */
    get changedFiles() { return previewFiles.filter((f) => f.before !== f.after); },

    /**
     * Make sure a preview exists for the current payload.
     *
     * Idempotent and self-guarding: safe to call from an `$effect` that merely
     * watches `previewKey`, from the write action, and from the dock's refresh
     * button, without any of the three knowing about the others.
     */
    async ensurePreview(): Promise<void> {
      const key = previewKey;
      if (previewing || previewedKey === key) return;
      await runPreview(key);
    },

    /** Throw the preview away and ask again — after a refused write, or on demand. */
    async rebuildPreview(): Promise<void> {
      if (previewing) return;
      applyError = null;
      await runPreview(previewKey);
    },

    /**
     * Write the previewed bytes, proving they are the ones that were reviewed.
     *
     * Returns the counts on success and `null` on refusal, leaving the reason in
     * `applyError`. Either way the preview is dropped: after a write it no longer
     * describes disk, and after a refusal it is exactly what was wrong.
     */
    async apply(): Promise<ApplyResult | null> {
      if (applying || !previewFiles.length) return null;
      // The digests prove the FILES did not move; nothing on the backend can prove
      // the MODEL did not. If the payload changed since the preview was computed,
      // the backend would re-plan from the new one and write bytes nobody reviewed
      // — the exact substitution this whole two-step exists to prevent. So the
      // staleness is refused here, in the same words the other refusal uses.
      if (previewedKey !== previewKey) {
        applyError =
          'What would be written changed after the diff was computed — nothing was written. '
          + 'Review the patch again before writing.';
        previewFiles = [];
        previewedKey = '';
        return null;
      }
      applying = true;
      applyError = null;
      const root = picusProjectStore.root;
      const payloadModel = $state.snapshot(model) as DmlModel;
      // Armed destinations only — see `runPreview`. The same list the preview was
      // computed from, so the digests handed back still describe it.
      const payloadTargets = $state.snapshot(enabledTargets) as Target[];
      const digests: Record<string, string> = {};
      for (const f of previewFiles) digests[f.path] = f.digest;
      try {
        const res = await applyScripts(root, payloadModel, payloadTargets, digests);
        applied = true;
        previewFiles = [];
        previewedKey = '';
        return res;
      } catch (e) {
        applyError = String(e);
        previewFiles = [];
        previewedKey = '';
        return null;
      } finally {
        applying = false;
      }
    },

    /**
     * Why Generate is unavailable, in the user's terms; `null` when it is available.
     *
     * `validated` is one of the reasons on purpose: with the check on the other
     * side of an IPC call, "no problems reported" is also what a model nobody has
     * looked at yet looks like, and generating from that is exactly the mistake
     * the check exists to prevent. It costs one debounce window of a disabled
     * button after the last keystroke — never a permanently dead one, because a
     * failed check still settles (see `runValidate`). The button says which of
     * these it is rather than greying out mutely.
     */
    get generateBlockedReason(): string | null {
      if (!table.trim()) return 'No table chosen.';
      if (!columns.length) return 'Nothing is known about this table’s columns.';
      if (!enabledTargets.length) return 'No destination is enabled.';
      if (!rows.length) return 'There are no rows to write.';
      // Three of the four operations are defined by which row they find, and
      // nothing can supply that on its own: with no database connected there is no
      // primary key to fall back on, so the user has to say which columns identify
      // a row. Said here rather than left to a statement with an empty WHERE.
      if (operation !== 'insert' && !keyColumns.length) {
        return 'No comparison key — tick the columns that identify a row.';
      }
      if (!validated) return 'Checking the values…';
      // `rowIssues`, not `validation`: the latter describes the row on screen, and
      // with several rows the offending one is usually not the one being looked
      // at. Generating from a form whose third row is invalid because the first
      // one looks fine is exactly the mistake the check exists to prevent.
      if (rowIssues.size) {
        return rowIssues.size === 1
          ? 'Some values cannot be written as typed.'
          : `${rowIssues.size} rows have values that cannot be written as typed.`;
      }
      return null;
    },

    /** True when there is something worth generating and nothing invalid. */
    get canGenerate() {
      return this.generateBlockedReason === null;
    },

    setSource(next: DmlSource) {
      source = next;
      generated = false;
      applied = false;
    },

    setTable(next: string) {
      table = next;
      formRows = [{}];
      formCursor = 0;
      keySelection = {};
      importedRows = [];
      importedColumns = [];
      csvMapping = {};
      generated = false;
      applied = false;
    },

    setOperation(next: DmlOperation) {
      operation = next;
      generated = false;
      applied = false;
    },

    setValue(column: string, value: string) {
      const at = activeFormRow;
      formRows = formRows.map((row, i) => (i === at ? { ...row, [column]: value } : row));
      generated = false;
      applied = false;
    },

    toggleKey(column: string) {
      keySelection = { ...keySelection, [column]: !keySelection[column] };
      generated = false;
    },

    /** Move to another form row. Out-of-range indices are clamped, not refused. */
    selectFormRow(index: number) {
      formCursor = Math.max(0, Math.min(index, formRows.length - 1));
    },

    /**
     * Add a row after the current one and move to it.
     *
     * `copy` carries the current values across, which is what makes entering
     * twenty near-identical parameter rows bearable: change the code, change the
     * label, next. A blank row is the other half — the two are one button with a
     * modifier rather than two verbs, because they answer the same question.
     */
    addFormRow(copy = false) {
      const at = activeFormRow;
      const seed = copy ? { ...(formRows[at] ?? {}) } : {};
      formRows = [...formRows.slice(0, at + 1), seed, ...formRows.slice(at + 1)];
      formCursor = at + 1;
      generated = false;
      applied = false;
    },

    /**
     * Drop a form row. The last one is emptied rather than removed: a form with
     * no rows at all is a state with no way back to typing.
     */
    removeFormRow(index?: number) {
      const at = index ?? activeFormRow;
      if (formRows.length <= 1) {
        formRows = [{}];
        formCursor = 0;
      } else {
        formRows = formRows.filter((_, i) => i !== at);
        formCursor = Math.max(0, Math.min(at, formRows.length - 1));
      }
      generated = false;
      applied = false;
    },

    /** Empty every row and go back to one. The form's "start again". */
    clearForm() {
      formRows = [{}];
      formCursor = 0;
      generated = false;
      applied = false;
    },

    setPasteText(text: string) { pasteText = text; },
    setCsvText(text: string) { csvText = text; },

    /**
     * Re-read pasted INSERTs into rows. Reports what it could not read.
     *
     * Takes the **table and the column set from the statements**, so this source
     * works with nothing connected. The live schema is passed in so its types win
     * where it knows the column; when it does not, the paste's own columns stand.
     */
    parsePaste() {
      const res = parsePastedInserts(pasteText, schemaStore.table(table)?.columns ?? []);
      pasteErrors = res.errors;
      importedRows = res.rows;
      importedColumns = res.columns;
      if (res.table && res.table !== table.toUpperCase()) {
        // The statements name the table; adopt it rather than writing their rows
        // into whatever was selected before.
        table = res.table;
        formRows = [{}];
        formCursor = 0;
        keySelection = {};
      }
      generated = res.rows.length > 0 && res.errors.length === 0;
      applied = false;
      return res;
    },

    /** Read the CSV, propose a header → column mapping, and materialise the rows. */
    parseCsvSource() {
      const { headers, records } = parseCsv(csvText);
      const proposed = proposeCsvMapping(headers, columns);
      csvMapping = { ...proposed, ...csvMapping };
      importedRows = records.map((rec) => {
        const row: DmlRow = {};
        headers.forEach((h, i) => {
          const col = csvMapping[h];
          if (col) row[col] = rec[i] ?? '';
        });
        return row;
      });
      generated = importedRows.length > 0;
      applied = false;
      return { headers, records };
    },

    /** Re-point one CSV column and rebuild the rows from the current text. */
    setCsvMapping(header: string, column: string | null) {
      const next = { ...csvMapping };
      if (column) next[header] = column;
      else delete next[header];
      csvMapping = next;
      this.parseCsvSource();
    },

    get csvHeaders() { return csvHeaders; },

    // ── Targets ──────────────────────────────────────────────────────────────

    /**
     * Add a destination for a file, with its role's preset already applied.
     * Re-adding an existing file focuses it instead of duplicating: two
     * destinations writing the same file would each think they own it.
     */
    addTarget(input: {
      file: string;
      dialect: Target['dialect'];
      role: FolderRole;
      /** The destination folder's product, when the repository declares any. */
      product?: string | null;
    }): Target {
      const existing = targets.find((t) => t.file === input.file);
      if (existing) {
        existing.enabled = true;
        expandedTargetId = existing.id;
        return existing;
      }
      const preset = presetForRole(input.role);
      // A **portable** destination takes the intersection of the two dialects,
      // and the intersection contains no procedural block — so no version guard,
      // no existence check and no savepoint either, since all three live in one.
      // The role's preset is narrowed here rather than left to conflict: a
      // destination that arrives already broken teaches the user that the
      // conflict banner is noise, and the very first portable update folder they
      // add would do exactly that.
      const portable = input.dialect === GENERIC_ENGINE;
      const target: Target = {
        // The file IS the identity of a destination — two targets writing the
        // same path would each think they own it, which is why the add above
        // focuses the existing one instead of making a second.
        id: `t-${input.file.replace(/[^\w]+/g, '-').toLowerCase()}`,
        file: input.file,
        dialect: input.dialect,
        role: input.role,
        enabled: true,
        wrap: portable ? 'plain' : preset.wrap,
        guards: portable
          ? { version: null, skipIfPresent: false, requireObject: false, transactional: false }
          : {
              ...preset.guards,
              version: preset.guards.version ? { ...preset.guards.version } : null,
            },
        // Resolved once, here, from the folder's declared product — and then
        // *materialised on the target*, not looked up again at emission time. A
        // destination should keep saying what it said when it was reviewed, even
        // if somebody edits the product list in between; the field is visible and
        // editable in the destination editor for exactly that reason.
        //
        // `undefined` for the ordinary repository, which declares no products: the
        // backend then falls back to the project's own filter, exactly as before
        // any of this existed.
        versionFilter: input.product
          ? picusSettingsStore.versionFilterFor(input.product)
          : undefined,
      };
      targets = [...targets, target];
      expandedTargetId = target.id;
      previewTargetId = target.id;
      generated = false;
      return target;
    },

    /**
     * Replace the destinations with a resolved set, and answer with what could
     * not be used.
     *
     * **Replace, not add**: a set is a statement about where a change like this
     * goes, and merging it into whatever was already on screen would produce a
     * list nobody chose — with the previous release's update file still armed,
     * which is a write into a shipped script.
     *
     * Entries the backend could not resolve are skipped and returned so the caller
     * can say which. One dead folder costs one destination, never the set.
     *
     * The folder comes back beside the reason rather than folded into it: two
     * refusals concatenated into one toast are a paragraph nobody reads, and what
     * the reader needs first is *which* destinations went, not why each did.
     */
    applyDestinationSet(resolved: ResolvedDestination[]): RefusedDestination[] {
      const refused: RefusedDestination[] = [];
      this.resetTargets();
      for (const entry of resolved) {
        if (entry.problem || !entry.file || !entry.dialect) {
          refused.push({
            folder: entry.folder,
            reason: entry.problem ?? `${entry.folder} could not be resolved.`,
          });
          continue;
        }
        const target = this.addTarget({
          file: entry.file,
          dialect: entry.dialect as Target['dialect'],
          role: entry.role,
          product: entry.product ?? null,
        });
        target.wrap = entry.wrap;
        target.guards = {
          // The bounds come from the naming scheme where it could work them out —
          // which is the whole payoff of storing a folder rather than a path: a
          // release template arrives with `4.12 → 4.13` already in it. Empty
          // strings where it could not, so the fields are there to be typed in.
          version: entry.versionGuard
            ? { from: entry.fromVersion ?? '', to: entry.toVersion ?? '' }
            : null,
          skipIfPresent: entry.skipIfPresent,
          requireObject: entry.requireObject,
          transactional: entry.transactional,
        };
        normalise(target);
      }
      // The first armed destination, so the preview is showing something.
      previewTargetId = targets.find((t) => t.enabled)?.id ?? '';
      expandedTargetId = null;
      generated = false;
      applied = false;
      return refused;
    },

    /**
     * The current destinations as a savable set.
     *
     * The **folder** is stored alongside the file, and the version **bounds** are
     * dropped while the guard itself is kept: the bounds are re-derived from the
     * naming scheme on every apply, and last release's numbers filled in
     * automatically would look right and be wrong.
     *
     * The file name is sent for **every** destination, including the update ones.
     * Whether it can be dropped in favour of "the next file the scheme names" is
     * decided by the backend, which can read the folder — this side used to drop
     * it for any `update` role on the theory that the scheme would always know,
     * and for a folder whose file names the scheme cannot parse that threw away
     * the only thing making the entry work.
     */
    captureDestinationSet(name: string): DestinationSetInput {
      return {
        name,
        entries: targets.map((t) => {
          const cut = t.file.lastIndexOf('/');
          const folder = cut === -1 ? '' : t.file.slice(0, cut);
          const file = cut === -1 ? t.file : t.file.slice(cut + 1);
          return {
            folder,
            file,
            wrap: t.wrap,
            versionGuard: !!t.guards.version,
            // Sent for the same reason as the file, and kept or dropped by the
            // same decision on the backend: an entry the naming scheme can read
            // gets fresh bounds every release, one it cannot has nowhere else to
            // get them and used to come back with an empty guard.
            fromVersion: t.guards.version?.from,
            toVersion: t.guards.version?.to,
            skipIfPresent: t.guards.skipIfPresent,
            requireObject: t.guards.requireObject,
            transactional: t.guards.transactional,
          };
        }),
      };
    },

    removeTarget(id: string) {
      targets = targets.filter((t) => t.id !== id);
      if (expandedTargetId === id) expandedTargetId = null;
      if (previewTargetId === id) previewTargetId = targets.find((t) => t.enabled)?.id ?? '';
      generated = false;
    },

    toggleTarget(id: string) {
      const t = targets.find((x) => x.id === id);
      if (t) { t.enabled = !t.enabled; generated = false; applied = false; }
      if (t && !t.enabled && previewTargetId === id) {
        previewTargetId = enabledTargets[0]?.id ?? id;
      }
    },

    expandTarget(id: string) {
      expandedTargetId = expandedTargetId === id ? null : id;
    },

    setPreviewTarget(id: string) { previewTargetId = id; },

    /** Procedural block on/off. Off also drops the version guard. */
    setWrap(id: string, wrap: Target['wrap']) {
      const t = targets.find((x) => x.id === id);
      if (!t) return;
      t.wrap = wrap;
      normalise(t);
      generated = false;
    },

    /** Version guard on/off. On also switches the procedural block on. */
    setVersionGuard(id: string, on: boolean) {
      const t = targets.find((x) => x.id === id);
      if (!t) return;
      if (on) {
        t.wrap = 'block';
        t.guards.version ??= { from: '', to: '' };
      } else {
        t.guards.version = null;
      }
      generated = false;
    },

    /**
     * Which row of the version table this destination reads and stamps.
     *
     * `null` hands it back to the project's own filter — the difference between
     * "this destination has nothing to say about it" and "this destination wants
     * no predicate", which is `''`. Both are real answers for a repository that
     * installs several products.
     */
    setVersionFilter(id: string, filter: string | null) {
      const t = targets.find((x) => x.id === id);
      if (!t) return;
      if (filter === null) delete t.versionFilter;
      else t.versionFilter = filter;
      generated = false;
    },

    setVersionBound(id: string, which: 'from' | 'to', value: string) {
      const t = targets.find((x) => x.id === id);
      if (!t?.guards.version) return;
      t.guards.version[which] = value;
      generated = false;
    },

    setGuard(id: string, guard: 'skipIfPresent' | 'requireObject' | 'transactional', on: boolean) {
      const t = targets.find((x) => x.id === id);
      if (!t) return;
      t.guards[guard] = on;
      generated = false;
    },

    /**
     * Propagate one target's rules to every OTHER target with the same role.
     * Never across roles: an initialisation script must not inherit an update
     * script's version guard.
     *
     * `versionFilter` is deliberately **not** propagated. It is a fact about which
     * product the destination belongs to, not a rule the user chose — and two
     * update scripts of two different products are exactly the case it exists for,
     * so copying it would overwrite the right answer with another right answer.
     */
    copyRulesToSameRole(id: string): number {
      const src = targets.find((x) => x.id === id);
      if (!src) return 0;
      let n = 0;
      for (const t of targets) {
        if (t.id === src.id || t.role !== src.role) continue;
        t.wrap = src.wrap;
        t.guards = {
          version: src.guards.version ? { ...src.guards.version } : null,
          skipIfPresent: src.guards.skipIfPresent,
          requireObject: src.guards.requireObject,
          transactional: src.guards.transactional,
        };
        normalise(t);
        n++;
      }
      generated = false;
      return n;
    },

    /** Apply the per-role preset to a target, discarding its manual rules. */
    resetTargetToPreset(id: string) {
      const t = targets.find((x) => x.id === id);
      if (!t) return;
      const preset = presetForRole(t.role);
      t.wrap = preset.wrap;
      t.guards = { ...preset.guards, version: preset.guards.version ? { ...preset.guards.version } : null };
      generated = false;
    },

    // ── Emission ─────────────────────────────────────────────────────────────

    /**
     * The SQL one target received from the last emission.
     *
     * A **pure read**: the cache is filled by the effect above, never here, so
     * this is safe to call from a `$derived` or from markup. Empty until the first
     * round trip lands — the preview has no refresh button by design, so it is
     * always a matter of milliseconds, not of pressing anything.
     */
    sqlFor(target: Target): string {
      return emittedSql[target.id] ?? '';
    },

    /**
     * Why this destination's rules cannot all apply — a version guard on a target
     * that emits bare statements has nothing to return from. `null` when coherent.
     */
    ruleConflictFor(id: string): string | null {
      return ruleConflicts[id] ?? null;
    },

    markGenerated() { generated = true; applied = false; },
    reset() {
      generated = false;
      applied = false;
      applyError = null;
      previewFiles = [];
      previewedKey = '';
    },

    /**
     * Forget every destination — the repository underneath them changed.
     *
     * A target is a path inside one repository; carrying it over to another would
     * mean writing into a file that happens to share a name, which is the worst
     * available outcome of switching connection.
     */
    resetTargets() {
      targets = [];
      expandedTargetId = null;
      previewTargetId = '';
      previewFiles = [];
      previewedKey = '';
      previewError = null;
      applyError = null;
      generated = false;
      applied = false;
    },
  };
}

export const dmlStore = createDmlStore();
