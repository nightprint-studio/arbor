<script lang="ts">
  /**
   * Picus settings — built on the shared `SettingsShell`, so it reads exactly
   * like every other settings surface in the suite.
   *
   * Each entry corresponds to a decision the product would otherwise make
   * silently: which encoding to assume when the heuristics can't decide, where a
   * generated block lands in a file, whether a write asks first. That is the bar
   * for adding a setting — make an assumption visible, don't defer a design
   * choice.
   */
  import { Settings, FileType, PenLine, FormInput, Database, FolderCog, Hash, Wand2 } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import SettingsShell, { type SettingsNavGroup } from '$lib/components/shared/ui/SettingsShell.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import {
    picusSettingsStore,
    INSERTION_RULE_LABELS,
    type InsertionRule,
  } from '$lib/stores/picus/settings.svelte';
  import { schemaStore } from '$lib/stores/picus/schema.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';

  let { onClose }: { onClose: () => void } = $props();

  let active = $state('project');

  const groups: SettingsNavGroup[] = [
    {
      label: 'Project',
      items: [
        { id: 'project', label: 'Project', icon: FolderCog },
        { id: 'version', label: 'Version table', icon: Hash },
      ],
    },
    {
      label: 'Scripts',
      items: [
        { id: 'encoding', label: 'Encoding', icon: FileType },
        { id: 'writing', label: 'Writing to disk', icon: PenLine },
      ],
    },
    {
      label: 'Work',
      items: [
        { id: 'generation', label: 'Generation', icon: FormInput },
        { id: 'queries', label: 'Queries', icon: Database },
      ],
    },
  ];

  const eolOptions = [
    { value: 'CRLF', label: 'CRLF (Windows)' },
    { value: 'LF', label: 'LF (Unix)' },
  ];

  /** Columns of the configured version table, for the column pickers. */
  const versionTableColumns = $derived(
    schemaStore.table(picusSettingsStore.versionTable.table)?.columns ?? [],
  );

  const versionColumnOptions = $derived(
    versionTableColumns.length
      ? versionTableColumns.map((c) => ({ value: c.name, label: `${c.name}  (${c.type})` }))
      : [{
          value: picusSettingsStore.versionTable.versionColumn,
          label: picusSettingsStore.versionTable.versionColumn,
        }],
  );

  /** The date column is optional — "none" is a real, and common, answer. */
  const dateColumnOptions = $derived([
    { value: '', label: '— this project stamps no date —' },
    ...versionTableColumns
      .filter((c) => /DATE|TIME/i.test(c.type))
      .map((c) => ({ value: c.name, label: `${c.name}  (${c.type})` })),
  ]);

  const tableOptions = $derived(schemaStore.tables.map((t) => ({ value: t.name, label: t.name })));

  /**
   * The guard as it will actually be emitted, from the current configuration.
   * Showing it here is the difference between four settings and a rule you can
   * picture — and picturing it is how you notice the date column is wrong.
   */
  const versionPreview = $derived.by(() => {
    const v = picusSettingsStore.versionTable;
    if (!v.table.trim()) return '-- version guards are disabled for this project';
    const where = v.filter.trim() ? `\n   WHERE ${v.filter.trim()}` : '';
    const sets = [`${v.versionColumn} = '4.13'`];
    if (v.dateColumn) sets.push(`${v.dateColumn} = SYSDATE`);
    return (
      `SELECT ${v.versionColumn} INTO v_versione FROM ${v.table}${where};\n` +
      `IF v_versione <> '4.12' THEN\n  RETURN;\nEND IF;\n` +
      `…\n` +
      `UPDATE ${v.table} SET ${sets.join(', ')}${where};`
    );
  });

  function detectVersionTable() {
    const found = picusSettingsStore.detectVersionTable(schemaStore.tables);
    if (!found) {
      toastStore.show('No table on this connection looks like a version table.', 'warning');
      return;
    }
    picusSettingsStore.setVersionTable(found);
    toastStore.show(
      found.dateColumn
        ? `Found ${found.table}: version in ${found.versionColumn}, date in ${found.dateColumn}.`
        : `Found ${found.table}: version in ${found.versionColumn}, no date column.`,
      'success',
    );
  }

  const encodingOptions = [
    { value: 'windows-1252', label: 'windows-1252 (legacy Western European)' },
    { value: 'UTF-8', label: 'UTF-8' },
    { value: 'ISO-8859-1', label: 'ISO-8859-1' },
    { value: 'ISO-8859-15', label: 'ISO-8859-15' },
  ];

  const insertionOptions = (Object.keys(INSERTION_RULE_LABELS) as InsertionRule[]).map((k) => ({
    value: k,
    label: INSERTION_RULE_LABELS[k],
  }));
</script>

<Modal {onClose} width="900px" height="620px" padBody={false} ariaLabel="Picus settings">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Settings size={14} />
      <span class="modal-title">Settings</span>
    </ModalHeader>
  {/snippet}

  <SettingsShell {groups} bind:active>
    {#snippet content()}
      {#if active === 'project'}
        <div class="section-header">
          <h2>Project</h2>
          <p>
            Facts about <strong>{picusProjectStore.project?.name ?? 'this project'}</strong> that the
            rest of Picus reads: how its scripts are written, and how they are read back.
          </p>
        </div>
        <div class="card">
          <FormRow
            label="Script encoding"
            description="What this project's scripts are declared to be. Detection is compared against it, so a file coming back in another encoding is a finding (ENC001) rather than the new normal."
          >
            <Select
              value={picusSettingsStore.projectEncoding}
              options={encodingOptions}
              onchange={(v) => picusSettingsStore.setProjectEncoding(v)}
            />
          </FormRow>
          <FormRow
            label="Line ending"
            description="Used for content Picus writes. Existing files keep their own, whatever this says."
          >
            <Select
              value={picusSettingsStore.projectEol}
              options={eolOptions}
              onchange={(v) => picusSettingsStore.setProjectEol(v as 'CRLF' | 'LF')}
            />
          </FormRow>
          <FormRow
            label="Lowercase identifiers on PostgreSQL"
            description="PostgreSQL folds unquoted identifiers to lowercase; emitting them that way keeps generated scripts consistent with what the server reports back."
          >
            <Toggle
              checked={picusSettingsStore.lowercasePostgres}
              size="sm"
              ariaLabel="Lowercase identifiers on PostgreSQL"
              onchange={(v) => picusSettingsStore.setLowercasePostgres(v)}
            />
          </FormRow>
        </div>
        <Alert
          variant="info"
          compact
          text="These belong to the project, not to Picus: they will live in the project's own configuration file, so a colleague opening the same repository inherits them."
        />

      {:else if active === 'version'}
        <div class="section-header">
          <h2>Version table</h2>
          <p>
            Where the installed version is recorded. Every project stamps one, but not in the
            same shape — and the version guard, the most valuable rule Picus has, is built
            entirely on getting this right.
          </p>
        </div>
        <div class="card">
          <FormRow
            label="Table"
            description="Leave empty to disable version guards entirely for this project."
          >
            <div class="ps-row">
              <Select
                value={picusSettingsStore.versionTable.table}
                options={tableOptions}
                onchange={(v) => picusSettingsStore.setVersionTable({ table: v })}
              />
              <Button
                variant="secondary"
                size="xs"
                tooltip={'Look for a version table on the active connection and fill these in'}
                onclick={detectVersionTable}
              >
                {#snippet iconStart()}<Wand2 size={12} />{/snippet}
                Detect
              </Button>
            </div>
          </FormRow>

          <FormRow label="Version column" description="Holds the version string the guard compares against.">
            <Select
              value={picusSettingsStore.versionTable.versionColumn}
              options={versionColumnOptions}
              onchange={(v) => picusSettingsStore.setVersionTable({ versionColumn: v })}
            />
          </FormRow>

          <FormRow
            label="Date column"
            description="Optional. Plenty of version tables hold nothing but the version string — with no date column, the closing UPDATE simply doesn't mention one instead of failing on a column that isn't there."
          >
            <Select
              value={picusSettingsStore.versionTable.dateColumn ?? ''}
              options={dateColumnOptions}
              onchange={(v) => picusSettingsStore.setVersionTable({ dateColumn: v || null })}
            />
          </FormRow>

          <FormRow
            label="Row filter"
            description="For version tables holding one row per module — e.g. MODULO = 'CORE'. Empty means the table holds a single row."
          >
            <Input
              value={picusSettingsStore.versionTable.filter}
              placeholder="MODULO = 'CORE'"
              oninput={(v) => picusSettingsStore.setVersionTable({ filter: v })}
            />
          </FormRow>
        </div>

        <div class="info-box">
          <strong>What the guard will emit</strong>
          <pre class="ps-preview">{versionPreview}</pre>
        </div>

      {:else if active === 'encoding'}
        <div class="section-header">
          <h2>Encoding</h2>
          <p>How a file's encoding is decided when the bytes alone don't settle it.</p>
        </div>
        <div class="card">
          <FormRow
            label="Fallback encoding"
            description="Used when there is no byte-order mark and the content is not valid multibyte UTF-8. windows-1252 is the right default for legacy Western European repositories: every byte maps to a distinct codepoint, so the round-trip is lossless."
          >
            <Select
              value={picusSettingsStore.defaultEncoding}
              options={encodingOptions}
              onchange={(v) => picusSettingsStore.setDefaultEncoding(v)}
            />
          </FormRow>
          <FormRow
            label="Pure-ASCII files inherit the folder's encoding"
            description="An ASCII-only file is genuinely ambiguous — it is valid in every candidate encoding. With this on it is marked neutral and takes the folder's dominant encoding, so rewriting it can't change the folder's mix."
          >
            <Toggle
              checked={picusSettingsStore.inheritAsciiEncoding}
              size="sm"
              ariaLabel="Inherit the folder encoding for ASCII-only files"
              onchange={(v) => picusSettingsStore.setInheritAsciiEncoding(v)}
            />
          </FormRow>
        </div>
        <Alert
          variant="info"
          compact
          text="A character that can't be represented in a file's encoding always blocks the write and names the character and the line. That is not configurable."
        />

      {:else if active === 'writing'}
        <div class="section-header">
          <h2>Writing to disk</h2>
          <p>What happens between a reviewed diff and changed bytes. Where the generated block lands inside a file is a generation setting.</p>
        </div>
        <div class="card">
          <FormRow
            label="Confirm before writing"
            description="The confirmation names how many files will change, which ones, and what stays untouched. Turning this off is deliberate — every other safeguard stays."
          >
            <Toggle
              checked={picusSettingsStore.confirmBeforeWrite}
              size="sm"
              ariaLabel="Confirm before writing"
              onchange={(v) => picusSettingsStore.setConfirmBeforeWrite(v)}
            />
          </FormRow>
          <FormRow
            label="Back up before rewriting"
            description="Originals are copied to .arbor/backup/<timestamp>/ and are what a failed multi-file write is rolled back from."
          >
            <Toggle
              checked={picusSettingsStore.backupBeforeWrite}
              size="sm"
              ariaLabel="Back up before rewriting"
              onchange={(v) => picusSettingsStore.setBackupBeforeWrite(v)}
            />
          </FormRow>
        </div>

      {:else if active === 'generation'}
        <div class="section-header">
          <h2>Generation</h2>
          <p>How the emitters behave unless a destination overrides them.</p>
        </div>
        <div class="card">
          <FormRow
            label="Insertion point — initialisation scripts"
            description="A dull, stated rule beats a clever, unpredictable one: knowing where the block lands is half of trusting the write."
          >
            <Select
              value={picusSettingsStore.insertionRuleInit}
              options={insertionOptions}
              onchange={(v) => picusSettingsStore.setInsertionRuleInit(v as InsertionRule)}
            />
          </FormRow>
          <FormRow label="Insertion point — update scripts">
            <Select
              value={picusSettingsStore.insertionRuleUpdate}
              options={insertionOptions}
              onchange={(v) => picusSettingsStore.setInsertionRuleUpdate(v as InsertionRule)}
            />
          </FormRow>
        </div>
        <Alert
          variant="info"
          compact
          text="Generation is deterministic: structured input → model → per-dialect emission. No language model takes part at any point in the flow."
        />

      {:else}
        <div class="section-header">
          <h2>Queries</h2>
          <p>How much a query brings back before you ask for the rest.</p>
        </div>
        <div class="card">
          <FormRow
            label="Row limit"
            description="Rows a query fetches. Applied by the server wherever the statement allows, so the rest never crosses the network; a result that reaches the limit says so above its rows. Table data is paged instead, and every page is rendered through the virtualised grid — so a large page costs no more to display than a small one."
          >
            <NumberStepper
              value={picusSettingsStore.rowLimit}
              min={10}
              max={100000}
              step={100}
              onchange={(v) => picusSettingsStore.setRowLimit(v)}
            />
          </FormRow>
        </div>
      {/if}
    {/snippet}
  </SettingsShell>
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }

  /* Table picker + Detect on one line: the button acts on the field beside it. */
  .ps-row { display: flex; align-items: center; gap: 8px; }

  /* The emitted-guard preview — the reason the four fields above are legible. */
  .ps-preview {
    margin: 6px 0 0;
    padding: 9px 11px;
    background: var(--bg-input);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font-family: var(--font-code);
    font-size: 11px;
    line-height: 1.6;
    color: var(--text-secondary);
    white-space: pre;
    overflow-x: auto;
  }
</style>
