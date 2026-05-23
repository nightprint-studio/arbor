<!--
  PresenceConflictResolver — dedicated UI for modify/delete and add/modify
  conflicts.

  These cases are *structurally* different from the regular line-merge:
    · modify/delete — one branch modified the file, the other deleted it.
    · add/modify   — one branch added the file (no equivalent on the other).

  The regular 2-column diff editor is misleading here because it would
  duplicate context lines on both sides (there are no `<<<<<<<` markers in
  the workdir for these cases). Reducing the choice to a single binary
  toggle — Keep vs Discard — with a live preview of what "keep" would mean
  is both faster to grok and faster to act on.

  The owning modal supplies which side is missing (`presence`) and the
  side labels; this widget renders the banner, the two choice buttons, and
  the preview block.
-->
<script lang="ts">
  import { CheckCircle2, X, TriangleAlert } from 'lucide-svelte';
  import { highlight } from '$lib/utils/diff-formatter';

  interface Props {
    /** Which side has the file. */
    presence:      { ours: boolean; theirs: boolean };
    /** Branch labels (ours, theirs) shown in the banner / preview header. */
    labels:        { ours: string; theirs: string };
    /** User's current pick. */
    decision:      'keep' | 'remove';
    /** Lines that would survive a "keep" — comes from the working content. */
    previewLines:  string[];
    /** File path — used as the highlighter's filename hint. */
    path:          string;

    onPick:        (next: 'keep' | 'remove') => void;
  }

  let { presence, labels, decision, previewLines, path, onPick }: Props = $props();

  // The "surviving" side label: when `ours` is missing, the file lives on
  // theirs; when `theirs` is missing, the file lives on ours.
  const sideLabel = $derived(!presence.ours ? labels.theirs : labels.ours);
</script>

<div class="presence-resolver">
  <div class="presence-banner" class:added={!presence.ours} class:deleted={!presence.theirs}>
    <TriangleAlert size={13} />
    {#if !presence.ours}
      <span>
        <strong>Added on {labels.theirs}</strong> — this file does not exist on
        <code>{labels.ours}</code>. Choose to keep the incoming file or discard it.
      </span>
    {:else}
      <span>
        <strong>Deleted on {labels.theirs}</strong> — this file no longer exists on
        <code>{labels.theirs}</code>. Choose to keep your current version or accept the deletion.
      </span>
    {/if}
  </div>

  <div class="presence-choice-row">
    <button
      type="button"
      class="presence-choice"
      class:active={decision === 'keep'}
      onclick={() => onPick('keep')}
    >
      <span class="presence-choice-title">
        <CheckCircle2 size={13} />
        Keep file
      </span>
      <span class="presence-choice-sub">Use the version from <code>{sideLabel}</code></span>
    </button>
    <button
      type="button"
      class="presence-choice danger"
      class:active={decision === 'remove'}
      onclick={() => onPick('remove')}
    >
      <span class="presence-choice-title">
        <X size={13} />
        Accept deletion
      </span>
      <span class="presence-choice-sub">Remove the file from workdir and index</span>
    </button>
  </div>

  <div class="presence-preview">
    <div class="presence-preview-header">
      {#if decision === 'remove'}
        <span class="presence-preview-title removed">
          <X size={12} /> Resolution: file will be removed
        </span>
      {:else}
        <span class="presence-preview-title kept">
          <CheckCircle2 size={12} /> Resolution: file will be kept ({sideLabel})
        </span>
      {/if}
    </div>
    {#if decision === 'remove'}
      <div class="presence-preview-empty">
        — the file will be removed from the working tree and the index —
      </div>
    {:else}
      <div class="presence-preview-body">
        {#each previewLines as line, i}
          <div class="presence-preview-row">
            <span class="presence-preview-linenum">{i + 1}</span>
            <code class="presence-preview-code">{@html highlight(line.replace(/\n$/, ''), path)}</code>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .presence-resolver {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .presence-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    font-size: 11px;
    line-height: 1.45;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .presence-banner code {
    font-family: var(--font-code);
    padding: 1px 5px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
  }
  .presence-banner.added {
    background: color-mix(in srgb, var(--success) 10%, transparent);
    color: var(--text-primary);
    border-bottom-color: color-mix(in srgb, var(--success) 35%, transparent);
  }
  .presence-banner.added :global(svg) { color: var(--success); }
  .presence-banner.deleted {
    background: color-mix(in srgb, var(--danger) 10%, transparent);
    color: var(--text-primary);
    border-bottom-color: color-mix(in srgb, var(--danger) 35%, transparent);
  }
  .presence-banner.deleted :global(svg) { color: var(--danger); }

  .presence-choice-row {
    display: flex;
    gap: 10px;
    padding: 12px;
    flex-shrink: 0;
  }
  .presence-choice {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 10px 14px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
    text-align: left;
    font-family: var(--font-ui-sans);
  }
  .presence-choice:hover { background: var(--bg-hover); }
  .presence-choice.active {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .presence-choice.danger.active {
    border-color: var(--danger);
    background: color-mix(in srgb, var(--danger) 12%, transparent);
  }
  .presence-choice-title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .presence-choice.active .presence-choice-title { color: var(--accent); }
  .presence-choice.danger.active .presence-choice-title { color: var(--danger); }
  .presence-choice-sub {
    font-size: 11px;
    color: var(--text-secondary);
  }
  .presence-choice-sub code {
    font-family: var(--font-code);
    padding: 0 4px;
    border-radius: 2px;
    background: var(--bg-base);
  }

  .presence-preview {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    margin: 0 12px 12px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .presence-preview-header {
    padding: 6px 12px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-overlay);
    font-size: 11px;
    flex-shrink: 0;
  }
  .presence-preview-title {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-weight: 600;
  }
  .presence-preview-title.kept    { color: var(--success); }
  .presence-preview-title.removed { color: var(--danger); }

  .presence-preview-body {
    margin: 0;
    padding: 6px 0;
    flex: 1;
    overflow: auto;
    font-family: var(--font-code);
    font-size: var(--font-size-sm);
    line-height: 1.45;
    color: var(--text-primary);
  }
  .presence-preview-row {
    display: flex;
    gap: 12px;
    padding: 0 12px;
    min-width: max-content;
  }
  .presence-preview-row:hover {
    background: color-mix(in srgb, var(--bg-overlay) 60%, transparent);
  }
  .presence-preview-linenum {
    flex-shrink: 0;
    width: 36px;
    text-align: right;
    color: var(--text-muted);
    user-select: none;
    font-variant-numeric: tabular-nums;
  }
  .presence-preview-code {
    flex: 1;
    white-space: pre;
    font-family: inherit;
    background: none;
  }
  .presence-preview-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    font-style: italic;
    font-size: 12px;
    padding: 24px;
  }
</style>
