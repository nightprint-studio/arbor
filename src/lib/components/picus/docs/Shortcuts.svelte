<script lang="ts">
  /**
   * Picus docs — keyboard shortcuts. Rendered from the same canonical list the
   * reference modal uses, so the two can never drift apart.
   */
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { PICUS_SHORTCUTS } from '../picus-shortcuts';
</script>

<h1>Keyboard shortcuts</h1>
<p class="doc-lead">
  Every action in Picus is reachable from the keyboard. The bindings below are
  window-local; <kbd>Shift</kbd>+<kbd>F1</kbd> opens the same list as a searchable
  reference.
</p>

{#each PICUS_SHORTCUTS as group (group.label)}
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
