<!--
  StudioHeaderUndoRedo — pair of compact undo / redo buttons that the
  format wrappers drop INSIDE their own `headerLeft` snippet, between
  the file icon and their own tab strip / title cluster.

  Sits on the modal-header chrome bg so the buttons read as part of
  the titlebar — visually distinct from the right-rail icon stack and
  the Tools sidecar. Same disabled / shortcut wiring as the previous
  footer-center version.
-->
<script lang="ts">
  import { Undo2, Redo2 } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { StudioFooterDoc } from './studio-footer-types';

  interface Props {
    doc:    StudioFooterDoc;
    onUndo: () => void | Promise<void>;
    onRedo: () => void | Promise<void>;
  }

  const { doc, onUndo, onRedo }: Props = $props();
</script>

<div class="sh-undo-redo" role="group" aria-label="Undo / redo">
  <button type="button" class="sh-btn"
    onclick={() => void onUndo()}
    disabled={!doc.canUndo}
    use:tooltip={{ content: 'Undo', shortcut: 'Ctrl+Z' }}
    aria-label="Undo"
  ><Undo2 size={14} /></button>

  <button type="button" class="sh-btn"
    onclick={() => void onRedo()}
    disabled={!doc.canRedo}
    use:tooltip={{ content: 'Redo', shortcut: 'Ctrl+Shift+Z' }}
    aria-label="Redo"
  ><Redo2 size={14} /></button>
</div>

<style>
  .sh-undo-redo {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    margin-left: 6px;
    padding: 2px;
    border: 1px solid var(--border-subtle);
    border-radius: 5px;
    background: var(--bg-base);
  }
  .sh-btn {
    display: inline-flex; align-items: center; justify-content: center;
    width: 22px; height: 22px;
    border: none;
    background: transparent;
    color: var(--text-primary);
    border-radius: 3px;
    cursor: pointer;
  }
  .sh-btn:hover:not(:disabled) { background: var(--bg-hover); }
  .sh-btn:disabled { color: var(--text-disabled); cursor: not-allowed; }
</style>
