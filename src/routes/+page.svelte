<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import type { Component } from 'svelte';
  import { signalWindowReady } from '$lib/ipc/window';
  import { applyOsAttribute, watchFullscreen } from '$lib/utils/platform';

  // Stamp `<html data-os>` before any shell mounts so title-bar chrome reserves
  // the macOS traffic-light gutter on its very first paint. Runs in every window.
  applyOsAttribute();
  // Keep `<html data-fullscreen>` in sync so headers reclaim the gutter in
  // fullscreen (macOS hides the traffic lights there). No-op off macOS.
  $effect(() => watchFullscreen());

  // Every window loads this same index.html; we branch on the window label to
  // mount the right shell:
  //  • main / launcher  → the Canopy launcher (entry-point home, Toolbox-like).
  //  • corvus           → the Git AppShell (the Corvus product window).
  //  • explorer[-N]      → the standalone File Explorer (Ctrl+Shift+E).
  //  • merula[-N]         → the music live-coding DAW shell.
  //  • tyto              → the screen-recorder control panel (Ctrl+Shift+R).
  //  • picus[-N]         → the SQL studio (databases + SQL script repository).
  //  • workspace         → the tabbed container hosting several products.
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
    if (label === 'picus' || label.startsWith('picus-')) return import('$lib/components/picus/PicusWindow.svelte');
    if (label === 'workspace') return import('$lib/components/shared/workspace/WorkspaceContainer.svelte');
    if (label === 'main' || label === 'launcher') return import('$lib/components/launcher/LauncherWindow.svelte');
    // corvus + any unknown label → the Git AppShell.
    return import('$lib/components/corvus/AppShell.svelte');
  };

  let Shell = $state<Component | null>(null);
  loadShell().then((m) => { Shell = m.default; });

  // Chromeless surfaces owned by another window: the recording HUD and the drag
  // ghost. They get no window-level chrome of their own — no switcher, and they
  // never appear in one either (the shell's `SurfaceKind::Overlay`).
  const OVERLAY_LABELS = ['drag-overlay', 'tyto-hud'];
  const IS_OVERLAY = OVERLAY_LABELS.includes(label);

  // Cross-product overlays (window switcher, credentials dialog) ride alongside
  // EVERY real window rather than inside a single product's shell — see
  // `GlobalOverlays`. Dynamic import for the same reason the shell uses one: the
  // chromeless overlay windows must not pay for it.
  let Overlays = $state<Component | null>(null);
  if (!IS_OVERLAY) {
    import('$lib/components/shared/GlobalOverlays.svelte').then((m) => { Overlays = m.default; });
  }

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

{#if Overlays}
  <Overlays />
{/if}
