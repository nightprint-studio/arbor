<script lang="ts">
  // Rendered from the canonical binding table so the docs never drift from the
  // live keymap (the same source the Shortcuts modal uses).
  import { NEMUS_BINDINGS, type NemusBinding } from '../nemus-keybindings';

  function chord(b: NemusBinding): string[] {
    const parts: string[] = [];
    if (b.ctrl) parts.push('Ctrl');
    if (b.alt) parts.push('Alt');
    if (b.shift) parts.push('Shift');
    parts.push(b.key.length === 1 ? b.key.toUpperCase() : b.key);
    return parts;
  }
  const global = NEMUS_BINDINGS.filter((b) => b.scope === 'global');
  const editor = NEMUS_BINDINGS.filter((b) => b.scope === 'editor');
</script>

<h1>Keyboard shortcuts</h1>
<p class="doc-lead">
  nemus is keyboard-first: every action is reachable without the mouse. Open the command
  palette with <kbd>Ctrl</kbd> + <kbd>K</kbd> to search them all by name.
</p>

<h2>Global</h2>
<table>
  <thead><tr><th>Keys</th><th>Action</th></tr></thead>
  <tbody>
    {#each global as b (b.id)}
      <tr>
        <td>{#each chord(b) as k, i (i)}{#if i > 0} + {/if}<kbd>{k}</kbd>{/each}</td>
        <td>{b.description}</td>
      </tr>
    {/each}
  </tbody>
</table>

<h2>Editor</h2>
<p>Active when the editor pane has focus.</p>
<table>
  <thead><tr><th>Keys</th><th>Action</th></tr></thead>
  <tbody>
    {#each editor as b (b.id)}
      <tr>
        <td>{#each chord(b) as k, i (i)}{#if i > 0} + {/if}<kbd>{k}</kbd>{/each}</td>
        <td>{b.description}</td>
      </tr>
    {/each}
  </tbody>
</table>

<h2>Mouse</h2>
<table>
  <thead><tr><th>Gesture</th><th>Action</th></tr></thead>
  <tbody>
    <tr><td><kbd>Ctrl</kbd> + click</td><td>Editor: go to declaration · Arrangement: reveal the source of an event</td></tr>
    <tr><td>Drag ruler</td><td>Scrub the play cursor</td></tr>
    <tr><td><kbd>Alt</kbd> + drag ruler</td><td>Set the loop region (<kbd>Esc</kbd> clears it)</td></tr>
    <tr><td>Right-click ruler</td><td>Add a marker here · right-click a marker to rename / delete</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>←</kbd> / <kbd>→</kbd></td><td>Arrangement: jump to the previous / next marker</td></tr>
    <tr><td>Wheel / <kbd>Shift</kbd> + wheel</td><td>Scroll the timeline horizontally</td></tr>
  </tbody>
</table>
