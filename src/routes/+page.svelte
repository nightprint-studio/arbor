<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import AppShell from '$lib/components/layout/AppShell.svelte';
  import ExplorerWindow from '$lib/components/layout/ExplorerWindow.svelte';
  import GroveWindow from '$lib/components/layout/GroveWindow.svelte';
  import DragOverlay from '$lib/components/layout/DragOverlay.svelte';

  // The dedicated File Explorer window (opened via the global Ctrl+Shift+E
  // shortcut) loads this same index.html. We branch on the window label so it
  // mounts ONLY the standalone explorer shell, never the full Arbor app.
  // The explorer window may be the canonical "explorer" or, when the user opts
  // into always-new windows, "explorer-2", "explorer-3", … — match the prefix.
  // The "grove" window hosts the music live-coding DAW shell (GroveWindow).
  // The "drag-overlay" window hosts only the cross-window drag ghost.
  let isExplorer = false;
  let isGrove = false;
  let isDragOverlay = false;
  try {
    const label = getCurrentWindow().label;
    isExplorer = label === 'explorer' || label.startsWith('explorer-');
    isGrove = label === 'grove' || label.startsWith('grove-');
    isDragOverlay = label === 'drag-overlay';
  } catch { /* non-Tauri / SSR */ }
</script>

{#if isDragOverlay}
  <DragOverlay />
{:else if isExplorer}
  <ExplorerWindow />
{:else if isGrove}
  <GroveWindow />
{:else}
  <AppShell />
{/if}
