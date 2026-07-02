<script lang="ts">
  /** Tyto docs — Keyboard shortcuts (rendered from the canonical list). */
  import { TYTO_SHORTCUTS } from '../tyto-shortcuts';
</script>

<h1>Keyboard shortcuts</h1>
<p class="doc-lead">
  Tyto is built to be driven entirely from the keyboard. The same reference is
  available any time with <kbd>Shift</kbd>+<kbd>F1</kbd>.
</p>

{#each TYTO_SHORTCUTS as group (group.label)}
  <h2>{group.label}</h2>
  <table>
    <thead>
      <tr><th>Shortcut</th><th>Action</th></tr>
    </thead>
    <tbody>
      {#each group.shortcuts as s (s.description)}
        <tr>
          <td>
            {#each s.keys as k, i}{#if i > 0}+{/if}<kbd>{k}</kbd>{/each}
          </td>
          <td>{s.description}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/each}
