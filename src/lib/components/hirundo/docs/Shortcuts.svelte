<script lang="ts">
  /**
   * Hirundo docs — keyboard shortcuts. Rendered from the same canonical list the shell's key handler uses,
   * so the two cannot drift.
   */
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { HIRUNDO_SHORTCUTS } from '../hirundo-shortcuts';
</script>

<h1>Keyboard shortcuts</h1>
<p class="doc-lead">
  Everything in Hirundo is reachable from the keyboard, and everything with a name is in the command palette
  (<kbd>Ctrl</kbd>+<kbd>K</kbd>) whether it has a key or not. The bindings are local to this window.
</p>

{#each HIRUNDO_SHORTCUTS as group (group.label)}
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
