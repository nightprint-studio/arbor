<script lang="ts">
  /**
   * BennuSafeDeleteModal — confirm a deletion, or show who still needs it.
   *
   * Two states over one shell, because they are two answers to one question and splitting them into
   * two dialogs would make the second look like an error rather than a result:
   *
   * * **Safe** — nothing uses it. A short confirmation naming what goes, and the file that goes with
   *   it when the declaration is a top-level type.
   * * **Not safe** — either a reason it can never be deleted, or the list of uses that have to go
   *   first. The list is the whole point of the feature: "it is used" is not an answer, and the next
   *   question is always *where*. Rows jump, so the list is also the way to go and fix them.
   *
   * The rows are the same `UsageHit` find-usages returns and are rendered the same way, so a use
   * means one thing in this editor whichever list you are reading.
   */
  import { Trash2, FileCode2, ShieldAlert } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import type { SafeDeletePlan } from '$lib/ipc/bennu/refactor';
  import type { UsageHit } from '$lib/ipc/bennu/nav';

  interface Props {
    /** The plan, or `null` while it is still being worked out. */
    plan: SafeDeletePlan | null;
    loading: boolean;
    onConfirm: () => void;
    onClose: () => void;
    /** Open a use site — the list is how you go and fix what is in the way. */
    onOpenUsage: (hit: UsageHit) => void;
  }

  let { plan, loading, onConfirm, onClose, onOpenUsage }: Props = $props();

  const safe = $derived(!!plan?.safe);
  const usages = $derived(plan?.usages ?? []);
  let confirmBtn = $state<HTMLButtonElement | undefined>(undefined);

  /** Ctrl/Cmd+Enter confirms, and only where confirming is a thing that can happen. */
  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && safe) {
      e.preventDefault();
      onConfirm();
    }
  }

  // The destructive button takes focus so the whole dialog is Enter-or-Escape from the keyboard —
  // the same `bind:element` `ConfirmModal` uses, not a second way of reaching a Button's node.
  $effect(() => {
    if (safe && confirmBtn) confirmBtn.focus();
  });

  function baseName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  /** Uses grouped by file, so four hits in one file read as one place to go and not as four. */
  const grouped = $derived.by(() => {
    const byFile = new Map<string, UsageHit[]>();
    for (const u of usages) {
      const list = byFile.get(u.file);
      if (list) list.push(u);
      else byFile.set(u.file, [u]);
    }
    return [...byFile.entries()];
  });
</script>

<Modal width="620px" height="480px" {onClose}>
  <ModalHeader {onClose}>
    <Trash2 size={14} />
    <span class="modal-title">Safe delete</span>
  </ModalHeader>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="sd" onkeydown={onKeydown}>
    {#if loading}
      <div class="sd-center"><Spinner /> <span>Looking for uses…</span></div>
    {:else if !plan}
      <Alert variant="info">
        There is nothing here to delete — put the caret on a method, a field or a type this project
        declares. If the project has just opened, its index may still be building.
      </Alert>
    {:else if plan.blocked}
      <!-- A reason that no amount of editing the call sites would fix. -->
      <Alert variant="warning">
        <strong>{plan.label}</strong> cannot be deleted safely: {plan.blocked}
      </Alert>
    {:else if safe}
      <div class="sd-safe">
        <ShieldAlert size={18} />
        <div>
          <p class="sd-lead">Nothing in this project uses <strong>{plan.label}</strong>.</p>
          <p class="sd-detail">
            It will be removed from <code>{baseName(plan.file)}</code> along with its documentation
            comment.
            {#if plan.file_delete}
              The file goes with it — a top-level type is its file.
            {/if}
          </p>
        </div>
      </div>
    {:else}
      <p class="sd-lead">
        <strong>{plan.label}</strong> is used in {usages.length}
        {usages.length === 1 ? 'place' : 'places'}. Those have to go first — pick one to open it.
      </p>
      <div class="sd-list">
        {#each grouped as [file, hits] (file)}
          <div class="sd-file"><FileCode2 size={11} /> {baseName(file)}</div>
          {#each hits as hit (hit.start)}
            <button type="button" class="sd-usage" onclick={() => onOpenUsage(hit)}>
              <span class="sd-pos">{hit.line}:{hit.col}</span>
              <span class="sd-preview">{hit.preview}</span>
            </button>
          {/each}
        {/each}
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter>
      <Button variant="ghost" onclick={onClose}>{safe ? 'Cancel' : 'Close'}</Button>
      {#if safe}
        <Button
          variant="danger"
          bind:element={confirmBtn}
          tooltip={{ content: 'Remove it', shortcut: 'Ctrl+Enter' }}
          onclick={onConfirm}
        >Delete</Button>
      {/if}
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .sd {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
    min-height: 0;
    height: 100%;
  }
  .sd-center {
    display: flex;
    align-items: center;
    gap: 10px;
    justify-content: center;
    padding: 32px 0;
    color: var(--text-muted);
  }
  .sd-safe { display: flex; gap: 12px; align-items: flex-start; }
  .sd-safe :global(svg) { color: var(--accent-warning); flex: none; margin-top: 2px; }
  .sd-lead { margin: 0; }
  .sd-detail { margin: 4px 0 0; color: var(--text-muted); font-size: var(--font-size-sm); }
  .sd-detail code { font-family: var(--font-code); font-size: var(--font-size-xs); }

  /* The list scrolls on its own so the footer stays reachable however many uses there are. */
  .sd-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }
  .sd-file {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 10px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    background: var(--bg-subtle);
    position: sticky;
    top: 0;
  }
  .sd-file :global(svg) { color: var(--text-disabled); }
  .sd-usage {
    display: flex;
    align-items: baseline;
    gap: 10px;
    width: 100%;
    padding: 5px 10px 5px 22px;
    background: none;
    border: none;
    text-align: left;
    cursor: pointer;
    color: inherit;
  }
  .sd-usage:hover, .sd-usage:focus-visible { background: var(--bg-hover); }
  .sd-pos {
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    flex: none;
    min-width: 40px;
    font-variant-numeric: tabular-nums;
  }
  .sd-preview {
    flex: 1;
    min-width: 0;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
