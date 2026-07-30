<script lang="ts">
  /**
   * Generate DML — the bridge between the two halves of Picus.
   *
   * One datum described once, written into every folder that expects it. The
   * page reads top to bottom as the flow itself: where the values come from →
   * where they are going and under what rules → what the SQL looks like → what
   * will change on disk.
   *
   * Nothing is written from this page directly: the final action asks for
   * confirmation and names exactly which files it will touch.
   */
  import {
    FormInput, Files, Code2, Filter, GitCompare, Check, Play, Download, Plus, RefreshCw,
  } from 'lucide-svelte';
  import Card from '$lib/components/shared/ui/Card.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import DmlValueGrid from '../generate/DmlValueGrid.svelte';
  import WhereBuilder from '../generate/WhereBuilder.svelte';
  import DmlKeyPicker from '../generate/DmlKeyPicker.svelte';
  import PasteSqlPanel from '../generate/PasteSqlPanel.svelte';
  import CsvImportGrid from '../generate/CsvImportGrid.svelte';
  import TargetEditor from '../generate/TargetEditor.svelte';
  import DestinationSets from '../generate/DestinationSets.svelte';
  import SqlPreview from '../generate/SqlPreview.svelte';
  import PatchDiffCard from '../generate/PatchDiffCard.svelte';
  import { untrack } from 'svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { schemaStore } from '$lib/stores/picus/schema.svelte';
  import { DML_OPERATION_LABELS, type DmlOperation, type DmlSource } from '$lib/types/picus';

  interface Props {
    /** The shell owns the confirmation dialog — this page only asks for it. */
    onWrite: () => void;
  }

  let { onWrite }: Props = $props();

  /**
   * Tables you can write into — from the **database and the repository both**.
   *
   * Sourcing this from the live schema alone made the whole page dead on a machine
   * with no database connected: an empty dropdown, nothing to pick, nothing to
   * generate. In a product whose subject is scripts on disk, that is the common
   * case, not the edge one — the repository's own index knows every table its
   * scripts install, and that is a perfectly good answer to "which table".
   *
   * Tables only: a view has columns but is not writable, so it is never a
   * destination for generated DML. Objects the repository merely reads are left
   * out for the same reason — nothing here installs them, so writing rows into
   * them from these scripts is not a thing this project does.
   */
  const tableOptions = $derived.by(() => {
    const live = new Set(schemaStore.tables.map((t) => t.name.toUpperCase()));
    const options = schemaStore.tables.map((t) => ({ value: t.name, label: t.name }));
    for (const obj of picusProjectStore.inventory) {
      if (obj.kind !== 'table' || obj.external) continue;
      if (live.has(obj.name.toUpperCase())) continue;
      // Marked, because the difference matters downstream: a table only the
      // scripts know has no column types, so the form cannot offer fields for it
      // and the values are taken as written.
      options.push({ value: obj.name, label: `${obj.name}  ·  from the scripts` });
    }
    return options;
  });

  const operationTabs: TabItem[] = (
    ['insert', 'upsert', 'update', 'delete'] as DmlOperation[]
  ).map((op) => ({ id: op, label: DML_OPERATION_LABELS[op] }));

  const sourceTabs: TabItem[] = [
    { id: 'form', label: 'Form' },
    { id: 'paste', label: 'Paste SQL' },
    { id: 'csv', label: 'CSV' },
  ];

  const blockTargets = $derived(dmlStore.enabledTargets.filter((t) => t.wrap === 'block').length);

  /**
   * Keep the patch section describing the current payload.
   *
   * The section only exists after an explicit **Generate**, and any edit takes it
   * away again — so this reads disk exactly when somebody has just said "build
   * this", never on a keystroke. Which is what the section was missing: it used to
   * offer the diff once and then show that first answer through every later
   * generation.
   *
   * `ensurePreview` is self-guarding, so watching the payload key is enough: a
   * landed preview does not re-trigger it, and neither does a failed one. The
   * bottom dock runs the same effect for the same reason; both being no-ops for
   * each other is the point of the guard. `untrack` keeps the call itself out of
   * the dependencies — it writes the very state this effect would otherwise read.
   */
  $effect(() => {
    if (!dmlStore.generated || !dmlStore.enabledTargets.length) return;
    if (!picusProjectStore.attached) return;
    void dmlStore.previewKey;
    untrack(() => void dmlStore.ensurePreview());
  });
</script>

<div class="gv">
  <!-- The tab is called "Generate DML"; the heading said it a second time and
       then explained the feature underneath, for as long as the tab stayed open.
       That is documentation — it goes in the Docs, where it can be read once and
       not re-read every session — and the sixty pixels it held go to the cards
       that actually do the work. -->

  <!-- Source and Destinations answer "what" and "where", and you move between
       them constantly — so they sit side by side while there is room, and stack
       when there isn't. The measure is the PANEL's width (a container query),
       not the screen's: the same window is wide with the sidebar closed and
       narrow with it open. -->
  <div class="gv-cols">
  <!-- ── 1. Source ─────────────────────────────────────────────────────────── -->
  <Card variant="flat" padding="none">
    {#snippet header()}
      <span class="gv-card-title"><FormInput size={13} /> Source</span>
    {/snippet}
    {#snippet actions()}
      <Tabs
        items={sourceTabs}
        value={dmlStore.source}
        variant="pill"
        size="sm"
        ariaLabel="Generation source"
        onSelect={(id) => dmlStore.setSource(id as DmlSource)}
      />
    {/snippet}

    <div class="gv-body">
      <div class="gv-row">
        <span class="gv-label">Table</span>
        {#if dmlStore.tableIsFromSource}
          <!-- The pasted statements named it. Shown, not asked for: re-picking
               what the source already says is a question with one right answer. -->
          <span class="gv-table-fixed" use:tooltip={'Read from the pasted statements'}>
            {dmlStore.table}
          </span>
        {:else}
          <Select
            value={dmlStore.table}
            options={tableOptions}
            searchable={tableOptions.length > 8}
            searchPlaceholder="Filter tables"
            placeholder="Choose a table"
            emptyMessage="No table is known yet — connect a database, or attach the script repository."
            onchange={(v) => dmlStore.setTable(v)}
          />
        {/if}
        <span class="gv-label gv-label-gap">Operation</span>
        <Tabs
          items={operationTabs}
          value={dmlStore.operation}
          variant="pill"
          size="sm"
          ariaLabel="Operation"
          onSelect={(id) => dmlStore.setOperation(id as DmlOperation)}
        />
      </div>

      {#if dmlStore.columnsFromSource}
        <!-- Said once, plainly: everything downstream behaves slightly differently
             and the user should know why before they read the generated SQL. -->
        <p class="gv-inferred">
          No connected database knows <code>{dmlStore.table}</code>, so the columns and their
          types come from
          {#if dmlStore.columnsFromScripts}
            what the repository's own statements write into it — which is why there are
            {dmlStore.columns.length} of them and not the table's full set.
          {:else}
            the statements themselves.
          {/if}
          A value written bare is emitted bare, a quoted one is re-quoted. Length limits and
          <code>NOT NULL</code> are not checked, and there is no primary key to fall back on.
        </p>
      {:else if dmlStore.table && !dmlStore.columns.length}
        <!-- The state that used to be an empty form with no explanation: a table
             picked, and nothing to type into it. -->
        <Alert
          variant="warning"
          compact
          title={`Nothing is known about ${dmlStore.table}'s columns`}
          text="No connected database has it, and nothing in this repository writes to it — so there is no column list to read. Connect a database that has the table, or paste an INSERT and let it say what the columns are."
        />
      {/if}

      {#if dmlStore.source !== 'form' && dmlStore.columns.length}
        <!-- The comparison key: the WHERE of an update or delete, and the conflict
             target of an upsert. Form mode picks it in the value grid; the imported
             sources have no grid to pick it in, and with no live schema there is no
             primary key to fall back on either — so it is asked for here. -->
        <DmlKeyPicker />
      {/if}

      {#if dmlStore.usesWhere}
        <!-- Only where there is a WHERE to build. An INSERT has none, and a card
             offering to filter one would be a control that does nothing. -->
        <section class="gv-where" aria-label="Which rows this touches">
          <h3 class="gv-where-head">
            <Filter size={12} />
            Which rows
            <span class="gv-where-note">
              {#if dmlStore.hasWhere}
                this replaces the comparison key
              {:else}
                empty — matches the comparison key
              {/if}
            </span>
          </h3>
          <WhereBuilder
            node={dmlStore.whereClause}
            columns={dmlStore.columns}
            onChange={(next) => dmlStore.setWhereClause(next)}
          />
        </section>
      {/if}

      {#if dmlStore.source === 'form'}
        <DmlValueGrid />
        <div class="gv-form-actions">
          <Button variant="ghost" size="sm" onclick={() => dmlStore.clearForm()}>Clear</Button>
          <Button
            variant="primary"
            size="sm"
            disabled={!dmlStore.canGenerate}
            tooltip={{
              // A disabled button that says why beats one that only greys out —
              // especially here, where "the values have not been checked yet" is a
              // legitimate and very short-lived reason.
              content: dmlStore.generateBlockedReason ?? 'Build the SQL for every enabled destination',
              shortcut: 'Ctrl+G',
            }}
            onclick={() => dmlStore.markGenerated()}
          >
            {#snippet iconStart()}<Play size={13} />{/snippet}
            Generate
          </Button>
        </div>
      {:else if dmlStore.source === 'paste'}
        <PasteSqlPanel />
      {:else}
        <CsvImportGrid />
      {/if}
    </div>
  </Card>

  <!-- ── 2. Destinations ───────────────────────────────────────────────────── -->
  <Card variant="flat" padding="none">
    {#snippet header()}
      <span class="gv-card-title"><Files size={13} /> Destinations</span>
    {/snippet}
    {#snippet actions()}
      <!-- Amber only when destinations EXIST and none of them is armed, which is
           a generation that would write nowhere. `0 of 0` is not that: it is the
           card's opening state, before anybody has added anything, and dressing
           it as a warning makes the first thing a new repository shows an alarm
           about something the user has not done yet. -->
      <Badge
        variant="tone"
        tone={dmlStore.enabledTargets.length
          ? 'accent'
          : dmlStore.targets.length
            ? 'warning'
            : 'neutral'}
        size="sm"
        label={`${dmlStore.enabledTargets.length} of ${dmlStore.targets.length} enabled`}
      />
      <!-- The same six places, every generation. Saved as folders rather than as
           paths, so a set still works next release. -->
      <DestinationSets />
      <Button
        variant="ghost"
        size="xs"
        tooltip={'Add another file to write this generation into'}
        ariaLabel="Add a destination"
        onclick={() => picusUiStore.openAddDestination()}
      >
        {#snippet iconStart()}<Plus size={13} />{/snippet}
        Add
      </Button>
    {/snippet}

    {#if !dmlStore.targets.length}
      <div class="gv-body">
        <Alert
          variant="info"
          compact
          text="No destination yet. Add the files this change has to be written into — one per folder that expects it."
        />
      </div>
    {:else if !dmlStore.enabledTargets.length}
      <div class="gv-body">
        <Alert
          variant="warning"
          compact
          text="Every destination is switched off. Nothing would be written — arm at least one."
        />
      </div>
    {/if}
    <TargetEditor />
  </Card>
  </div>

  <!-- ── 3. Preview ────────────────────────────────────────────────────────── -->
  <Card variant="flat" padding="none">
    {#snippet header()}
      <span class="gv-card-title"><Code2 size={13} /> Generated SQL</span>
    {/snippet}
    <SqlPreview />
  </Card>

  <!-- ── 4. What changes on disk ───────────────────────────────────────────── -->
  <!-- The preview is what the backend would actually write, read from disk. It is
       built by the effect above whenever this section exists — which is only after
       an explicit Generate — and **Recompute** is the manual override for the one
       thing an effect cannot notice: a destination file changing underneath.
       Both of those replace an earlier arrangement where the only way to build a
       preview was a button inside the "no files yet" branch. The moment one
       landed, that button was gone, and the first payload's cards then sat here
       through every later generation looking current. A diff that is silently
       stale and cannot be recomputed is worse than no diff. -->
  {#if dmlStore.generated && dmlStore.enabledTargets.length}
    {@const stale = dmlStore.previewFiles.length > 0 && !dmlStore.previewFresh}
    <section class="gv-patches" aria-label="Changes to the scripts">
      <h2 class="gv-section">
        <GitCompare size={13} /> Changes to the scripts
        {#if dmlStore.previewing}<Spinner size={11} />{/if}
        <span class="gv-section-spacer"></span>
        {#if dmlStore.previewFiles.length || dmlStore.previewError}
          <Button
            variant={stale ? 'primary' : 'ghost'}
            size="xs"
            disabled={dmlStore.previewing || !picusProjectStore.attached}
            tooltip={stale
              ? 'What is shown describes an earlier version of this generation — read the files again'
              : 'Read the destination files again'}
            onclick={() => void dmlStore.rebuildPreview()}
          >
            {#snippet iconStart()}<RefreshCw size={12} />{/snippet}
            {stale ? 'Recompute' : 'Refresh'}
          </Button>
        {/if}
      </h2>

      {#if stale}
        <!-- Said before the cards rather than after, because the cards are what
             gets reviewed and approving a stale one is the mistake this whole
             two-step exists to prevent. The write refuses on it too — but being
             refused at the last step is a worse way to find out. -->
        <Alert
          variant="warning"
          compact
          title="These are not the current changes"
          text="Something moved since they were computed — a value, a destination, one of its rules. Recompute before writing; the write refuses a diff nobody reviewed."
        />
      {/if}

      {#if dmlStore.previewError}
        <Alert variant="error" title="The patch could not be computed" text={dmlStore.previewError} />
      {:else if dmlStore.previewing}
        <Card variant="elevated">
          <div class="gv-preview-ask">
            <Spinner size={13} />
            <span>Reading the destination files…</span>
          </div>
        </Card>
      {:else if !dmlStore.previewFiles.length}
        <!-- Reached when there is nothing to read from: no repository attached.
             The effect above builds the preview in every other case, so an empty
             state with a button would be a button nobody ever needs to press. -->
        <Card variant="elevated">
          <div class="gv-preview-ask">
            <span>
              The exact bytes each destination would receive are read from the repository —
              and this connection has none attached, so there is nothing to compare against.
            </span>
            <Button
              variant="secondary"
              size="sm"
              disabled={dmlStore.previewing || !picusProjectStore.attached}
              tooltip={picusProjectStore.attached
                ? undefined
                : { content: 'This connection has no script repository attached' }}
              onclick={() => void dmlStore.ensurePreview()}
            >
              {#snippet iconStart()}<GitCompare size={13} />{/snippet}
              Show what would change
            </Button>
          </div>
        </Card>
      {:else}
        <!-- Dimmed rather than hidden while stale: the previous diff is still the
             best available answer, and the banner above says what it is. Same
             treatment the generated SQL gets, for the same reason. -->
        <div class="gv-patch-list" class:gv-dim={stale}>
          {#each dmlStore.previewFiles as file (file.path)}
            <PatchDiffCard {file} target={dmlStore.targets.find((t) => t.file === file.path) ?? null} />
          {/each}
        </div>
      {/if}
    </section>
  {/if}

  <!-- ── 5. Write ──────────────────────────────────────────────────────────── -->
  <Card variant="elevated">
    <div class="gv-write">
      <div class="gv-write-text">
        <strong>{dmlStore.applied ? 'Written to disk' : 'Ready to write'}</strong>
        <span>
          {#if dmlStore.applied}
            Files rewritten in their original encoding, line endings untouched. Originals are
            in <code>.arbor/backup</code>.
          {:else}
            {dmlStore.enabledTargets.length} file{dmlStore.enabledTargets.length === 1 ? '' : 's'},
            {blockTargets} with a procedural block. You review the exact bytes first; the write
            then refuses if any of those files moved in the meantime.
          {/if}
        </span>
      </div>
      <Button
        variant="secondary"
        size="sm"
        onclick={() => toastStore.show('Exporting a .sql bundle arrives with the backend milestone.', 'info')}
      >
        {#snippet iconStart()}<Download size={13} />{/snippet}
        Export .sql
      </Button>
      <Button
        variant="primary"
        size="sm"
        disabled={!dmlStore.generated || dmlStore.applied}
        tooltip={{ content: 'Write the generated SQL into the scripts', shortcut: 'Ctrl+Shift+W' }}
        onclick={onWrite}
      >
        {#snippet iconStart()}<Check size={13} />{/snippet}
        {dmlStore.applied ? 'Written' : 'Write to the scripts'}
      </Button>
    </div>
  </Card>
</div>


<style>
  /* Document-flow view: it owns its vertical scroll. `flex-shrink: 0` on the
     children is load-bearing — in a column flex box they would otherwise be
     compressed to fit the viewport instead of overflowing into the scroll. */
  .gv {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px 20px 60px;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    /* The two-column break is measured against THIS box, not the viewport: the
       same window is wide with the sidebar closed and narrow with it open. */
    container-type: inline-size;
  }
  .gv > :global(*) {
    flex-shrink: 0;
    width: 100%;
    max-width: 1600px;
  }

  /* Source ("what") beside Destinations ("where") — the two you move between
     while composing. The generated SQL gets the full width below, because a
     procedural block is wide and reading it half-width is miserable. */
  .gv-cols {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 12px;
    align-items: start;
  }
  @container (max-width: 1080px) {
    .gv-cols { grid-template-columns: minmax(0, 1fr); }
  }

  .gv-card-title {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: var(--font-size-sm);
    font-weight: 600;
  }
  .gv-card-title :global(svg) { color: var(--text-muted); }

  .gv-body { padding: 12px; }

  .gv-where { display: flex; flex-direction: column; gap: 4px; }
  .gv-where-head {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: var(--font-size-xs);
    font-weight: 600;
    color: var(--text-secondary);
  }
  .gv-where-note { font-weight: 400; color: var(--text-muted); }

  .gv-row {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    margin-bottom: 12px;
  }
  .gv-label { font-size: var(--font-size-xs); color: var(--text-muted); }
  .gv-label-gap { margin-left: 6px; }

  /* The table the source dictates: a value, not a control. Styled as an input
     that cannot be typed in rather than as text, so the row still reads as a form
     row and nobody hunts for the dropdown that used to be here. */
  .gv-table-fixed {
    padding: 3px 9px;
    background: var(--bg-hover);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--text-primary);
  }

  .gv-inferred {
    margin-bottom: 12px;
    font-size: var(--font-size-xs);
    line-height: 1.5;
    color: var(--text-muted);
    max-width: 88ch;
  }
  .gv-inferred code { font-family: var(--font-code); font-size: var(--font-size-xs); }

  .gv-form-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }

  .gv-patches { display: flex; flex-direction: column; gap: 8px; }
  .gv-preview-ask { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .gv-preview-ask span {
    flex: 1;
    min-width: 240px;
    font-size: var(--font-size-xs);
    line-height: 1.5;
    color: var(--text-muted);
  }
  .gv-section {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--font-size-2xs);
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
    padding-top: 4px;
  }
  .gv-section-spacer { flex: 1; }

  .gv-patch-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    transition: opacity var(--transition-fast);
  }
  /* Showing a diff for a payload that has moved on. Dimmed rather than blanked:
     the previous answer is still the best one available, and the banner above
     says exactly what it is. */
  .gv-dim { opacity: 0.5; }

  .gv-write { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .gv-write-text { flex: 1; min-width: 240px; display: flex; flex-direction: column; gap: 3px; }
  .gv-write-text strong { font-size: var(--font-size-sm); }
  .gv-write-text span { font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-muted); }
  .gv-write-text code { font-family: var(--font-code); font-size: var(--font-size-xs); }
</style>
