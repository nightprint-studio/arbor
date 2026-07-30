<script lang="ts">
  /**
   * Floating intentions popover (IntelliJ Alt+Enter "Show Context Actions"). Lists
   * the quick-fixes / refactors applicable at the caret — rename, inline, extract,
   * fix an unresolved instrument, transpose notes — and applies the chosen one.
   * The floating chrome + keyboard nav live in the shared {@link FloatingPicker};
   * this only supplies the header + per-action row and stages the pick on the
   * `intentionsStore` (the editor relay does the actual edit). One mount in shell.
   */
  import { Lightbulb, Wand2 } from 'lucide-svelte';
  import FloatingPicker from './FloatingPicker.svelte';
  import { intentionsStore } from '../stores/intentions.svelte';
  import type { IntentionItem } from './merula-intentions';
</script>

<FloatingPicker
  open={intentionsStore.open}
  anchor={intentionsStore.anchor}
  width={340}
  items={intentionsStore.items}
  ariaLabel="Context actions"
  onSelect={(it) => intentionsStore.choose(it)}
  onClose={() => intentionsStore.close()}
>
  {#snippet header()}
    <Lightbulb size={12} />
    <span class="it-title">Context actions</span>
  {/snippet}

  {#snippet row(it: IntentionItem)}
    <span class="it-icon"><Wand2 size={12} /></span>
    <span class="it-label">{it.label}</span>
  {/snippet}

  {#snippet empty()}
    <div class="it-empty">No actions here — place the caret on a name, instrument, or note (or select a pattern).</div>
  {/snippet}
</FloatingPicker>

<style>
  .it-title { flex: 1; min-width: 0; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .it-icon { display: flex; flex-shrink: 0; color: var(--accent); }
  .it-label {
    flex: 1; min-width: 0; font-size: var(--font-size-sm); color: var(--text-primary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .it-empty { padding: 12px 14px; font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-muted); }
</style>
