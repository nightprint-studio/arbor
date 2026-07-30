<script lang="ts">
  /**
   * BennuMojibakeScanModal — whole-project mojibake report.
   *
   * Scans every text file for UTF-8-decoded-as-Cp1252 corruption (`Ã©` → `é`, `â€™` → `'`) and lists
   * the affected files, expandable to the individual hits (click a hit to jump to it). Optionally
   * pushes the hits into the Problems panel as warnings.
   */
  import { SvelteSet } from 'svelte/reactivity';
  import { ShieldAlert, FileWarning, ChevronRight, ChevronDown, RotateCw, Check } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuDiagnosticsStore } from '$lib/stores/bennu/diagnostics.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { mojibakeProject, type ProjectMojibakeResult } from '$lib/ipc/bennu/mojibake';
  import type { FileDiagnostics } from '$lib/types/bennu';

  let { onClose }: { onClose: () => void } = $props();

  let result = $state<ProjectMojibakeResult | null>(null);
  let loading = $state(false);
  let added = $state(false);
  const expanded = new SvelteSet<string>();

  const root = $derived(projectStore.project?.root ?? null);

  async function scan() {
    if (!root || loading) return;
    loading = true;
    added = false;
    expanded.clear();
    try {
      result = await mojibakeProject(root);
    } catch (e) {
      toastStore.show(`Mojibake scan failed: ${e}`, 'error');
    } finally {
      loading = false;
    }
  }

  // Auto-run once when the modal opens on a project.
  $effect(() => {
    if (root && !result && !loading) void scan();
  });

  function baseName(p: string): string {
    return p.split('/').pop() ?? p;
  }
  function toggle(file: string) {
    if (expanded.has(file)) expanded.delete(file);
    else expanded.add(file);
  }
  function openHit(file: string, start: number) {
    void projectStore.openFile(file).then(() => bennuUiStore.requestGotoOffset(start));
    onClose();
  }
  function addToProblems() {
    if (!result) return;
    const list: FileDiagnostics[] = result.files.map((f) => ({
      file: f.file,
      diagnostics: f.hits.map((h) => ({
        message: `Mojibake: “${h.bad}” → “${h.fix}”`,
        severity: 'warning' as const,
        code: 'mojibake',
        start: h.start,
        end: h.end,
      })),
    }));
    bennuDiagnosticsStore.setMojibakeDiagnostics(list);
    added = true;
    const n = result.total_hits;
    toastStore.show(`Added ${n} mojibake warning${n === 1 ? '' : 's'} to Problems`, 'success');
  }
</script>

<Modal {onClose} width="640px" height="560px" padBody={false} ariaLabel="Project mojibake scan">
  {#snippet header()}
    <ModalHeader {onClose}>
      <ShieldAlert size={14} />
      <span class="modal-title">Project mojibake</span>
      <button
        class="hdr-rescan"
        type="button"
        use:tooltip={'Re-scan the whole project'}
        aria-label="Rescan"
        disabled={loading || !root}
        onclick={scan}
      >
        <RotateCw size={12} class={loading ? 'spin' : ''} />
        {loading ? 'Scanning…' : 'Rescan'}
      </button>
    </ModalHeader>
  {/snippet}

  <div class="body">
    {#if !root}
      <EmptyState message="Open a project to scan it for mojibake." />
    {:else if loading && !result}
      <div class="loading"><Spinner /> <span>Scanning project for mojibake…</span></div>
    {:else if result}
      <div class="summary">
        {#if result.total_hits === 0}
          <span class="clean"><Check size={15} /> No mojibake found across {result.total_files_scanned} files.</span>
        {:else}
          <span>
            <strong>{result.total_hits}</strong> occurrence{result.total_hits === 1 ? '' : 's'}
            in <strong>{result.files_with_hits}</strong> file{result.files_with_hits === 1 ? '' : 's'}
          </span>
          <span class="muted">· {result.total_files_scanned} files scanned</span>
        {/if}
      </div>

      {#if result.total_hits > 0}
        <div class="list">
          {#each result.files as f (f.file)}
            <div class="file">
              <button class="file-row" type="button" onclick={() => toggle(f.file)}>
                {#if expanded.has(f.file)}<ChevronDown size={13} />{:else}<ChevronRight size={13} />{/if}
                <FileWarning size={13} class="fw" />
                <span class="fname">{baseName(f.file)}</span>
                <span class="fpath" title={f.file}>{f.file}</span>
                <span class="count">{f.hits.length}</span>
              </button>
              {#if expanded.has(f.file)}
                <div class="hits">
                  {#each f.hits as h, i (i)}
                    <button class="hit" type="button" onclick={() => openHit(f.file, h.start)}>
                      <span class="bad">{h.bad}</span>
                      <span class="arrow">→</span>
                      <span class="fix">{h.fix}</span>
                      <span class="off">@{h.start}</span>
                    </button>
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="foot-hint">
        {#if result && result.total_hits > 0}Click a hit to jump to it.{/if}
      </span>
      <div class="foot-actions">
        {#if result && result.total_hits > 0}
          <Button variant="secondary" onclick={addToProblems} disabled={added}>
            {added ? 'Added to Problems' : `Add ${result.total_hits} to Problems`}
          </Button>
        {/if}
        <Button onclick={onClose}>Close</Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .modal-title { font-weight: 600; font-size: var(--font-size-md); margin-right: auto; }
  .hdr-rescan {
    display: flex; align-items: center; gap: 5px;
    height: 22px; padding: 0 9px;
    background: transparent; border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-secondary); font-size: var(--font-size-xs); cursor: pointer;
  }
  .hdr-rescan:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .hdr-rescan:disabled { opacity: 0.6; cursor: default; }

  .body { display: flex; flex-direction: column; height: 100%; overflow: hidden; }
  .loading {
    flex: 1; display: flex; align-items: center; justify-content: center; gap: 10px;
    color: var(--text-muted); font-size: var(--font-size-sm);
  }
  .summary {
    display: flex; align-items: center; gap: 6px; flex-wrap: wrap;
    padding: 10px 14px; border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-sm); color: var(--text-secondary);
  }
  .summary strong { color: var(--text-primary); }
  .summary .muted { color: var(--text-muted); }
  .summary .clean { display: flex; align-items: center; gap: 6px; color: var(--success); }

  .list { flex: 1; overflow-y: auto; padding: 4px 0; }
  .file { border-bottom: 1px solid var(--border-subtle); }
  .file-row {
    display: flex; align-items: center; gap: 6px; width: 100%;
    padding: 6px 12px; background: transparent; border: none; cursor: pointer;
    color: var(--text-primary); font-size: var(--font-size-sm); text-align: left;
  }
  .file-row:hover { background: var(--bg-hover); }
  .file-row :global(.fw) { color: var(--warning); flex-shrink: 0; }
  .fname { font-weight: 500; flex-shrink: 0; }
  .fpath { color: var(--text-muted); font-size: var(--font-size-xs); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }
  .count {
    flex-shrink: 0; min-width: 20px; text-align: center;
    padding: 1px 6px; border-radius: 9px;
    background: color-mix(in srgb, var(--warning) 18%, transparent);
    color: var(--warning); font-size: var(--font-size-xs); font-weight: 600;
  }
  .hits { display: flex; flex-direction: column; padding: 2px 0 6px 30px; }
  .hit {
    display: flex; align-items: center; gap: 8px;
    padding: 3px 12px; background: transparent; border: none; cursor: pointer;
    color: var(--text-secondary); font-size: var(--font-size-sm); text-align: left;
    font-family: var(--font-mono, monospace);
  }
  .hit:hover { background: var(--bg-hover); color: var(--text-primary); }
  .hit .bad { color: var(--danger); }
  .hit .arrow { color: var(--text-muted); }
  .hit .fix { color: var(--success); }
  .hit .off { margin-left: auto; color: var(--text-muted); font-size: var(--font-size-xs); }

  .foot-hint { color: var(--text-muted); font-size: var(--font-size-xs); }
  .foot-actions { display: flex; align-items: center; gap: 8px; }
</style>
