<script lang="ts">
  /**
   * Generate DML — the bridge between the two halves of Picus.
   *
   * One datum described once, written into every branch that expects it. The
   * page reads top to bottom as the flow itself: where the values come from →
   * where they are going and under what rules → what the SQL looks like → what
   * will change on disk.
   *
   * Nothing is written from this page directly: the final action asks for
   * confirmation and names exactly which files it will touch.
   */
  import { FormInput, Files, Code2, GitCompare, Check, Play, Download, Plus } from 'lucide-svelte';
  import Card from '$lib/components/shared/ui/Card.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import DmlValueGrid from '../generate/DmlValueGrid.svelte';
  import PasteSqlPanel from '../generate/PasteSqlPanel.svelte';
  import CsvImportGrid from '../generate/CsvImportGrid.svelte';
  import TargetEditor from '../generate/TargetEditor.svelte';
  import SqlPreview from '../generate/SqlPreview.svelte';
  import PatchDiffCard from '../generate/PatchDiffCard.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { schemaStore } from '$lib/stores/picus/schema.svelte';
  import { DML_OPERATION_LABELS, type DmlOperation, type DmlSource } from '$lib/types/picus';

  interface Props {
    /** The shell owns the confirmation dialog — this page only asks for it. */
    onWrite: () => void;
  }

  let { onWrite }: Props = $props();

  // Tables only: a view has columns but is not writable, so it is never a
  // destination for generated DML.
  const tableOptions = $derived(
    schemaStore.tables.map((t) => ({ value: t.name, label: t.name })),
  );

  const operationTabs: TabItem[] = (
    ['insert', 'upsert', 'update', 'delete'] as DmlOperation[]
  ).map((op) => ({ id: op, label: DML_OPERATION_LABELS[op] }));

  const sourceTabs: TabItem[] = [
    { id: 'form', label: 'Form' },
    { id: 'paste', label: 'Paste SQL' },
    { id: 'csv', label: 'CSV' },
  ];

  const blockTargets = $derived(dmlStore.enabledTargets.filter((t) => t.wrap === 'block').length);
</script>

<div class="gv">
  <header class="gv-head">
    <h1>Generate DML</h1>
    <p>
      One datum described once, written into every branch that expects it. Each destination
      decides for itself whether it needs a procedural block and under which conditions it
      may run.
    </p>
  </header>

  <!-- Source and Destinations answer "what" and "where", and you move between
       them constantly — so they sit side by side while there is room, and stack
       when there isn't. The measure is the PANEL's width (a container query),
       not the screen's: the same window is wide with the sidebar closed and
       narrow with it open. -->
  <div class="gv-cols">
  <!-- ── 1. Source ─────────────────────────────────────────────────────────── -->
  <Card padding="none">
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
        <Select
          value={dmlStore.table}
          options={tableOptions}
          onchange={(v) => dmlStore.setTable(v)}
        />
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

      {#if dmlStore.source === 'form'}
        <DmlValueGrid />
        <div class="gv-form-actions">
          <Button variant="ghost" size="sm" onclick={() => dmlStore.clearForm()}>Clear</Button>
          <Button
            variant="primary"
            size="sm"
            disabled={!dmlStore.canGenerate}
            tooltip={{ content: 'Build the SQL for every enabled destination', shortcut: 'Ctrl+G' }}
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
  <Card padding="none">
    {#snippet header()}
      <span class="gv-card-title"><Files size={13} /> Destinations</span>
    {/snippet}
    {#snippet actions()}
      <Badge
        variant="tone"
        tone={dmlStore.enabledTargets.length ? 'accent' : 'warning'}
        size="sm"
        label={`${dmlStore.enabledTargets.length} of ${dmlStore.targets.length} enabled`}
      />
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
          text="No destination yet. Add the files this change has to be written into — one per branch that expects it."
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
  <Card padding="none">
    {#snippet header()}
      <span class="gv-card-title"><Code2 size={13} /> Generated SQL</span>
    {/snippet}
    <SqlPreview />
  </Card>

  <!-- ── 4. What changes on disk ───────────────────────────────────────────── -->
  {#if dmlStore.generated && dmlStore.enabledTargets.length}
    <section class="gv-patches" aria-label="Changes to the scripts">
      <h2 class="gv-section"><GitCompare size={13} /> Changes to the scripts</h2>
      {#each dmlStore.enabledTargets as target (target.id)}
        <PatchDiffCard {target} sql={dmlStore.sqlFor(target)} />
      {/each}
    </section>
  {/if}

  <!-- ── 5. Write ──────────────────────────────────────────────────────────── -->
  <Card variant="subtle">
    <div class="gv-write">
      <div class="gv-write-text">
        <strong>{dmlStore.applied ? 'Written to disk' : 'Ready to write'}</strong>
        <span>
          {#if dmlStore.applied}
            Files rewritten in their original encoding, line endings untouched. Originals are
            in <code>.arbor/backup</code>.
          {:else}
            {dmlStore.enabledTargets.length} file{dmlStore.enabledTargets.length === 1 ? '' : 's'},
            {blockTargets} with a procedural block. Encoding and line endings stay as they are,
            and every file is backed up first.
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

  .gv-head h1 { font-size: 16px; font-weight: 600; margin-bottom: 3px; }
  .gv-head p {
    font-size: 12px;
    line-height: 1.55;
    color: var(--text-muted);
    max-width: 76ch;
  }

  .gv-card-title {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    font-weight: 600;
  }
  .gv-card-title :global(svg) { color: var(--text-muted); }

  .gv-body { padding: 12px; }

  .gv-row {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    margin-bottom: 12px;
  }
  .gv-label { font-size: 11.5px; color: var(--text-muted); }
  .gv-label-gap { margin-left: 6px; }

  .gv-form-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }

  .gv-patches { display: flex; flex-direction: column; gap: 8px; }
  .gv-section {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
    padding-top: 4px;
  }

  .gv-write { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .gv-write-text { flex: 1; min-width: 240px; display: flex; flex-direction: column; gap: 3px; }
  .gv-write-text strong { font-size: 12px; }
  .gv-write-text span { font-size: 11.5px; line-height: 1.5; color: var(--text-muted); }
  .gv-write-text code { font-family: var(--font-code); font-size: 11px; }
</style>
