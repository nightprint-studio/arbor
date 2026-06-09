<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import AppShell from '$lib/components/layout/AppShell.svelte';
  import ExplorerWindow from '$lib/components/layout/ExplorerWindow.svelte';

  // The dedicated File Explorer window (opened via the global Ctrl+Shift+E
  // shortcut) loads this same index.html. We branch on the window label so it
  // mounts ONLY the standalone explorer shell, never the full Arbor app.
  let isExplorer = false;
  try { isExplorer = getCurrentWindow().label === 'explorer'; } catch { /* non-Tauri / SSR */ }
</script>

{#if isExplorer}
  <ExplorerWindow />
{:else}
  <AppShell />
{/if}
