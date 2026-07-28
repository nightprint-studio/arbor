<script lang="ts">
  /**
   * Generate panel — the sidebar face of the DML generator: pick a source, see
   * at a glance which destinations are armed.
   *
   * The rules of each destination are edited in the centre area, not here: this
   * panel answers "where is this going?", the tab answers "in what form?".
   */
  import { FormInput, ClipboardPaste, FileSpreadsheet, Files, Check, Plus } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import PicusRoleChip from '../PicusRoleChip.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import type { DmlSource } from '$lib/types/picus';

  const SOURCES: { id: DmlSource; label: string; hint: string; icon: any }[] = [
    { id: 'form', label: 'Guided form', hint: 'Columns read from the schema', icon: FormInput },
    { id: 'paste', label: 'Existing INSERTs', hint: 'Paste them and they get re-read', icon: ClipboardPaste },
    { id: 'csv', label: 'CSV file', hint: 'One row per record', icon: FileSpreadsheet },
  ];

  function pick(source: DmlSource) {
    dmlStore.setSource(source);
    picusTabsStore.openGenerate();
  }
</script>

<PanelShell title="Generate DML">
  {#snippet icon()}<FormInput size={13} />{/snippet}

  {#snippet actions()}
    <Button
      variant="icon"
      size="xs"
      title="Start a new generation"
      ariaLabel="Start a new generation"
      onclick={() => { dmlStore.reset(); picusTabsStore.openGenerate(); }}
    >
      {#snippet iconStart()}<Plus size={13} />{/snippet}
    </Button>
  {/snippet}

  <SidebarSection label="Source" expanded>
    {#each SOURCES as s (s.id)}
      <SidebarItem selected={dmlStore.source === s.id} onclick={() => pick(s.id)}>
        {#snippet icon()}
          {@const Icon = s.icon}
          <Icon size={13} />
        {/snippet}
        <span class="gp-label">{s.label}</span>
        {#snippet subtitle()}{s.hint}{/snippet}
      </SidebarItem>
    {/each}
  </SidebarSection>

  <SidebarSection
    label="Destinations"
    badge={`${dmlStore.enabledTargets.length}/${dmlStore.targets.length}`}
    badgeTitle="Enabled destinations"
    expanded
  >
    {#each dmlStore.targets as target (target.id)}
      <SidebarItem
        selected={dmlStore.expandedTargetId === target.id}
        onclick={() => { dmlStore.expandTarget(target.id); picusTabsStore.openGenerate(); }}
      >
        {#snippet icon()}
          <!-- The checkbox arms the destination; the row itself opens its rules. -->
          <button
            type="button"
            class="gp-check"
            class:gp-on={target.enabled}
            aria-pressed={target.enabled}
            aria-label={`${target.enabled ? 'Disable' : 'Enable'} ${target.file}`}
            onclick={(e) => { e.stopPropagation(); dmlStore.toggleTarget(target.id); }}
          >
            {#if target.enabled}<Check size={10} />{/if}
          </button>
        {/snippet}
        <span class="gp-chips">
          <PicusDialectChip engine={target.dialect} terse />
          <PicusRoleChip role={target.role} terse />
          {#if target.wrap === 'block'}
            <Badge variant="tone" tone="accent" size="sm" label="block" />
          {/if}
          {#if target.guards.version}
            <Badge
              variant="tone"
              tone="warning"
              size="sm"
              label={`${target.guards.version.from || '?'} → ${target.guards.version.to || '?'}`}
            />
          {/if}
        </span>
        {#snippet subtitle()}<span class="gp-path">{target.file}</span>{/snippet}
      </SidebarItem>
    {/each}

    <div class="gp-add">
      <Button variant="ghost" size="xs" block onclick={() => picusUiStore.openAddDestination()}>
        {#snippet iconStart()}<Files size={13} />{/snippet}
        Add a destination…
      </Button>
    </div>
  </SidebarSection>

  <p class="gp-hint">
    Each destination decides for itself whether it needs a procedural block and under
    which conditions it may run. Those rules live in the Generate tab.
  </p>
</PanelShell>

<style>
  .gp-label { overflow: hidden; text-overflow: ellipsis; }

  .gp-chips { display: inline-flex; align-items: center; gap: 4px; flex-wrap: wrap; }

  .gp-path {
    font-family: var(--font-code);
    font-size: 10px;
    overflow: hidden;
    text-overflow: ellipsis;
    display: block;
  }

  /* Arming checkbox — the row click opens the rules, the box toggles the target. */
  .gp-check {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 15px;
    height: 15px;
    padding: 0;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: transparent;
    cursor: pointer;
    flex-shrink: 0;
  }
  .gp-check:hover { border-color: var(--border-focus); }
  .gp-check.gp-on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--text-on-accent);
  }

  .gp-add { padding: 4px 8px 2px; }

  .gp-hint {
    padding: 10px 12px;
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-muted);
  }
</style>
