<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import AppShell from '$lib/components/layout/AppShell.svelte';
  import ExplorerWindow from '$lib/components/layout/ExplorerWindow.svelte';
  import NemusWindow from '$lib/components/layout/NemusWindow.svelte';
  import DragOverlay from '$lib/components/layout/DragOverlay.svelte';

  // The dedicated File Explorer window (opened via the global Ctrl+Shift+E
  // shortcut) loads this same index.html. We branch on the window label so it
  // mounts ONLY the standalone explorer shell, never the full Arbor app.
  // The explorer window may be the canonical "explorer" or, when the user opts
  // into always-new windows, "explorer-2", "explorer-3", … — match the prefix.
  // The "nemus" window hosts the music live-coding DAW shell (NemusWindow).
  // The "drag-overlay" window hosts only the cross-window drag ghost.
  let isExplorer = false;
  let isNemus = false;
  let isDragOverlay = false;
  try {
    const label = getCurrentWindow().label;
    isExplorer = label === 'explorer' || label.startsWith('explorer-');
    isNemus = label === 'nemus' || label.startsWith('nemus-');
    isDragOverlay = label === 'drag-overlay';
  } catch { /* non-Tauri / SSR */ }
</script>

{#if isDragOverlay}
  <DragOverlay />
{:else if isExplorer}
  <ExplorerWindow />
{:else if isNemus}
  <NemusWindow />
{:else}
  <AppShell />
{/if}
