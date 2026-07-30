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
  import {
    Settings, FileType, PenLine, FormInput, Database, FolderCog, Hash, Tags, Wand2,
    ShieldCheck, Save,
  } from 'lucide-svelte';
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
  import ProjectAliases from './settings/ProjectAliases.svelte';
  import ProjectProducts from './settings/ProjectProducts.svelte';
  import { RULE_FAMILIES } from './settings/rule-catalogue';
  import type { InitialisationModel } from '$lib/ipc/picus/project';
  import {
    picusSettingsStore,
    INSERTION_RULE_LABELS,
    type InsertionRule,
  } from '$lib/stores/picus/settings.svelte';
  import { schemaStore } from '$lib/stores/picus/schema.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';

  /**
   * `initialSection` lets a caller address one page by name — the command palette
   * does, because "Folder names in this project…" is a verb of the product and
   * making the user find it inside a dialog would be exactly the mouse detour the
   * palette exists to remove.
   */
  let { onClose, initialSection = '' }: { onClose: () => void; initialSection?: string } = $props();

  // svelte-ignore state_referenced_locally
  let active = $state(initialSection || 'project');

  const groups: SettingsNavGroup[] = [
    {
      label: 'Project',
      items: [
        { id: 'project', label: 'Project', icon: FolderCog },
        { id: 'aliases', label: 'Folder names', icon: Tags },
        { id: 'version', label: 'Version table', icon: Hash },
        { id: 'analysis', label: 'Analysis', icon: ShieldCheck },
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
  const dateColumnOptions = $derived.by(() => {
    const configured = picusSettingsStore.versionTable.dateColumn ?? '';
    const dated = versionTableColumns.filter((c) => /DATE|TIME/i.test(c.type));
    const out = [{ value: '', label: '— this project stamps no date —' }];
    // Same reasoning as the table picker: the column is the project's answer and
    // the catalogue is the connection's, so a setting must not vanish from the
    // list because nobody is connected.
    if (configured && !dated.some((c) => c.name.toLowerCase() === configured.toLowerCase())) {
      out.push({ value: configured, label: `${configured}  (not on this connection)` });
    }
    return out.concat(dated.map((c) => ({ value: c.name, label: `${c.name}  (${c.type})` })));
  });

  /**
   * What the table picker offers.
   *
   * Three things, and each is there because leaving it out broke something:
   *
   * * **"none"**, because emptying the name is a documented, meaningful answer —
   *   it switches the version guards off — and the picker previously had no way
   *   to express it, so the description said "leave empty" next to a control that
   *   could not be left empty;
   * * **the configured table**, even when this connection's catalogue does not
   *   contain it. The table name belongs to the *project* and the catalogue
   *   belongs to the *connection*: opening the settings with no connection, or on
   *   another database, must not make a perfectly good setting look unset — or,
   *   worse, let a stray click erase it;
   * * **the catalogue**, which is the useful case and the only one that was here.
   */
  const tableOptions = $derived.by(() => {
    const configured = picusSettingsStore.versionTable.table.trim();
    const known = schemaStore.tables.map((t) => t.name);
    const out = [{ value: '', label: '— none: switch the version guards off —' }];
    if (configured && !known.some((n) => n.toLowerCase() === configured.toLowerCase())) {
      out.push({ value: configured, label: `${configured}  (not on this connection)` });
    }
    return out.concat(known.map((name) => ({ value: name, label: name })));
  });

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

  // ── The project half ────────────────────────────────────────────────────────
  //
  // Everything under the "Project" group lands in `.arbor/picus/project.toml`,
  // which is committed into the user's repository. So it is written on an
  // explicit Save and never as you type — the same rule that governs every other
  // write Picus makes to somebody's tree.

  /** Which pages edit the project file, and therefore show the save bar. */
  const PROJECT_PAGES = ['project', 'version', 'analysis'];
  const editingProject = $derived(PROJECT_PAGES.includes(active));

  $effect(() => {
    void picusSettingsStore.loadProject(picusProjectStore.root);
  });

  const initialisationOptions = [
    {
      value: 'cumulative',
      label: 'Cumulative — the initialisation is kept at the latest version',
    },
    {
      value: 'mirrored',
      label: 'Mirrored — the two halves must agree in both directions',
    },
    {
      value: 'independent',
      label: 'Independent — the two halves are maintained separately',
    },
  ];

  /** What the chosen model actually does to the report, spelled out. */
  const initialisationEffect = $derived.by(() => {
    switch (picusSettingsStore.initialisation) {
      case 'mirrored':
        return 'CONS002 and CONS003 both run: every row installed must also be carried forward by an update, and every row an update adds must also be seeded.';
      case 'independent':
        return 'Neither CONS002 nor CONS003 runs. The report says so rather than looking clean.';
      default:
        return 'CONS003 runs — a row an update adds must also be in the initialisation, or a fresh install comes up missing something every older database has. CONS002 does not: a row the initialisation holds and no update carries is a first-release row, and there is no update for the beginning. The cost is real and worth knowing: adding a row to the initialisation and forgetting the matching update script is a genuine mistake, and under this model nothing catches it.';
    }
  });

  async function saveProject() {
    const outcome = await picusSettingsStore.saveProject(picusProjectStore.root);
    if (typeof outcome === 'string') {
      toastStore.show(`The project settings were not saved — ${outcome}`, 'error');
      return;
    }
    toastStore.show(`Project settings saved in ${outcome.configPath}.`, 'success');
    for (const problem of outcome.problems) toastStore.show(problem, 'warning');
    // What the rules are allowed to look at changed, so the verdict on screen
    // describes a configuration that no longer exists.
    void picusProjectStore.analyze();
  }
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
        </div>
        <Alert
          variant="info"
          compact
          text="These belong to the project, not to this machine: they live in the repository's own .arbor/picus/project.toml, so a colleague opening the same folder inherits them."
        />

      {:else if active === 'aliases'}
        <!-- A page of its own rather than another branch in here: this one owns a
             list, a write and an async count, and the settings dialog is a layout,
             not a place for a feature to live. -->
        <ProjectAliases />

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
            wideControl
          >
            <div class="ps-row">
              <!-- Filterable and full width, because a real schema has hundreds
                   of tables and the answer is one the user can already name.
                   Sized to the container rather than to the selected label: a
                   picker that collapses to the width of `T1` cannot be read. -->
              <Select
                value={picusSettingsStore.versionTable.table}
                options={tableOptions}
                searchable
                fill
                searchPlaceholder="Filter tables…"
                placeholder="— none: version guards are off —"
                emptyMessage="No table on this connection — connect to one, or type the name into the project file directly."
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

          <FormRow
            label="Version column"
            description="Holds the version string the guard compares against."
            wideControl
          >
            <Select
              value={picusSettingsStore.versionTable.versionColumn}
              options={versionColumnOptions}
              searchable={versionColumnOptions.length > 12}
              fill
              onchange={(v) => picusSettingsStore.setVersionTable({ versionColumn: v })}
            />
          </FormRow>

          <FormRow
            label="Date column"
            description="Optional. Plenty of version tables hold nothing but the version string — with no date column, the closing UPDATE simply doesn't mention one instead of failing on a column that isn't there."
            wideControl
          >
            <Select
              value={picusSettingsStore.versionTable.dateColumn ?? ''}
              options={dateColumnOptions}
              fill
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

        <!-- A repository that installs more than one product has a version table
             per module, and an update script belonging to the second module
             guards against the second table — perfectly correctly. With one name
             declared, every one of those scripts was reported as unguarded. -->
        <div class="card">
          <FormRow
            label="Other version tables"
            description="One per module, when this repository installs more than one product. An update script that guards against any of them counts as guarded. Generation still stamps the table above — something has to be stamped, and that is the one the project named first."
            wideControl
          >
            <Input
              value={picusSettingsStore.otherVersionTables.join(', ')}
              placeholder="VERSIONE_PORTALE, VERSIONE_DBPORT"
              oninput={(v) =>
                picusSettingsStore.setOtherVersionTables(
                  v.split(',').map((n) => n.trim()).filter(Boolean),
                )}
            />
          </FormRow>
        </div>

        <!-- A repository that installs several products into ONE version table:
             the row a generated block reads and stamps is then a property of where
             the script is going, not of the project. Declared here, assigned to
             folders in the classifier. -->
        <div class="card">
          <FormRow
            label="Products"
            description="When this repository installs more than one product into the same version table. Each names the predicate that selects its row; folders then say which product's scripts they hold, and a generated block stamps the right one without the predicate being retyped per destination."
            wideControl
          >
            <ProjectProducts />
          </FormRow>
        </div>

        <div class="info-box">
          <strong>What the guard will emit</strong>
          <pre class="ps-preview">{versionPreview}</pre>
        </div>

      {:else if active === 'analysis'}
        <div class="section-header">
          <h2>Analysis</h2>
          <p>
            What the consistency check is allowed to assume about this repository, and which
            rules it runs. Both belong to the project: a colleague opening the same folder must
            get the same verdict you do.
          </p>
        </div>

        <div class="card">
          <!-- First, because it decides whether two of the fourteen rules run at
               all — and on a repository whose halves have genuinely diverged it
               is the difference between a usable report and a wall. -->
          <FormRow
            label="Compare the two dialects against each other"
            description="The comparison Picus exists for: an object one engine's scripts change and the other's never do, and a table the two fill in differently. Switch it off for a repository whose two halves have drifted far enough apart that the comparison says nothing you can act on — the version chain, the duplicates, the dangerous DML and the encodings all keep working."
          >
            <Toggle
              checked={picusSettingsStore.compareDialects}
              size="sm"
              ariaLabel="Compare the two dialects"
              onchange={(v) => picusSettingsStore.setCompareDialects(v)}
            />
          </FormRow>
          <FormRow
            label="What the initialisation folders are"
            description="Not derivable from the SQL — it is a fact about how the team works, and each half of the install-versus-upgrade check only makes sense under one reading of it."
            wideControl
          >
            <Select
              value={picusSettingsStore.initialisation}
              options={initialisationOptions}
              fill
              onchange={(v) => picusSettingsStore.setInitialisation(v as InitialisationModel)}
            />
          </FormRow>
        </div>
        <Alert variant="info" compact text={initialisationEffect} />

        <div class="card">
          <!-- Named objects rather than a whole rule, because silencing one table
               by switching a rule off stops it watching the other four hundred. -->
          <FormRow
            label="Objects the rules say nothing about"
            description="For the handful of tables that are a special case for a reason nothing in the scripts can express — a staging table one dialect fills, a log the installer writes, a legacy table kept for one customer. Matched on the name whatever kind of object carries it. They stay in the Inventory with their coverage: what is in the repository and what should be checked are different questions."
            wideControl
          >
            <Input
              value={picusSettingsStore.excludedObjects.join(', ')}
              placeholder="MECATALOGO, STAGING_IMPORT"
              oninput={(v) =>
                picusSettingsStore.setExcludedObjects(
                  v.split(',').map((n) => n.trim()).filter(Boolean),
                )}
            />
          </FormRow>
        </div>

        <div class="section-header ps-sub">
          <h3>Rules</h3>
          <p>
            A rule you switch off is never silently absent: it is reported in the check as a rule
            that did not run, with this page named as the reason. A clean report has to keep
            meaning something.
          </p>
        </div>

        {#each RULE_FAMILIES as family (family.label)}
          <div class="card">
            <div class="ps-family">
              <span class="ps-family-name">{family.label}</span>
              <span class="ps-family-blurb">{family.blurb}</span>
            </div>
            {#each family.rules as rule (rule.id)}
              <!-- The id is part of the label rather than a chip beside it: it is
                   what a `-- picus: ignore CONS002 — …` comment names and what the
                   report prints, so it has to be readable and copyable from here. -->
              <FormRow
                label={`${rule.id} · ${rule.title}`}
                description={rule.offWhen ? `Reasonable to switch off when: ${rule.offWhen}` : undefined}
              >
                <Toggle
                  checked={picusSettingsStore.ruleEnabled(rule.id)}
                  size="sm"
                  ariaLabel={`Run ${rule.id}`}
                  onchange={(on) => picusSettingsStore.setRuleEnabled(rule.id, on)}
                />
              </FormRow>
            {/each}
          </div>
        {/each}

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
          <!-- Here rather than on the Project page: this is *your* preference and
               it is stored in your profile, while everything under Project is
               stored in the repository. A setting filed under the wrong one of
               those is how a team ends up with two people generating differently
               shaped SQL and no idea why. -->
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
          text="Generation is deterministic: structured input → model → per-dialect emission. No language model takes part at any point in the flow."
        />

      {:else}
        <div class="section-header">
          <h2>Queries</h2>
          <p>How much of a result comes back at a time.</p>
        </div>
        <div class="card">
          <FormRow
            label="Rows per window"
            description="Rows fetched in one trip. Results and table data are scrolled continuously over a cursor the server keeps open, so this is not a ceiling on what a query returns: a larger number means fewer, bigger trips, a smaller one means more, smaller ones. The next window is asked for before the viewport reaches the edge of the one you are reading."
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

      <!-- The project file is committed into somebody's repository, so it is
           written on an explicit Save and never as you type. The bar stays out of
           the way until something has actually changed — a permanently visible
           Save button teaches people to press it without reading. -->
      {#if editingProject && picusSettingsStore.projectDirty}
        <div class="ps-save">
          <span class="ps-save-text">
            Unsaved changes to <code>.arbor/picus/project.toml</code> — this file is committed, so
            nothing is written until you say so.
          </span>
          <Button
            variant="secondary"
            size="xs"
            onclick={() => void picusSettingsStore.loadProject(picusProjectStore.root)}
          >
            Discard
          </Button>
          <Button
            variant="primary"
            size="xs"
            disabled={picusSettingsStore.projectSaving || !picusProjectStore.attached}
            onclick={() => void saveProject()}
          >
            {#snippet iconStart()}<Save size={12} />{/snippet}
            Save and re-check
          </Button>
        </div>
      {/if}
    {/snippet}
  </SettingsShell>
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }

  /* Sticks to the bottom of the scrolling page: the rule list is long, and a Save
     button you have to scroll back up to find is a Save button people forget. */
  .ps-save {
    position: sticky;
    bottom: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 12px;
    padding: 10px 12px;
    background: var(--bg-elevated);
    border: 1px solid var(--warning);
    border-radius: var(--radius-md);
  }
  .ps-save-text { flex: 1; font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-secondary); }
  .ps-save-text code { font-family: var(--font-code); font-size: var(--font-size-xs); }

  /* A second-level heading inside a page that already has one. */
  .ps-sub { margin-top: 18px; }
  .ps-sub h3 { font-size: var(--font-size-md); font-weight: 600; margin-bottom: 3px; }

  .ps-family {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 2px 0 8px;
    border-bottom: 1px solid var(--border-subtle);
    margin-bottom: 4px;
  }
  .ps-family-name {
    font-size: var(--font-size-2xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .ps-family-blurb { font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-muted); }

  /* Table picker + Detect on one line: the button acts on the field beside it. */
  /* Table picker + Detect on one line. `flex: 1` because this sits inside a
     `wideControl` row, which is itself a flex box: every link in the chain has to
     grow, or the width stops at whichever one forgets to. */
  .ps-row { display: flex; align-items: center; gap: 8px; flex: 1 1 auto; min-width: 0; }

  /* The emitted-guard preview — the reason the four fields above are legible. */
  .ps-preview {
    margin: 6px 0 0;
    padding: 9px 11px;
    background: var(--bg-input);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    line-height: 1.6;
    color: var(--text-secondary);
    white-space: pre;
    overflow-x: auto;
  }
</style>
