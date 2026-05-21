<script lang="ts">
  import { Maximize2, Copy, Check, ChevronUp, ChevronDown, FileText } from 'lucide-svelte';
  import Contribution from '$lib/components/shared/Contribution.svelte';
  import PluginIcon   from '$lib/components/plugins/PluginIcon.svelte';
  import EncodingPill from '$lib/components/shared/ui/EncodingPill.svelte';
  import { diffStore } from '$lib/stores/diff.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { encodingOverrides } from '$lib/stores/encodingOverrides.svelte';
  import type { DiffFile } from '$lib/types/git';
  import { computeChunkAnchors } from '$lib/utils/diff-chunks';
  import { tooltipForAction, shortcutFor } from '$lib/utils/shortcut';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    file,
    stageable = false,
    staged = false,
    selectedCount = 0,
    currentChunkIdx = 0,
    copyDone = false,
    showPath = true,
    onStageSelected,
    onCopyCode,
    onOpenFullscreen,
    onPrevChunk,
    onNextChunk,
    onEncodingChange,
  }: {
    file: DiffFile;
    stageable?: boolean;
    staged?: boolean;
    selectedCount?: number;
    currentChunkIdx?: number;
    copyDone?: boolean;
    showPath?: boolean;
    onStageSelected?: () => void;
    onCopyCode?: () => void;
    onOpenFullscreen?: () => void;
    onPrevChunk?: () => void;
    onNextChunk?: () => void;
    onEncodingChange?: () => void;
  } = $props();

  const mode     = $derived(diffStore.mode);
  const fullFile = $derived(diffStore.fullFile);

  const chunkCount = $derived(computeChunkAnchors(file).length);
  const selectionLabel = $derived(
    selectedCount === 0
      ? null
      : `${staged ? 'Unstage' : 'Stage'} ${selectedCount} line${selectedCount === 1 ? '' : 's'}`
  );

  const activeRepoPath = $derived(
    tabsStore.tabs.find(t => t.id === tabsStore.activeTabId)?.path ?? null
  );
  const encodingOverridden = $derived.by(() => {
    if (!activeRepoPath || !file) return false;
    return encodingOverrides.get(activeRepoPath, file.path) !== undefined;
  });

  function pickEncoding(label: string | undefined) {
    if (!activeRepoPath || !file) return;
    if (label === undefined) encodingOverrides.clear(activeRepoPath, file.path);
    else encodingOverrides.set(activeRepoPath, file.path, label);
    onEncodingChange?.();
  }
</script>

{#if showPath}
  <span class="diff-path">{file.old_path ? `${file.old_path} → ` : ''}{file.path}</span>
{/if}
<div class="diff-stats">
  {#if file.stats.additions > 0}<span class="add">+{file.stats.additions}</span>{/if}
  {#if file.stats.deletions > 0}<span class="del">-{file.stats.deletions}</span>{/if}
</div>

{#if stageable && selectionLabel}
  <button class="stage-sel-btn" onclick={() => onStageSelected?.()} use:tooltip={selectionLabel}>
    {selectionLabel}
  </button>
{/if}

{#if chunkCount > 0}
  <div class="chunk-nav" use:tooltip={`Navigate change chunks (${shortcutFor('next_chunk') ?? 'F3'} / ${shortcutFor('prev_chunk') ?? 'Shift+F3'})`}>
    <button class="expand-btn" onclick={() => onPrevChunk?.()} use:tooltip={tooltipForAction('Previous chunk', 'prev_chunk')} aria-label="Previous chunk">
      <ChevronUp size={12} />
    </button>
    <span class="chunk-counter">{currentChunkIdx + 1}/{chunkCount}</span>
    <button class="expand-btn" onclick={() => onNextChunk?.()} use:tooltip={tooltipForAction('Next chunk', 'next_chunk')} aria-label="Next chunk">
      <ChevronDown size={12} />
    </button>
  </div>
{/if}

<div class="mode-toggle">
  <button class="mode-btn" class:active={mode === 'unified'} onclick={() => diffStore.setMode('unified')}>Unified</button>
  <button class="mode-btn" class:active={mode === 'split'}   onclick={() => diffStore.setMode('split')}>Split</button>
</div>

<button
  class="expand-btn"
  class:toggle-on={fullFile}
  use:tooltip={fullFile ? 'Showing full file (click to disable)' : 'Show full file as context'}
  aria-pressed={fullFile}
  onclick={() => diffStore.setFullFile(!fullFile)}
>
  <FileText size={12} />
</button>

<button
  class="expand-btn"
  class:copy-done={copyDone}
  use:tooltip={'Copy code (without line numbers)'}
  onclick={() => onCopyCode?.()}
>
  {#if copyDone}<Check size={12} />{:else}<Copy size={12} />{/if}
</button>

{#if file.encoding}
  <EncodingPill
    encoding={file.encoding}
    overridden={encodingOverridden}
    onChange={pickEncoding}
    compact
  />
{/if}

<Contribution point="arbor:diff-toolbar">
  {#snippet item({ payload, fire })}
    {@const p = payload as { icon: string; action: string; label?: string; tooltip?: string }}
    <button
      type="button"
      class="plugin-diff-toolbar-item"
      use:tooltip={p.tooltip ?? p.label ?? ''}
      onclick={() => fire()}
    >
      <PluginIcon name={p.icon} size={14} />
      {#if p.label}<span>{p.label}</span>{/if}
    </button>
  {/snippet}
</Contribution>

<button class="expand-btn" use:tooltip={'Full screen'} onclick={() => onOpenFullscreen?.()}>
  <Maximize2 size={12} />
</button>

<style>
  .diff-path {
    /* High flex-grow so this dominates any sibling spacers in a header.
       The element is also used inside DiffViewer's own header where the
       grow:1 spacer doesn't exist; flex-basis:0 makes it consume all
       free space without affecting siblings. */
    flex: 100 1 0;
    min-width: 0;
    font-family: var(--font-code);
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .diff-stats { display: flex; gap: 8px; flex-shrink: 0; }
  .add { color: var(--success); }
  .del { color: var(--error); }

  .stage-sel-btn {
    padding: 2px 10px;
    background: var(--accent);
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-on-accent);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    font-weight: 500;
    white-space: nowrap;
    transition: background var(--transition-fast);
    animation: fadeIn 100ms ease;
    flex-shrink: 0;
  }
  .stage-sel-btn:hover { background: var(--accent-hover, #3b5fc0); }
  @keyframes fadeIn { from { opacity: 0; transform: scale(0.95); } to { opacity: 1; transform: scale(1); } }

  .chunk-nav {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 4px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
    flex-shrink: 0;
  }
  .chunk-counter {
    font-family: var(--font-ui-sans);
    font-size: 10px;
    color: var(--text-muted);
    min-width: 28px;
    text-align: center;
    user-select: none;
  }

  .mode-toggle { display: flex; gap: 2px; flex-shrink: 0; }
  .mode-btn {
    padding: 2px 8px;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    transition: all var(--transition-fast);
  }
  .mode-btn.active, .mode-btn:hover {
    background: var(--accent-subtle);
    color: var(--accent);
    border-color: var(--accent);
  }

  .expand-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px; height: 22px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .expand-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .expand-btn.copy-done { color: var(--success); }
  .expand-btn.toggle-on {
    background: var(--accent-subtle);
    color: var(--accent);
  }

  .plugin-diff-toolbar-item {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    cursor: pointer;
    color: var(--text-secondary);
    font-size: 12px;
    flex-shrink: 0;
  }
  .plugin-diff-toolbar-item:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border);
  }
</style>
