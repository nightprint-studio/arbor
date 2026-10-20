<script lang="ts">
  /**
   * The popup a change-gutter bar opens: the block as it was in HEAD against what is there now,
   * with *Revert block* and *Copy old text*, and arrows to the previous / next change.
   *
   * It only reports intent — reverting rewrites the buffer, and the buffer is the editor's.
   * Keyboard-first: Esc closes, Alt+↑ / Alt+↓ walk the changes, and the buttons are real
   * buttons in tab order.
   */
  import { ArrowUp, ArrowDown, Undo2, Copy, X } from 'lucide-svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import TextDiff, { type DiffHunkModel, type DiffLineModel } from '$lib/components/shared/ui/TextDiff.svelte';
  import type { ChangeBlock } from '$lib/components/shared/ui/code-editor/change-gutter';

  interface Props {
    block: ChangeBlock;
    /** What the block's lines say now (no trailing newline per line needed). */
    newText: string;
    /** Viewport position of the click. */
    x: number;
    y: number;
    onRevert: () => void;
    onCopyOld: () => void;
    onPrev: () => void;
    onNext: () => void;
    onClose: () => void;
  }

  let { block, newText, x, y, onRevert, onCopyOld, onPrev, onNext, onClose }: Props = $props();

  const TITLE = { added: 'Added lines', modified: 'Changed lines', removed: 'Removed lines' } as const;
  const lines = (text: string) => (text === '' ? [] : text.replace(/\n$/, '').split('\n'));

  const hunk = $derived.by<DiffHunkModel>(() => {
    const body: DiffLineModel[] = [];
    const start = block.fromLine;
    lines(block.hunk.old_text).forEach((text, i) => body.push({ kind: 'del', old: i + 1, text }));
    if (block.hunk.kind !== 'removed') {
      lines(newText).forEach((text, i) => body.push({ kind: 'add', new: start + i, text }));
    }
    return { old_start: 1, new_start: start, lines: body };
  });

  const revertLabel = $derived(block.hunk.kind === 'added' ? 'Remove block' : 'Revert block');

  // Keep the popup inside the viewport; it is anchored below the click unless that would not fit.
  const WIDTH = 520;
  const left = $derived(Math.max(8, Math.min(x, window.innerWidth - WIDTH - 8)));
  const top = $derived(y + 260 > window.innerHeight ? Math.max(8, y - 260) : y + 8);

  let root = $state<HTMLDivElement | undefined>(undefined);
  $effect(() => { root?.focus(); });

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); onClose(); }
    else if (e.altKey && e.key === 'ArrowDown') { e.preventDefault(); onNext(); }
    else if (e.altKey && e.key === 'ArrowUp') { e.preventDefault(); onPrev(); }
  }

  // A click anywhere else dismisses it, like a menu.
  $effect(() => {
    const away = (e: PointerEvent) => { if (root && !root.contains(e.target as Node)) onClose(); };
    window.addEventListener('pointerdown', away, true);
    return () => window.removeEventListener('pointerdown', away, true);
  });
</script>

<div
  bind:this={root}
  class="cp"
  role="dialog"
  tabindex="-1"
  aria-label={TITLE[block.hunk.kind]}
  style:left="{left}px"
  style:top="{top}px"
  style:width="{WIDTH}px"
  onkeydown={onKeydown}
>
  <div class="cp-head">
    <span class="cp-title">{TITLE[block.hunk.kind]}</span>
    <span class="cp-lines">
      {block.hunk.kind === 'removed' ? `before line ${block.fromLine}` : block.fromLine === block.toLine ? `line ${block.fromLine}` : `lines ${block.fromLine}–${block.toLine}`}
    </span>
    <span class="cp-spacer"></span>
    <IconButton size={22} tooltip="Previous change" shortcut="Alt+↑" onclick={onPrev}><ArrowUp size={13} /></IconButton>
    <IconButton size={22} tooltip="Next change" shortcut="Alt+↓" onclick={onNext}><ArrowDown size={13} /></IconButton>
    <IconButton size={22} tooltip="Close" shortcut="Esc" onclick={onClose}><X size={13} /></IconButton>
  </div>

  <div class="cp-diff">
    <TextDiff hunks={[hunk]} identical={false} mode="unified" emptyMessage="Nothing to show." />
  </div>

  <div class="cp-foot">
    <Button size="xs" variant="outline" onclick={onRevert}>
      {#snippet iconStart()}<Undo2 size={12} />{/snippet}
      {revertLabel}
    </Button>
    {#if block.hunk.old_text}
      <Button size="xs" variant="ghost" onclick={onCopyOld}>
        {#snippet iconStart()}<Copy size={12} />{/snippet}
        Copy old text
      </Button>
    {/if}
  </div>
</div>

<style>
  .cp {
    position: fixed;
    z-index: 1000;
    display: flex;
    flex-direction: column;
    max-height: 260px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-lg, 0 8px 24px rgba(0, 0, 0, 0.35));
    outline: none;
    overflow: hidden;
  }
  .cp-head {
    display: flex; align-items: center; gap: 6px;
    padding: 4px 6px 4px 10px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs);
  }
  .cp-title { font-weight: 600; color: var(--text-primary); }
  .cp-lines { color: var(--text-muted); font-family: var(--font-code); }
  .cp-spacer { flex: 1; }
  .cp-diff { flex: 1; min-height: 0; overflow: auto; background: var(--bg-base); }
  .cp-foot { display: flex; gap: 6px; padding: 6px 8px; border-top: 1px solid var(--border-subtle); }
</style>
