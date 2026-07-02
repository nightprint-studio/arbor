<script lang="ts">
  /**
   * Bennu settings — the two-pane settings surface (shared `SettingsShell`), the
   * same look as Arbor's and merula's settings.
   *
   * Phase 0 has no persisted `[bennu]` config yet, so this exposes the resolved,
   * read-only facts about the open project — the JDK, the detected capabilities and
   * their evidence, and the source encoding. When typed config lands (rule #11),
   * new panels slot into `groups` and persist through a `configStore`; this is the
   * seam, not a stand-in for localStorage.
   */
  import { Settings, Coffee, Boxes, FileType } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import SettingsShell, { type SettingsNavGroup } from '$lib/components/shared/ui/SettingsShell.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const groups: SettingsNavGroup[] = [
    { label: 'Project', items: [
      { id: 'jdk',          label: 'JDK',          icon: Coffee },
      { id: 'capabilities', label: 'Capabilities', icon: Boxes },
      { id: 'encoding',     label: 'Encoding',     icon: FileType },
    ] },
  ];
  let active = $state('jdk');

  const project = $derived(projectStore.project);
  const jdk = $derived(project?.jdk ?? null);
  const caps = $derived(projectStore.capabilities);

  // Enabled capability field names (skip the `hits` array).
  const enabledCaps = $derived.by(() => {
    if (!caps) return [] as string[];
    return Object.entries(caps)
      .filter(([k, v]) => k !== 'hits' && v === true)
      .map(([k]) => k);
  });
  const hits = $derived(caps?.hits ?? []);
  function capLabel(field: string): string {
    return field.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
  }
</script>

<Modal {onClose} width="840px" height="540px" padBody={false} ariaLabel="Bennu Settings">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Settings size={14} />
      <span class="modal-title">Settings</span>
    </ModalHeader>
  {/snippet}

  <SettingsShell {groups} bind:active>
    {#snippet content()}
      {#if !project}
        <EmptyState message="Open a project to see its resolved settings." />
      {:else if active === 'jdk'}
        <div class="section-header">
          <h2>JDK</h2>
          <p>The Java language level Bennu resolves the classpath against.</p>
        </div>
        <div class="card">
          <div class="card-section-title"><Coffee size={12} /> Resolved JDK</div>
          {#if jdk}
            <div class="bs-kv"><span class="bs-k">Version</span><span class="bs-v">{jdk.version}</span></div>
            <div class="bs-kv"><span class="bs-k">Source</span><span class="bs-v"><code>{jdk.source}</code></span></div>
          {:else}
            <p class="bs-none">Not inferred — set a compiler source/target in the pom, or an override.</p>
          {/if}
        </div>
      {:else if active === 'capabilities'}
        <div class="section-header">
          <h2>Capabilities</h2>
          <p>The domain frameworks detected in this project, with the evidence that activated each.</p>
        </div>
        <div class="card">
          <div class="card-section-title"><Boxes size={12} /> Detected ({enabledCaps.length})</div>
          {#if enabledCaps.length === 0}
            <p class="bs-none">No domain capabilities detected.</p>
          {:else}
            <div class="bs-chips">
              {#each enabledCaps as c (c)}
                <Badge variant="tone" tone="accent" label={capLabel(c)} />
              {/each}
            </div>
          {/if}
        </div>
        {#if hits.length}
          <div class="card">
            <div class="card-section-title"><Boxes size={12} /> Evidence</div>
            {#each hits as h, i (i)}
              <div class="bs-hit">
                <span class="bs-tier bs-tier-{h.tier.toLowerCase()}">{h.tier}</span>
                <span class="bs-hit-body">
                  <span class="bs-hit-cap">{capLabel(h.capability)}</span>
                  <span class="bs-hit-detail">{h.detail}</span>
                </span>
              </div>
            {/each}
          </div>
        {/if}
      {:else if active === 'encoding'}
        <div class="section-header">
          <h2>Encoding</h2>
          <p>How file source is decoded. Legacy projects often declare <code>Cp1252</code> in the pom.</p>
        </div>
        <div class="card">
          <div class="card-section-title"><FileType size={12} /> Active file</div>
          {#if projectStore.activeFilePath}
            <div class="bs-kv"><span class="bs-k">File</span><span class="bs-v">{projectStore.activeFilePath.split(/[\\/]/).pop()}</span></div>
            <div class="bs-kv"><span class="bs-k">Decoded as</span><span class="bs-v">{projectStore.activeEncoding}</span></div>
          {:else}
            <p class="bs-none">Open a file to see the encoding it was decoded from.</p>
          {/if}
        </div>
      {/if}
    {/snippet}
  </SettingsShell>
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .bs-kv { display: flex; align-items: center; gap: 10px; padding: 6px 2px; font-size: 12.5px; }
  .bs-k { width: 110px; flex-shrink: 0; color: var(--text-muted); }
  .bs-v { color: var(--text-primary); }
  .bs-none { font-size: 12px; color: var(--text-muted); font-style: italic; padding: 4px 2px; }
  .bs-chips { display: flex; flex-wrap: wrap; gap: 6px; padding: 4px 0; }
  .bs-hit { display: flex; align-items: flex-start; gap: 10px; padding: 7px 2px; border-top: 1px solid var(--border-subtle); }
  .bs-hit:first-of-type { border-top: none; }
  .bs-tier {
    flex-shrink: 0; width: 18px; height: 18px; border-radius: var(--radius-sm);
    display: flex; align-items: center; justify-content: center;
    font-size: 10px; font-weight: 700;
  }
  .bs-tier-a { color: var(--success); background: color-mix(in srgb, var(--success) 18%, transparent); }
  .bs-tier-b { color: var(--info);    background: color-mix(in srgb, var(--info) 18%, transparent); }
  .bs-tier-c { color: var(--warning); background: color-mix(in srgb, var(--warning) 18%, transparent); }
  .bs-hit-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .bs-hit-cap { font-size: 12.5px; font-weight: 600; color: var(--text-primary); }
  .bs-hit-detail { font-size: 11.5px; color: var(--text-muted); }
</style>
