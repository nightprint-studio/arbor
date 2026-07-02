<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import type { Component } from 'svelte';
  import { signalWindowReady } from '$lib/ipc/window';

  // Every window loads this same index.html; we branch on the window label to
  // mount the right shell:
  //  • main / launcher  → the Canopy launcher (entry-point home, Toolbox-like).
  //  • corvus           → the Git AppShell (the Corvus product window).
  //  • explorer[-N]      → the standalone File Explorer (Ctrl+Shift+E).
  //  • merula[-N]         → the music live-coding DAW shell.
  //  • tyto              → the screen-recorder control panel (Ctrl+Shift+R).
  //  • drag-overlay      → only the cross-window drag ghost.
  // Unknown labels fall back to the Git AppShell.
  //
  // The shell is loaded with a DYNAMIC import, not a static one, so each window
  // pulls only its own chunk. A static `import AppShell` here would execute
  // AppShell's whole module graph in EVERY window — including stores that fire an
  // IPC load at import time (e.g. the git graph/issues config) — so the explorer
  // / launcher / merula windows would all hit the corvus backend they never use.
  // Per-window code-splitting keeps each product's side effects in its own window.
  let label = '';
  try { label = getCurrentWindow().label; } catch { /* non-Tauri / SSR */ }

  const loadShell = (): Promise<{ default: Component }> => {
    if (label === 'drag-overlay') return import('$lib/components/shared/DragOverlay.svelte');
    if (label === 'explorer' || label.startsWith('explorer-')) return import('$lib/components/sitta/ExplorerWindow.svelte');
    if (label === 'merula' || label.startsWith('merula-')) return import('$lib/components/merula/MerulaWindow.svelte');
    if (label === 'tyto-hud') return import('$lib/components/tyto/RecordingHud.svelte');
    if (label === 'tyto' || label.startsWith('tyto-')) return import('$lib/components/tyto/TytoWindow.svelte');
    if (label === 'bennu' || label.startsWith('bennu-')) return import('$lib/components/bennu/BennuWindow.svelte');
    if (label === 'main' || label === 'launcher') return import('$lib/components/launcher/LauncherWindow.svelte');
    // corvus + any unknown label → the Git AppShell.
    return import('$lib/components/corvus/AppShell.svelte');
  };

  let Shell = $state<Component | null>(null);
  loadShell().then((m) => { Shell = m.default; });

  // Anti-white-flash: every launcher/product window is built HIDDEN by the shell and
  // revealed only once painted — an opaque WebView2 window flashes its white default
  // page during load otherwise. Signal readiness after the shell mounts + two frames
  // (so the first real frame is on screen). Excluded — these own their reveal timing:
  //  • drag-overlay   — shown/hidden per-drag, never a persistent reveal.
  const OWNS_REVEAL = label === 'drag-overlay';
  $effect(() => {
    if (!Shell || OWNS_REVEAL) return;
    requestAnimationFrame(() => requestAnimationFrame(() => void signalWindowReady().catch(() => {})));
  });
</script>

{#if Shell}
  <Shell />
{/if}
