/**
 * Picus DML generator — three input sources, one intermediate model, N per-target
 * emissions.
 *
 * The shape of the whole feature: a form / a pasted set of INSERTs / a CSV all
 * collapse into `{ table, operation, keyColumns, rows }`, and from there the
 * source no longer matters. Each **target** is one file in one branch, with its
 * own dialect and its own rules, so the same logical change becomes a bare
 * INSERT in the Oracle init script and a guarded PL/SQL block in the update one.
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

import type {
  Column,
  DmlOperation,
  DmlRow,
  DmlSource,
  FolderRole,
  Target,
  TargetGuards,
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
  type ApplyResult,
  type PreviewFile,
} from '$lib/ipc/picus/scripts';
import { parseCsv, proposeCsvMapping } from '$lib/utils/picus/csv';
import { parsePastedInserts } from '$lib/utils/picus/paste-sql';
import { picusProjectStore } from './project.svelte';
import { schemaStore } from './schema.svelte';
import { picusSettingsStore } from './settings.svelte';

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

  /** Form mode: one value per column. */
  let values = $state<Record<string, string>>({});
  /** Explicit comparison-key selection; empty falls back to the primary key. */
  let keySelection = $state<Record<string, boolean>>({});

  let pasteText = $state('');
  let pasteErrors = $state<string[]>([]);
  let csvText = $state('');
  /** CSV header → table column. Explicit, with a name-match proposal. */
  let csvMapping = $state<Record<string, string>>({});

  /** Rows read from the paste/CSV sources. Form mode derives its single row. */
  let importedRows = $state<DmlRow[]>([]);

  /** Destinations. Empty until the user adds one from the open repository. */
  let targets = $state<Target[]>([]);
  let expandedTargetId = $state<string | null>(null);
  let previewTargetId = $state<string>('');

  let generated = $state(false);
  let applied = $state(false);

  // Only real tables: DML is written against something writable, so a view
  // never shows up here even though it has columns.
  const columns = $derived<Column[]>(schemaStore.table(table)?.columns ?? []);

  const keyColumns = $derived.by<Column[]>(() => {
    const picked = columns.filter((c) => keySelection[c.name]);
    return picked.length ? picked : columns.filter((c) => c.primaryKey);
  });

  const rows = $derived<DmlRow[]>(
    source === 'form'
      ? [columns.reduce<DmlRow>((acc, c) => { acc[c.name] = values[c.name] ?? ''; return acc; }, {})]
      : importedRows,
  );

  const enabledTargets = $derived(targets.filter((t) => t.enabled));
  const csvHeaders = $derived(parseCsv(csvText).headers);

  const model = $derived<DmlModel>({
    table,
    operation,
    columns,
    keyColumns,
    rows,
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
    for (const p of problems) {
      if (p.row === 0) out[p.column] = p.reason;
    }
    return out;
  });

  /** Rows that fail their column types, per imported row index. */
  const rowIssues = $derived.by(() => {
    const out = new Map<number, string[]>();
    if (source === 'form') return out;
    for (const p of problems) {
      const list = out.get(p.row);
      if (list) list.push(`${p.column}: ${p.reason}`);
      else out.set(p.row, [`${p.column}: ${p.reason}`]);
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
    get values() { return values; },
    get keySelection() { return keySelection; },
    get columns() { return columns; },
    get keyColumns() { return keyColumns; },
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
      if (!enabledTargets.length) return 'No destination is enabled.';
      if (!rows.length) return 'There are no rows to write.';
      if (!validated) return 'Checking the values…';
      const bad = source === 'form' ? Object.keys(validation).length : rowIssues.size;
      return bad ? 'Some values cannot be written as typed.' : null;
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
      values = {};
      keySelection = {};
      importedRows = [];
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
      values = { ...values, [column]: value };
      generated = false;
      applied = false;
    },

    toggleKey(column: string) {
      keySelection = { ...keySelection, [column]: !keySelection[column] };
      generated = false;
    },

    clearForm() {
      values = {};
      generated = false;
      applied = false;
    },

    setPasteText(text: string) { pasteText = text; },
    setCsvText(text: string) { csvText = text; },

    /** Re-read pasted INSERTs into rows. Reports what it could not read. */
    parsePaste() {
      const res = parsePastedInserts(pasteText, columns);
      pasteErrors = res.errors;
      importedRows = res.rows;
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
    addTarget(input: { file: string; dialect: Target['dialect']; role: FolderRole; branchId: string }): Target {
      const existing = targets.find((t) => t.file === input.file);
      if (existing) {
        existing.enabled = true;
        expandedTargetId = existing.id;
        return existing;
      }
      const preset = presetForRole(input.role);
      const target: Target = {
        id: `t-${input.branchId}-${input.file.replace(/[^\w]+/g, '-').toLowerCase()}`,
        file: input.file,
        dialect: input.dialect,
        role: input.role,
        branchId: input.branchId,
        enabled: true,
        wrap: preset.wrap,
        guards: { ...preset.guards, version: preset.guards.version ? { ...preset.guards.version } : null },
      };
      targets = [...targets, target];
      expandedTargetId = target.id;
      previewTargetId = target.id;
      generated = false;
      return target;
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
