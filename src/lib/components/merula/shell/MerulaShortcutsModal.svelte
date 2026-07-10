<script lang="ts">
  /**
   * Merula keyboard-shortcuts cheat-sheet. Read-only reference for the merula
   * window's bindings (the canonical set is `merula-keybindings.ts`). Merula keeps
   * its own binding registry — separate from Arbor's rebindable `keybindings.ts`
   * — so the window stays self-contained / extractable; these aren't rebindable.
   */
  import { Keyboard } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { MERULA_BINDINGS, type MerulaBinding } from '../merula-keybindings';
  import { macKeyLabel } from '$lib/utils/keybindings';
  import { isMac } from '$lib/utils/platform';

  /** Human key label for a single binding key (Space / letter / fn / punctuation). */
  function keyLabel(k: string): string {
    if (k === ' ') return 'Space';
    if (k.length === 1) return k.toUpperCase();
    return k; // F1, …
  }

  /**
   * Render-time chord pieces. Built as words (Ctrl · Alt · Shift · key), then
   * folded to macOS glyphs on the Mac via the shared {@link macKeyLabel} so the
   * cheat-sheet matches the rest of the app. Also applies to the pre-split
   * `keys` arrays (editor / contextual), so 'Ctrl' → ⌘ there too.
   */
  function displayParts(parts: string[]): string[] {
    return isMac ? macKeyLabel(parts.join('+')).split('+') : parts;
  }

  /** The chord pieces, in render order (Ctrl · Alt · Shift · key). */
  function chord(b: MerulaBinding): string[] {
    const parts: string[] = [];
    if (b.ctrl) parts.push('Ctrl');
    if (b.alt) parts.push('Alt');
    if (b.shift) parts.push('Shift');
    parts.push(keyLabel(b.key));
    return displayParts(parts);
  }

  // Editor keys provided by CodeMirror (not keydown bindings in MERULA_BINDINGS) —
  // surfaced here so the cheat-sheet is complete.
  const editorKeys = [
    { keys: ['Ctrl', '/'],          description: 'Toggle line comment (or the selected lines)' },
    { keys: ['Ctrl', 'Y'],          description: 'Delete the current line' },
    { keys: ['Ctrl', 'Space'],      description: 'Trigger autocomplete' },
    { keys: ['Ctrl', 'Shift', 'Z'], description: 'Redo' },
  ];

  // Ctrl+Click is handled in the editor mousedown (not a keydown binding), so it
  // isn't in MERULA_BINDINGS — surface it here as a documented contextual key.
  const contextual = [
    { keys: ['Ctrl', 'Click'],  description: 'Editor: go to declaration (fn / let / import, incl. cross-file)' },
    { keys: ['Ctrl', 'Click'],  description: 'Editor: preview an instrument name — inst("…") / s("…")' },
    { keys: ['Ctrl', 'Click'],  description: 'Arrangement: reveal the source that produced a hap' },
    { keys: ['Right-click'],    description: 'Editor: play the selection one-shot · send it to Scratch' },
    { keys: ['Click'],          description: 'Outline: ▶ plays a track / constant one-shot' },
    { keys: ['Select'],         description: 'Editor: highlights the matching regions on the arrangement' },
    { keys: ['Drag'],           description: 'Arrangement ruler: scrub the seek cursor' },
    { keys: ['Wheel'],          description: 'Arrangement ruler: scroll the timeline (Shift+wheel anywhere)' },
    { keys: ['Drag'],           description: 'Reorder editor tabs · fold blocks via the gutter arrows' },
    { keys: ['↑', '↓'],         description: 'Arrangement: move between track lanes' },
    { keys: ['←', '→'],         description: 'Arrangement: nudge + seek the cursor' },
    { keys: ['Home'],           description: 'Arrangement: seek to the start' },
  ];
</script>

<Modal {onClose} width="560px" height="560px" ariaLabel="Keyboard Shortcuts">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Keyboard size={14} />
      <span class="modal-title">Keyboard Shortcuts</span>
    </ModalHeader>
  {/snippet}

  <p class="sc-lead">Merula is fully keyboard-navigable. Editor-scoped keys fire only when the tab pane has focus.</p>

  <table class="sc-table">
    <tbody>
      {#each MERULA_BINDINGS as b (b.id)}
        <tr>
          <td class="sc-keys">
            {#each chord(b) as part, i (i)}
              {#if i > 0 && !isMac}<span class="sc-plus">+</span>{/if}
              <kbd>{part}</kbd>
            {/each}
          </td>
          <td class="sc-desc">
            {b.description}
            {#if b.scope === 'editor'}<span class="sc-scope">editor</span>{/if}
          </td>
        </tr>
      {/each}

      <tr><td colspan="2" class="sc-section">Editor</td></tr>
      {#each editorKeys as c (c.description)}
        <tr>
          <td class="sc-keys">
            {#each displayParts(c.keys) as part, i (i)}
              {#if i > 0 && !isMac}<span class="sc-plus">+</span>{/if}
              <kbd>{part}</kbd>
            {/each}
          </td>
          <td class="sc-desc">{c.description}<span class="sc-scope">editor</span></td>
        </tr>
      {/each}

      <tr><td colspan="2" class="sc-section">Contextual</td></tr>
      {#each contextual as c (c.description)}
        <tr>
          <td class="sc-keys">
            {#each displayParts(c.keys) as part, i (i)}
              {#if i > 0 && !isMac}<span class="sc-plus">+</span>{/if}
              <kbd>{part}</kbd>
            {/each}
          </td>
          <td class="sc-desc">{c.description}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .sc-lead { font-size: 12px; color: var(--text-secondary); margin: 0 0 14px; line-height: 1.5; }

  .sc-table { width: 100%; border-collapse: collapse; }
  .sc-table td { padding: 6px 0; vertical-align: middle; border-bottom: 1px solid var(--border-subtle); }
  .sc-keys { width: 160px; white-space: nowrap; }
  .sc-desc { font-size: 12px; color: var(--text-secondary); }
  .sc-scope {
    margin-left: 8px; padding: 0 5px; font-size: 9px; font-weight: 700;
    text-transform: uppercase; letter-spacing: 0.4px;
    color: var(--text-muted); background: var(--bg-overlay);
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
  }
  .sc-plus { color: var(--text-disabled); font-size: 10px; margin: 0 3px; }
  .sc-section {
    padding-top: 14px !important; padding-bottom: 4px !important;
    font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.6px;
    color: var(--text-muted); border-bottom: none !important;
  }
  kbd {
    display: inline-block;
    font-family: var(--font-code); font-size: 10.5px; line-height: 1.6;
    padding: 1px 6px; min-width: 18px; text-align: center;
    color: var(--text-secondary);
    background: var(--bg-overlay);
    border: 1px solid var(--border); border-bottom-width: 2px;
    border-radius: var(--radius-sm);
  }
</style>
