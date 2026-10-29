<script lang="ts">
  /**
   * Which samply the Rust profiler uses: the one the user picked, the one Arbor downloaded, or the
   * one on PATH — in that order. Shows what resolved (and its version), lets the user pick an
   * executable, and downloads / updates / removes Arbor's own copy.
   */
  import { Download, FolderOpen, Trash2 } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { bennuSamplyStore as samply } from '$lib/stores/bennu/samply.svelte';
  import SamplyInstallModal from './SamplyInstallModal.svelte';

  let picking = $state(false);
  let installOpen = $state(false);
  let path = $state('');
  let editing = false;

  const status = $derived(samply.status);
  const resolved = $derived(status?.resolved ?? null);
  const sourceLabel = $derived(
    resolved ? ({ user: 'your path', managed: 'downloaded by Arbor', path: 'on PATH' } as const)[resolved.source] : '',
  );

  $effect(() => { void samply.loadStatus(); });
  // Mirror the saved path into the field, unless the user is typing in it.
  $effect(() => { if (!editing) path = status?.user_path ?? ''; });

  async function commit(value: string) {
    editing = false;
    if (value.trim() === (status?.user_path ?? '')) return;
    await samply.setPath(value);
  }

  function openInstall() {
    installOpen = true;
    void samply.checkLatest();
  }
</script>

<div class="tool">
  <div class="row">
    {#if !status}
      <Spinner size={12} />
    {:else if resolved}
      <Badge variant="tone" tone="success" label={`samply ${resolved.version ?? ''}`.trim()} />
      <span class="muted" title={resolved.path}>{sourceLabel} · {resolved.path}</span>
    {:else}
      <Badge variant="tone" tone="warning" label="samply not found" />
    {/if}
  </div>

  <div class="row">
    <div class="path">
      <Input
        size="sm"
        value={path}
        oninput={(v) => { editing = true; path = v; }}
        onchange={(v) => void commit(v)}
        onblur={() => void commit(path)}
        placeholder="Path to a samply executable (optional)"
        ariaLabel="Path to samply"
        error={samply.pathError}
        clearable
        onclear={() => void commit('')}
      />
    </div>
    <Button size="sm" variant="outline" onclick={() => (picking = true)}>
      {#snippet iconStart()}<FolderOpen size={11} />{/snippet}
      Choose…
    </Button>
    {#if status?.supported !== false}
      <Button size="sm" variant="outline" disabled={samply.installing} onclick={openInstall}>
        {#snippet iconStart()}<Download size={11} />{/snippet}
        {status?.managed_version ? 'Check for update' : 'Download samply'}
      </Button>
    {/if}
    {#if status?.managed_version}
      <Button size="sm" variant="ghost" onclick={() => void samply.remove()} title={`Remove the downloaded samply ${status.managed_version}`}>
        {#snippet iconStart()}<Trash2 size={11} />{/snippet}
        Remove
      </Button>
    {/if}
  </div>

  {#if samply.validating}
    <span class="muted"><Spinner size={11} /> Checking the file…</span>
  {:else if samply.pathVersion}
    <span class="muted">Detected samply {samply.pathVersion}.</span>
  {/if}
  {#if status?.problem}
    <Alert variant="warning" compact>{status.problem}</Alert>
  {/if}
  {#if status && !status.supported}
    <Alert variant="info" compact>There is no samply download for this OS / architecture. Build it with <code>cargo install samply</code> and choose it above.</Alert>
  {/if}
  <span class="muted">
    Build the program with debug info (<code>[profile.release] debug = true</code>) so frames have names and lines.
  </span>
</div>

{#if picking}
  <FileExplorerModal
    mode="file"
    title="Choose the samply executable"
    onConfirm={(file) => { picking = false; path = file; void commit(file); }}
    onCancel={() => (picking = false)}
    onClose={() => (picking = false)}
  />
{/if}

{#if installOpen}
  <SamplyInstallModal onClose={() => (installOpen = false)} />
{/if}

<style>
  .tool { display: flex; flex-direction: column; gap: 6px; }
  .row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .path { flex: 1; min-width: 240px; max-width: 520px; }
  .muted { color: var(--text-muted); font-size: var(--font-size-xs); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
</style>
