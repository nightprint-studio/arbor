<script lang="ts">
  /**
   * AssistantWorktreeModal — everything an isolated copy changed, read as one diff before it touches the project.
   *
   * The files are a list on the left (each with what it adds and removes, and whether it is new, changed or
   * deleted); the one picked is drawn on the right. Applying is all or nothing: if the project changed a file
   * the copy also changed, nothing is written and every such file is listed here with the reason.
   * ↑ / ↓ move between files, Ctrl+Enter applies, Esc closes.
   */
  import { FileCode2, FilePlus2, FileMinus2, Check, TriangleAlert, GitBranch } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import TextDiff from '$lib/components/shared/ui/TextDiff.svelte';
  import DiffModeToggle from '$lib/components/shared/ui/DiffModeToggle.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { bennuWorktreeStore } from '$lib/stores/bennu/assistant-worktree.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';

  const wt = bennuWorktreeStore;
  const diff = $derived(wt.diff);
  const files = $derived(diff?.files ?? []);

  let picked = $state(0);
  let split = $state(false);

  const file = $derived(files[Math.min(picked, Math.max(files.length - 1, 0))] ?? null);
  const added = $derived(files.reduce((n, f) => n + (f.added ?? 0), 0));
  const removed = $derived(files.reduce((n, f) => n + (f.removed ?? 0), 0));
  // What the apply refused over: fresh from the last attempt, or found when the diff was made.
  const blocked = $derived(wt.clashes.length > 0 ? wt.clashes : (diff?.clashes ?? []));
  const canApply = $derived(!wt.diffLoading && !wt.applying && files.length > 0 && blocked.length === 0);

  $effect(() => { void wt.diff; picked = 0; });

  function close() { wt.closeDiff(); }

  async function apply() {
    if (!canApply) return;
    const id = wt.info?.conversationId;
    if (await wt.apply() && id) await bennuAssistantStore.reload(id);
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); void apply(); }
    else if (e.key === 'ArrowDown') { e.preventDefault(); picked = Math.min(picked + 1, files.length - 1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); picked = Math.max(picked - 1, 0); }
  }

  function split2(path: string): { dir: string; name: string } {
    const i = path.lastIndexOf('/');
    return i < 0 ? { dir: '', name: path } : { dir: path.slice(0, i + 1), name: path.slice(i + 1) };
  }
</script>

{#if wt.diffOpen}
  <Modal onClose={close} width="980px" height="640px" padBody={false} bodyBorder ariaLabel="Review the isolated copy">
    {#snippet header()}
      <ModalHeader onClose={close}>
        <GitBranch size={14} />
        <span class="modal-title">Apply isolated copy</span>
        <span class="sub">{files.length} file{files.length === 1 ? '' : 's'}</span>
        <span class="delta"><b class="add">+{added}</b> <b class="del">−{removed}</b></span>
      </ModalHeader>
    {/snippet}

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="review" onkeydown={onKeydown}>
      {#if wt.diffLoading}
        <div class="center"><Spinner size={18} /></div>
      {:else if files.length === 0}
        <div class="center"><EmptyState message="No changes" description="The copy is the same as the project." /></div>
      {:else}
        <nav class="files" aria-label="Changed files">
          {#each files as f, i (f.path)}
            {@const p = split2(f.path)}
            <SidebarItem selected={i === picked} onclick={() => (picked = i)}>
              {#snippet icon()}
                {#if f.kind === 'added'}<FilePlus2 size={13} />{:else if f.kind === 'deleted'}<FileMinus2 size={13} />{:else}<FileCode2 size={13} />{/if}
              {/snippet}
              <span class="name">{p.name}</span>
              {#snippet subtitle()}<span class="dir">{f.renamedFrom ? `from ${f.renamedFrom}` : (p.dir || './')}</span>{/snippet}
              {#snippet badges()}
                {#if blocked.some((c) => c.rel === f.path)}<span class="stale-mark" title="The project changed this file meanwhile"><TriangleAlert size={11} /></span>{/if}
                {#if !f.binary}<span class="delta"><span class="add">+{f.added ?? 0}</span> <span class="del">−{f.removed ?? 0}</span></span>{/if}
              {/snippet}
            </SidebarItem>
          {/each}
        </nav>

        <section class="diff">
          {#if blocked.length > 0}
            <div class="blocked">
              <Alert variant="warning" compact title="Nothing was applied — the project moved on">
                <ul>
                  {#each blocked as c (c.rel)}<li><code>{c.rel}</code> — {c.reason}</li>{/each}
                </ul>
              </Alert>
            </div>
          {/if}
          {#if wt.held}
            <div class="blocked"><Alert variant="warning" compact title="A file is being edited elsewhere" text={wt.held} /></div>
          {/if}
          {#if file}
            <header class="bar">
              <span class="path">{file.path}</span>
              <DiffModeToggle {split} onToggle={() => (split = !split)} />
            </header>
            <div class="body">
              {#if file.hunks}
                <TextDiff hunks={file.hunks} mode={split ? 'split' : 'unified'} ariaLabel={`Changes to ${file.path}`} />
              {:else}
                <div class="center"><EmptyState message={file.binary ? 'Not a text file' : 'Too large to show'} description={file.kind === 'deleted' ? 'It will be deleted.' : 'It will be replaced.'} /></div>
              {/if}
            </div>
          {/if}
        </section>
      {/if}
    </div>

    {#snippet footer()}
      <ModalFooter align="between">
        <span class="note">All or nothing: if any file cannot be applied, none is.</span>
        <div class="actions">
          <Button variant="ghost" size="sm" onclick={close}>Close</Button>
          <Button variant="primary" size="sm" disabled={!canApply} loading={wt.applying} tooltip={{ content: 'Apply every change to the project', shortcut: 'Ctrl+Enter' }} onclick={apply}>
            <Check size={13} /> Apply to project
          </Button>
        </div>
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .sub { font-size: var(--font-size-xs); color: var(--text-muted); }

  .review { display: grid; grid-template-columns: 250px minmax(0, 1fr); height: 100%; min-height: 0; }
  .center { grid-column: 1 / -1; display: flex; align-items: center; justify-content: center; height: 100%; }
  .files {
    display: flex; flex-direction: column; gap: 1px;
    padding: 8px 6px; overflow-y: auto;
    border-right: 1px solid var(--border-subtle);
    background: var(--bg-base);
  }
  .name { font-family: var(--font-code); font-size: var(--font-size-xs); }
  .dir { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); }

  .diff { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .blocked { padding: 8px 10px 0; }
  .blocked ul { margin: 4px 0 0; padding-left: 16px; font-size: var(--font-size-xs); }
  .bar { display: flex; align-items: center; gap: 10px; padding: 7px 12px; border-bottom: 1px solid var(--border-subtle); }
  .path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary); }
  .body { flex: 1; min-height: 0; }

  .delta { flex-shrink: 0; font-family: var(--font-code); font-size: var(--font-size-2xs); font-variant-numeric: tabular-nums; }
  .add { color: var(--success); font-weight: 600; }
  .del { color: var(--error); font-weight: 600; }
  .stale-mark { display: inline-flex; color: var(--warning); }
  .note { font-size: var(--font-size-xs); color: var(--text-muted); }
  .actions { display: flex; align-items: center; gap: 8px; }
</style>
