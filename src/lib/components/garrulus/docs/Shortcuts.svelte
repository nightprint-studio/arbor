<script lang="ts">
  /**
   * Garrulus docs — keyboard shortcuts. Rendered from the same canonical list the
   * reference modal and the shell's key handler use, so the three cannot drift.
   */
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { GARRULUS_SHORTCUTS } from '../garrulus-shortcuts';
</script>

<h1>Keyboard shortcuts</h1>
<p class="doc-lead">
  Every action in Garrulus is reachable from the keyboard, and everything with a name is
  in the command palette (<kbd>Ctrl</kbd>+<kbd>K</kbd>) whether it has a key or not. The
  bindings below are local to this window, and <kbd>Shift</kbd>+<kbd>F1</kbd> comes
  straight back to this page from wherever you are.
</p>

{#each GARRULUS_SHORTCUTS as group (group.label)}
  <h2>{group.label}</h2>
  <table>
    <tbody>
      {#each group.shortcuts as s (s.description)}
        <tr>
          <td class="doc-keys"><Kbd keys={s.keys} size="sm" /></td>
          <td>{s.description}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/each}

<style>
  .doc-keys { white-space: nowrap; width: 1%; padding-right: 14px; }
</style>
