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
 * MOCK: emission runs through `ipc/picus/mock-emit` (a TypeScript stand-in for
 * the `picus-emit` crate) until `picus-be` exists.
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
import { MOCK_CSV, MOCK_PASTED_SQL, MOCK_TARGETS } from '$lib/ipc/picus/mock';
import {
  emitForTarget,
  parseCsv,
  parsePastedInserts,
  proposeCsvMapping,
  validateValue,
  type DmlModel,
} from '$lib/ipc/picus/mock-emit';
import { schemaStore } from './schema.svelte';
import { picusSettingsStore } from './settings.svelte';

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

function cloneTarget(t: Target): Target {
  return {
    ...t,
    guards: { ...t.guards, version: t.guards.version ? { ...t.guards.version } : null },
  };
}

function createDmlStore() {
  let source = $state<DmlSource>('form');
  let table = $state('PARAMETRI');
  let operation = $state<DmlOperation>('upsert');

  /** Form mode: one value per column. */
  let values = $state<Record<string, string>>({
    COD_PARAMETRO: 'SOGLIA_SCONTO',
    VALORE: '15',
    DESCRIZIONE: 'Soglia sconto massimo applicabile',
    DATA_MOD: 'SYSDATE',
  });
  /** Explicit comparison-key selection; empty falls back to the primary key. */
  let keySelection = $state<Record<string, boolean>>({ COD_PARAMETRO: true });

  let pasteText = $state(MOCK_PASTED_SQL);
  let pasteErrors = $state<string[]>([]);
  let csvText = $state(MOCK_CSV);
  /** CSV header → table column. Explicit, with a name-match proposal. */
  let csvMapping = $state<Record<string, string>>({});

  /** Rows read from the paste/CSV sources. Form mode derives its single row. */
  let importedRows = $state<DmlRow[]>([]);

  let targets = $state<Target[]>(MOCK_TARGETS.map(cloneTarget));
  let expandedTargetId = $state<string | null>('t-ora-upd');
  let previewTargetId = $state<string>('t-ora-upd');

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

  /** Per-column validation of the form, recomputed as the user types. */
  const validation = $derived.by(() => {
    const out: Record<string, string> = {};
    for (const c of columns) {
      const msg = validateValue(values[c.name] ?? '', c);
      if (msg) out[c.name] = msg;
    }
    return out;
  });

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

  /** Rows that fail their column types, per imported row index. */
  const rowIssues = $derived.by(() => {
    const out = new Map<number, string[]>();
    importedRows.forEach((row, i) => {
      const msgs: string[] = [];
      for (const c of columns) {
        const msg = validateValue(row[c.name] ?? '', c);
        if (msg) msgs.push(`${c.name}: ${msg}`);
      }
      if (msgs.length) out.set(i, msgs);
    });
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

    /** True when there is something worth generating and nothing invalid. */
    get canGenerate() {
      if (!enabledTargets.length || !rows.length) return false;
      if (source === 'form') return Object.keys(validation).length === 0;
      return rowIssues.size === 0;
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

    /** The SQL one target receives. Recomputed on every read — the preview has
     *  no refresh button by design. */
    sqlFor(target: Target): string {
      return emitForTarget(model, target);
    },

    markGenerated() { generated = true; applied = false; },
    markApplied() { applied = true; },
    reset() { generated = false; applied = false; },
  };
}

export const dmlStore = createDmlStore();
