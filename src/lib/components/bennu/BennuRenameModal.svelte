<script lang="ts">
  /**
   * BennuRenameModal — the Shift+F6 rename refactor with a per-file preview.
   *
   * Opens off the caret context the editor captured (`bennuRefactorStore.renameReq`:
   * file · buffer · byte offset · initial name). Typing a new name re-plans the
   * rename via `bennu_rename_plan` (debounced), showing every edit grouped by file
   * with its reason and an "inferred" flag for heuristic sites (an overloaded
   * method's calls). On confirm it hands the whole set to `projectStore.applyEdits` — the
   * one write path that checks each edit against the text it was computed against and
   * treats the set as a single bulk edit — then carries out the file move the rename
   * implies, if any. The backend never writes buffers, so CodeMirror stays the source of
   * truth and undo works per file.
   *
   * Keyboard-first: the field auto-focuses (via <Modal>), Ctrl/Cmd+Enter renames,
   * Esc cancels.
   */
  import { PenLine, FileCode2, AlertTriangle } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import ValueChange from '$lib/components/shared/ui/ValueChange.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { bennuRefactorStore } from '$lib/stores/bennu/refactor.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { renamePlan, type RenamePreview } from '$lib/ipc/bennu/nav';

  let { onClose }: { onClose: () => void } = $props();

  const req = $derived(bennuRefactorStore.renameReq);

  // Opens on the suggestion when the caller computed one (the naming-convention fix), otherwise on
  // the current name — which the user then edits. Either way the field starts selected, so typing
  // over it costs nothing.
  let newName = $state(
    bennuRefactorStore.renameReq?.suggestedName ?? bennuRefactorStore.renameReq?.initialName ?? '',
  );
  let preview = $state<RenamePreview | null>(null);
  let planning = $state(false);
  let applying = $state(false);

  const JAVA_IDENT = /^[A-Za-z_$][A-Za-z0-9_$]*$/;
  const valid = $derived(
    !!req && JAVA_IDENT.test(newName.trim()) && newName.trim() !== req.initialName,
  );
  // `blocked` is a refusal from the engine, not advice: applying anyway produces code that does not
  // compile, so it disables the button rather than adding a banner to click past.
  const canRename = $derived(
    valid && !!preview && preview.total_edits > 0 && !preview.blocked && !applying,
  );

  function baseName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  // Re-plan (debounced) whenever the new name changes to a valid, different identifier.
  let planTimer: ReturnType<typeof setTimeout> | undefined;
  let planSeq = 0;
  $effect(() => {
    const name = newName.trim();
    const r = req;
    if (planTimer) clearTimeout(planTimer);
    if (!r || !JAVA_IDENT.test(name) || name === r.initialName) {
      preview = null;
      planning = false;
      return;
    }
    planning = true;
    const seq = ++planSeq;
    planTimer = setTimeout(() => {
      void renamePlan(r.file, r.source, r.offset, name)
        .then((p) => { if (seq === planSeq) { preview = p; planning = false; } })
        .catch(() => { if (seq === planSeq) { preview = null; planning = false; } });
    }, 250);
  });

  async function doRename() {
    const r = req;
    const p = preview;
    if (!r || !p || !canRename) return;
    applying = true;
    try {
      // Through the store, not a loop of our own: that is the one write path that verifies each
      // edit still matches the text it was computed against, and that treats the whole set as ONE
      // bulk edit — a rename touching hundreds of files must not schedule hundreds of whole-project
      // re-validations behind itself.
      const failed = await projectStore.applyEdits(p.files.flatMap((f) => f.edits));

      // The file move comes AFTER the edits: they are addressed to the old path.
      let moved = '';
      if (p.file_rename) {
        const base = p.file_rename.to.split('/').pop() ?? p.file_rename.to;
        try {
          await projectStore.renameFile(p.file_rename.from, base);
          moved = ` · renamed the file to ${base}`;
        } catch (e) {
          toastStore.show(`Renamed, but the file could not be moved: ${e}`, 'error');
          bennuRefactorStore.closeRename();
          return;
        }
      }

      const summary = `${p.total_edits} edit(s) in ${p.files.length} file(s)${moved}`;
      if (failed) {
        toastStore.show(`Renamed, but ${failed} file(s) could not be written · ${summary}`, 'error');
      } else {
        toastStore.show(`Renamed to “${newName.trim()}” · ${summary}`, 'success');
      }
      bennuRefactorStore.closeRename();
    } catch {
      toastStore.show('Rename failed', 'error');
    } finally {
      applying = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      void doRename();
    }
  }
</script>

<Modal {onClose} width="660px" height="560px" padBody={false} ariaLabel="Rename symbol">
  {#snippet header()}
    <ModalHeader {onClose}>
      <PenLine size={14} />
      <span class="modal-title">Rename</span>
      {#if req}<span class="hdr-old">{req.initialName}</span>{/if}
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" onkeydown={onKeydown}>
    {#if !req}
      <EmptyState message="Place the caret on a symbol, then press Shift+F6." />
    {:else}
      <div class="field-row">
        <FormField label="New name" hint="Rename is best-effort and config-aware; OGNL / JSP references are not rewritten.">
          <Input bind:value={newName} placeholder="newName" />
        </FormField>
      </div>

      <div class="preview">
        {#if planning}
          <div class="pv-state"><Spinner size={13} /> Computing preview…</div>
        {:else if !valid}
          <div class="pv-state muted">Enter a different, valid Java identifier.</div>
        {:else if (!preview || preview.total_edits === 0) && !preview?.blocked}
          <!-- A refusal is NOT this case, even with nothing to show: `blocked` names the symbol and
               the reason, and collapsing it into "isn't renameable, or the index is still building"
               told the user neither — it reads as a bug in Bennu rather than as an answer. -->
          <div class="pv-state muted">No edits — the symbol under the caret isn't renameable, or the index is still building.</div>
        {:else if preview}
          <div class="pv-head">
            <span class="pv-target">{preview.target_label}</span>
            {#if preview.total_edits > 0}
              <span class="pv-count">{preview.total_edits} edit{preview.total_edits === 1 ? '' : 's'} · {preview.files.length} file{preview.files.length === 1 ? '' : 's'}</span>
            {/if}
          </div>
          {#if preview.blocked}
            <!-- Shown WITH the edit list, not instead of it: the list is what makes the reason
                 legible. Apply is disabled. -->
            <Alert variant="error">{preview.blocked}</Alert>
          {:else if preview.has_inferred}
            <div class="pv-warn"><AlertTriangle size={12} /> Some edits are inferred (e.g. overloaded calls) — review before applying.</div>
          {/if}
          <div class="pv-files">
            {#each preview.files as f (f.file)}
              <div class="pv-file">
                <div class="pv-file-head"><FileCode2 size={12} /> {baseName(f.file)} <span class="pv-file-n">{f.edits.length}</span></div>
                {#each f.edits as e, i (i)}
                  <div class="pv-edit" class:inferred={e.inferred}>
                    <span class="pv-reason r-{e.reason}">{e.reason}</span>
                    <ValueChange from={e.old} to={e.new_text} />
                    {#if e.inferred}<span class="pv-inf">inferred</span>{/if}
                  </div>
                {/each}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="end">
      <Button variant="secondary" size="sm" onclick={onClose}>Cancel</Button>
      <Button
        variant="primary"
        size="sm"
        onclick={doRename}
        disabled={!canRename}
        tooltip={{ content: 'Apply rename', shortcut: 'Ctrl+Enter' }}
      >
        {applying ? 'Renaming…' : 'Rename'}
      </Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .hdr-old { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted); }

  .body { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .field-row { padding: 14px 16px 8px; flex-shrink: 0; }

  .preview { flex: 1; min-height: 0; overflow-y: auto; padding: 0 16px 12px; }
  .pv-state { display: flex; align-items: center; gap: 7px; padding: 16px 2px; font-size: var(--font-size-sm); color: var(--text-secondary); }
  .pv-state.muted { color: var(--text-muted); }

  .pv-head { display: flex; align-items: center; gap: 8px; padding: 6px 0 8px; position: sticky; top: 0; background: var(--bg-base); }
  .pv-target { flex: 1; min-width: 0; font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pv-count { font-size: var(--font-size-2xs); color: var(--text-muted); flex-shrink: 0; }

  .pv-warn {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 9px; margin-bottom: 8px; font-size: var(--font-size-xs);
    color: var(--warning); background: color-mix(in srgb, var(--warning) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--warning) 26%, transparent); border-radius: var(--radius-sm);
  }

  .pv-files { display: flex; flex-direction: column; gap: 10px; }
  .pv-file-head {
    display: flex; align-items: center; gap: 6px;
    font-size: var(--font-size-xs); color: var(--text-primary); font-weight: 500;
    padding-bottom: 3px; border-bottom: 1px solid var(--border-subtle); margin-bottom: 3px;
  }
  .pv-file-head :global(svg) { color: var(--text-muted); }
  .pv-file-n { font-size: var(--font-size-2xs); color: var(--text-muted); font-weight: 600; }

  .pv-edit { display: flex; align-items: center; gap: 8px; padding: 3px 4px; font-size: var(--font-size-xs); }
  .pv-reason {
    flex-shrink: 0; font-size: var(--font-size-3xs); font-weight: 700; text-transform: uppercase; letter-spacing: 0.4px;
    padding: 1px 5px; border-radius: var(--radius-sm); min-width: 66px; text-align: center;
    background: var(--bg-overlay); color: var(--text-muted);
  }
  .r-declaration { color: var(--accent); background: var(--accent-subtle); }
  .r-spring-bean { color: var(--info); background: color-mix(in srgb, var(--info) 14%, transparent); }
  /* The change itself is `ValueChange`; the flag is pushed to the far edge, which is what the
     old-name/new-name span used to do by taking the slack. */
  .pv-inf { margin-left: auto; font-size: var(--font-size-3xs); color: var(--warning); flex-shrink: 0; }
</style>
