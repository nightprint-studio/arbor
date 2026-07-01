<script lang="ts">
  /**
   * Transcription-models manager — a Settings section listing the on-demand ONNX
   * models (basic-pitch, Demucs) with their install status, size, and a
   * Download / Cancel / Delete control. Drives `modelsStore`; progress also shows
   * in the shared Downloads & Exports overlay. Once a model is installed, audio
   * import uses it automatically (no extra toggle).
   */
  import { Download, Trash2, Check, HardDrive } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import ProgressBar from '$lib/components/shared/ui/ProgressBar.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { modelsStore } from '../stores/models.svelte';
  import type { MerulaModelStatus } from '$lib/ipc/merula/merula';

  let confirmDelete = $state<MerulaModelStatus | null>(null);
  let deleteError   = $state<string | null>(null);

  function formatBytes(n: number): string {
    if (n <= 0) return '—';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let v = n, i = 0;
    while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
    return `${v < 10 && i > 0 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
  }

  function askDelete(model: MerulaModelStatus) { deleteError = null; confirmDelete = model; }
  async function doDelete() {
    if (!confirmDelete) return;
    try {
      await modelsStore.remove(confirmDelete.id);
      confirmDelete = null;
    } catch (e) {
      deleteError = e instanceof Error ? e.message : String(e);
    }
  }
  const deleting = $derived(confirmDelete ? modelsStore.deletingOf(confirmDelete.id) : false);
</script>

<div class="models">
  {#each modelsStore.models as model (model.id)}
    {@const pct = modelsStore.progressOf(model.id)}
    <div class="model" class:installed={model.installed}>
      <div class="model-head">
        <span class="model-name">{model.name}</span>
        {#if model.installed}
          <Badge variant="tone" tone="success" size="sm"><Check size={9} /> installed</Badge>
        {/if}
      </div>
      {#if model.description}
        <p class="model-desc">{model.description}</p>
      {/if}

      {#if model.installed}
        <div class="model-foot">
          <span class="model-meta"><HardDrive size={11} /> {formatBytes(model.size_bytes)} on disk</span>
          <button class="model-del" use:tooltip={'Delete model'}
                  aria-label={`Delete ${model.name}`} onclick={() => askDelete(model)}>
            <Trash2 size={13} />
          </button>
        </div>
      {:else if modelsStore.downloadingOf(model.id)}
        <div class="model-dl">
          <div class="model-dl-head">
            <span class="model-phase">Downloading…</span>
            {#if pct != null}<span class="model-pct">{Math.round(pct)}%</span>{/if}
          </div>
          <ProgressBar pct={pct ?? undefined} indeterminate={pct == null}
                       ariaLabel={`${model.name} download progress`} />
          <Button size="xs" variant="ghost" block onclick={() => modelsStore.cancel(model.id)}>Cancel</Button>
        </div>
      {:else}
        <div class="model-foot">
          <span class="model-meta" use:tooltip={'Approximate download size'}>
            <Download size={11} /> ~{formatBytes(model.approx_bytes)}
          </span>
          <Button size="sm" variant="secondary" onclick={() => modelsStore.download(model.id)}>
            {#snippet iconStart()}<Download size={13} />{/snippet}
            Download
          </Button>
        </div>
      {/if}
    </div>
  {/each}
</div>

{#if confirmDelete}
  <ConfirmModal
    variant="danger"
    title="Delete model"
    message={`Delete “${confirmDelete.name}” from disk?`}
    detail={deleteError ?? 'You can re-download it any time from here.'}
    confirmLabel="Delete"
    busy={deleting}
    onConfirm={doDelete}
    onCancel={() => { if (!deleting) confirmDelete = null; }}
  />
{/if}

<style>
  .models { display: flex; flex-direction: column; gap: 8px; }
  .model {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 10px 12px;
    background: var(--bg-base);
  }
  .model.installed { border-color: color-mix(in srgb, var(--success) 30%, var(--border-subtle)); }
  .model-head { display: flex; align-items: center; gap: 8px; }
  .model-name { font-size: 12.5px; font-weight: 600; color: var(--text-primary); }
  .model-desc { margin: 4px 0 8px; font-size: 11.5px; line-height: 1.4; color: var(--text-secondary); }
  .model-foot { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .model-meta {
    display: inline-flex; align-items: center; gap: 5px;
    font-size: 11px; color: var(--text-muted);
  }
  .model-del {
    display: inline-flex; align-items: center; justify-content: center;
    width: 26px; height: 26px; border: none; border-radius: var(--radius-sm);
    background: transparent; color: var(--text-muted); cursor: pointer;
  }
  .model-del:hover { background: var(--bg-hover); color: var(--danger); }
  .model-dl { display: flex; flex-direction: column; gap: 6px; }
  .model-dl-head { display: flex; align-items: center; justify-content: space-between; }
  .model-phase { font-size: 11px; color: var(--text-secondary); }
  .model-pct { font-size: 11px; color: var(--text-muted); font-variant-numeric: tabular-nums; }
</style>
