<script lang="ts">
  /**
   * ExportAllModal — bounce every `.merula` in the project to audio in one go.
   *
   * The single-file export asks for a cycle count because a `Pattern` has no
   * intrinsic length. Asking that once per file across a whole project would be
   * unusable, so each file declares its own length in its front-matter
   * (`meta { cycles = "88" }`) and the backend's export plan resolves it — falling
   * back to the arrangement period, then to 1 for one-shots. This dialog shows what
   * it resolved (and where from), lets you uncheck what you don't want, and picks
   * ONE format for the whole batch.
   *
   * Keyboard-first: Esc cancels, Ctrl/Cmd+Enter exports, every row is a real
   * checkbox so the list is tabbable. Files that failed to parse are listed with
   * their error and unchecked by default — one broken file must not block the rest.
   */
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import RenderFormatFields from './RenderFormatFields.svelte';
  import { FolderOpen, PackageOpen } from 'lucide-svelte';

  import {
    merulaExportPlan, merulaExportAll,
    type MerulaExportPlanEntry,
  } from '$lib/ipc/merula/merula';
  import { projectStore } from '../stores/project.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';

  interface Props { open: boolean; onclose: () => void; }
  let { open, onclose }: Props = $props();

  const FORMAT_OPTIONS = [
    { value: 'ogg', label: 'OGG Vorbis (compressed)' },
    { value: 'wav', label: 'WAV (lossless)' },
  ];

  let plan       = $state<MerulaExportPlanEntry[]>([]);
  let excluded   = $state<Set<string>>(new Set());
  let loading    = $state(false);
  let exporting  = $state(false);
  let loadError  = $state<string | null>(null);

  let format     = $state('ogg');
  let sampleRate = $state(48000);
  let bitDepth   = $state('int24');
  let tail       = $state(2.0);
  let outDir     = $state('');
  let pickerOpen = $state(false);

  const projectDir = $derived(projectStore.project?.path ?? '');
  const selected   = $derived(plan.filter((e) => !excluded.has(e.path)));
  const canExport  = $derived(selected.length > 0 && outDir.trim().length > 0 && !exporting);

  // Load the plan when the dialog opens; default the output folder to the project
  // and pre-exclude anything that failed to parse.
  $effect(() => {
    if (!open || !projectDir) return;
    loading = true;
    loadError = null;
    void merulaExportPlan(projectDir)
      .then((entries) => {
        plan = entries;
        excluded = new Set(entries.filter((e) => e.error).map((e) => e.path));
        if (!outDir) outDir = projectDir;
      })
      .catch((e) => { loadError = e instanceof Error ? e.message : String(e); })
      .finally(() => { loading = false; });
  });

  function toggle(path: string) {
    const next = new Set(excluded);
    if (next.has(path)) next.delete(path); else next.add(path);
    excluded = next;
  }
  function selectAll()  { excluded = new Set(); }
  function selectNone() { excluded = new Set(plan.map((e) => e.path)); }

  async function doExport() {
    if (!canExport) return;
    exporting = true;
    try {
      await merulaExportAll(
        projectDir,
        selected.map((e) => ({ path: e.path, stem: e.stem, cycles: e.cycles })),
        outDir,
        { format, sample_rate: sampleRate, bit_depth: bitDepth, tail_max_secs: tail },
      );
      toastStore.show(`Exporting ${selected.length} files — progress in Downloads & Exports`, 'success');
      onclose();
    } catch (e) {
      toastStore.show(`Export failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
    } finally {
      exporting = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') { e.preventDefault(); void doExport(); }
  }
</script>

<Modal {open} {onclose} width="720px" height="640px" onkeydown={onKeydown}>
  <ModalHeader title="Export all" subtitle="Bounce every script in this project to audio" icon={PackageOpen} />

  <div class="body">
    {#if loadError}
      <Alert kind="error">{loadError}</Alert>
    {:else if loading}
      <div class="loading"><Spinner size={18} /> <span>Reading project…</span></div>
    {:else if plan.length === 0}
      <EmptyState title="No scripts found" description="This project has no .merula files to export." />
    {:else}
      <div class="listhead">
        <span class="count">{selected.length} of {plan.length} selected</span>
        <div class="bulk">
          <Button size="xs" variant="ghost" onclick={selectAll}>All</Button>
          <Button size="xs" variant="ghost" onclick={selectNone}>None</Button>
        </div>
      </div>

      <ul class="files">
        {#each plan as e (e.path)}
          <li class:err={!!e.error}>
            <label>
              <input type="checkbox" checked={!excluded.has(e.path)} onchange={() => toggle(e.path)} />
              <span class="name">
                {e.title ?? e.stem}
                <span class="rel">{e.rel}</span>
              </span>
              <span class="cycles" title={`render length resolved from: ${e.cycles_from}`}>
                {e.cycles} {e.cycles === 1 ? 'cycle' : 'cycles'}
                {#if e.cycles_from !== 'meta'}<em>({e.cycles_from})</em>{/if}
              </span>
            </label>
            {#if e.error}<p class="why">{e.error}</p>{/if}
          </li>
        {/each}
      </ul>

      <div class="opts">
        <FormRow label="Format" description="Applies to every exported file.">
          <Select value={format} options={FORMAT_OPTIONS} onchange={(v) => (format = String(v))} />
        </FormRow>
        <RenderFormatFields
          {sampleRate} {bitDepth} {tail}
          onSampleRate={(v) => (sampleRate = v)}
          onBitDepth={(v) => (bitDepth = v)}
          onTail={(v) => (tail = v)}
        />
        <FormRow label="Output folder">
          <div class="dir">
            <span class="path" title={outDir}>{outDir || 'Choose a folder…'}</span>
            <Button size="sm" variant="secondary" onclick={() => (pickerOpen = true)}>
              <FolderOpen size={14} /> Browse
            </Button>
          </div>
        </FormRow>
      </div>
    {/if}
  </div>

  <ModalFooter>
    <Button variant="ghost" onclick={onclose}>Cancel</Button>
    <Button variant="primary" disabled={!canExport} onclick={doExport}>
      {#if exporting}<Spinner size={14} />{/if}
      Export {selected.length} file{selected.length === 1 ? '' : 's'}
    </Button>
  </ModalFooter>
</Modal>

{#if pickerOpen}
  <FileExplorerModal
    mode="folder"
    title="Export all — pick an output folder"
    initialPath={outDir || projectDir}
    onConfirm={(p) => { outDir = Array.isArray(p) ? p[0] : p; pickerOpen = false; }}
    onCancel={() => (pickerOpen = false)}
    onClose={() => (pickerOpen = false)}
  />
{/if}

<style>
  .body { display: flex; flex-direction: column; gap: 12px; min-height: 0; flex: 1; overflow: hidden; }
  .loading { display: flex; align-items: center; gap: 8px; color: var(--text-secondary); padding: 24px 0; }

  .listhead { display: flex; align-items: center; justify-content: space-between; }
  .count { font-size: 12px; color: var(--text-secondary); }
  .bulk { display: flex; gap: 4px; }

  .files {
    list-style: none; margin: 0; padding: 0;
    overflow-y: auto; flex: 1; min-height: 120px;
    border: 1px solid var(--border); border-radius: var(--radius-md);
  }
  .files li { border-bottom: 1px solid var(--border-subtle); }
  .files li:last-child { border-bottom: none; }
  .files li.err { background: var(--danger-subtle); }

  label { display: flex; align-items: center; gap: 10px; padding: 6px 10px; cursor: pointer; }
  label:hover { background: var(--bg-hover); }
  .name { flex: 1; display: flex; flex-direction: column; font-size: 13px; min-width: 0; }
  .rel { font-size: 11px; color: var(--text-disabled); font-family: var(--font-code); }
  .cycles { font-size: 11px; color: var(--text-secondary); white-space: nowrap; }
  .cycles em { color: var(--text-disabled); font-style: normal; }
  .why { margin: 0 10px 6px 34px; font-size: 11px; color: var(--danger); }

  .opts { display: flex; flex-direction: column; gap: 2px; flex: none; }
  .dir { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .path {
    flex: 1; font-size: 12px; font-family: var(--font-code); color: var(--text-secondary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
</style>
