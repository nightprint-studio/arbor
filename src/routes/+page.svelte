<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import AppShell from '$lib/components/layout/AppShell.svelte';
  import ExplorerWindow from '$lib/components/layout/ExplorerWindow.svelte';
  import NemusWindow from '$lib/components/layout/NemusWindow.svelte';
  import LauncherWindow from '$lib/components/layout/LauncherWindow.svelte';
  import DragOverlay from '$lib/components/layout/DragOverlay.svelte';

  // Every window loads this same index.html; we branch on the window label to
  // mount the right shell:
  //  • main / launcher  → the Canopy launcher (entry-point home, Toolbox-like).
  //  • corvus           → the Git AppShell (the Corvus product window).
  //  • explorer[-N]      → the standalone File Explorer (Ctrl+Shift+E).
  //  • nemus[-N]         → the music live-coding DAW shell.
  //  • drag-overlay      → only the cross-window drag ghost.
  // Unknown labels fall back to the Git AppShell.
  let isExplorer = false;
  let isNemus = false;
  let isDragOverlay = false;
  let isCorvus = false;
  let isLauncher = false;
  try {
    const label = getCurrentWindow().label;
    isExplorer = label === 'explorer' || label.startsWith('explorer-');
    isNemus = label === 'nemus' || label.startsWith('nemus-');
    isDragOverlay = label === 'drag-overlay';
    isCorvus = label === 'corvus' || label.startsWith('corvus-');
    isLauncher = label === 'main' || label === 'launcher';
  } catch { /* non-Tauri / SSR */ }
</script>

{#if isDragOverlay}
  <DragOverlay />
{:else if isExplorer}
  <ExplorerWindow />
{:else if isNemus}
  <NemusWindow />
{:else if isCorvus}
  <AppShell />
{:else if isLauncher}
  <LauncherWindow />
{:else}
  <AppShell />
{/if}
