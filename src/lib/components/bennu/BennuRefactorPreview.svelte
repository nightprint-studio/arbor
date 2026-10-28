<script lang="ts">
  /**
   * BennuRefactorPreview — the per-file edit list a project-wide refactor shows before it writes
   * anything: what it targets, how many edits in how many files, a refusal (`blocked`) or a review
   * nudge (`inferred`), then every edit grouped by file with why it exists.
   *
   * Shared by the rename and add-parameter dialogs, which plan through the same wire shape
   * (`RenamePreview`) — so the list, its refusal wording and its look stay one thing.
   */
  import { FileCode2, AlertTriangle } from 'lucide-svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import ValueChange from '$lib/components/shared/ui/ValueChange.svelte';
  import type { RenamePreview } from '$lib/ipc/bennu/nav';

  let {
    preview,
    planning,
    valid,
    invalidMessage,
    emptyMessage,
  }: {
    preview: RenamePreview | null;
    /** A plan is being computed. */
    planning: boolean;
    /** The input the plan depends on is in a state worth planning; `false` shows `invalidMessage`. */
    valid: boolean;
    /** What to say while the input is not yet valid. */
    invalidMessage: string;
    /** What to say when the engine answered with nothing to do. */
    emptyMessage: string;
  } = $props();

  function baseName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }
</script>

<div class="preview">
  {#if planning}
    <div class="pv-state"><Spinner size={13} /> Computing preview…</div>
  {:else if !valid}
    <div class="pv-state muted">{invalidMessage}</div>
  {:else if (!preview || preview.total_edits === 0) && !preview?.blocked}
    <!-- A refusal is NOT this case, even with nothing to show: `blocked` names the symbol and
         the reason, and collapsing it into "isn't renameable, or the index is still building"
         told the user neither — it reads as a bug in Bennu rather than as an answer. -->
    <div class="pv-state muted">{emptyMessage}</div>
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

<style>
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
