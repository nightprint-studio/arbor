<script lang="ts">
  /**
   * Tyto titlebar — composes the shared `TitleBar`: logo · hamburger (About /
   * Close) · a reduced mode switcher (with explicit key hints) · a centered
   * capture cluster (IntelliJ/merula-style) with the primary action as a
   * LABELLED, coloured pill · documentation button · settings gear (a menu with
   * Settings / Shortcuts / Theme, like Arbor & merula) · window controls.
   */
  import { Camera, Video, Circle, Square, Settings, Keyboard, Info, Palette, LogOut, Minimize2 } from 'lucide-svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import TitleBar from '$lib/components/shared/ui/TitleBar.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import ArborLogo from '$lib/components/shared/internal/ArborLogo.svelte';
  import WindowControls from '$lib/components/shared/WindowControls.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  // Titlebar sits at the very top — tooltips fly downward so they aren't clipped.
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';
  import { recorderStore, formatDuration, type CaptureMode } from '$lib/stores/tyto/recorder.svelte';
  import { tytoUiStore } from '$lib/stores/tyto/ui.svelte';
  import { themeStore } from '$lib/stores/theme.svelte';

  const ready = $derived(recorderStore.targetReady);
  const notReadyTip = 'Pick a capture region first';
  const target = $derived(recorderStore.currentTargetLabel);

  // Reduced mode switcher, mirrored from the body. Each carries its digit hint
  // (Ctrl+1 / Ctrl+2) so the shortcut is visible right on the tab.
  const modeTabs: TabItem[] = [
    { id: 'record',     label: 'Recording',  icon: Video,  data: { hint: '1' } },
    { id: 'screenshot', label: 'Screenshot', icon: Camera, data: { hint: '2' } },
  ];

  // Hamburger — light: identity + about + close (settings live in the gear).
  const hamburgerMenu: DropdownItem[] = [
    { kind: 'item', id: 'compact', label: 'Compact mode', icon: Minimize2, shortcut: 'Ctrl+Shift+C', onclick: () => tytoUiStore.setCompact(true) },
    { kind: 'item', id: 'about', label: 'About Tyto…', icon: Info, onclick: () => tytoUiStore.openAbout() },
    { kind: 'separator' },
    { kind: 'item', id: 'close', label: 'Close Window', icon: LogOut, danger: true, onclick: () => { void getCurrentWindow().close(); } },
  ];

  // Theme submenu — every available theme, single-select with a check on the
  // active one. Keyboard-navigable through the shared Dropdown flyout.
  const themeItems = $derived<DropdownItem[]>(
    themeStore.allThemes.map((t) => ({
      kind: 'item',
      id: `theme:${t.id}`,
      label: t.name,
      active: themeStore.activeTheme.id === t.id,
      onclick: () => void themeStore.setActive(t.id),
    })),
  );

  // Settings gear — a menu (Arbor/merula style): Settings, Shortcuts, Theme.
  const settingsMenu = $derived<DropdownItem[]>([
    { kind: 'item', id: 'settings',  label: 'Settings…',           icon: Settings, shortcut: 'Ctrl+,',   onclick: () => tytoUiStore.openSettings() },
    { kind: 'item', id: 'shortcuts', label: 'Keyboard Shortcuts…', icon: Keyboard, shortcut: 'Shift+F1', onclick: () => tytoUiStore.openShortcuts() },
    { kind: 'separator' },
    { kind: 'submenu', id: 'theme', label: 'Theme', icon: Palette, items: themeItems },
  ]);
</script>

<TitleBar
  logoTooltip="Tyto — screen recorder"
  menu={hamburgerMenu}
  menuWidth="220px"
  docs={{ active: tytoUiStore.docsOpen, tooltip: { content: 'Documentation', shortcut: 'F1' }, onclick: () => tytoUiStore.toggleDocs() }}
  settings={{ menu: settingsMenu, menuWidth: '230px', tooltip: 'Settings' }}
>
  {#snippet logo()}
    <ArborLogo size={22} />
  {/snippet}

  <!-- Reduced mode switcher, right after the hamburger. -->
  {#snippet leading()}
    <div class="mode-switch">
      <Tabs
        variant="pill"
        size="sm"
        value={recorderStore.mode}
        items={modeTabs}
        onSelect={(id) => recorderStore.setMode(id as CaptureMode)}
        ariaLabel="Capture mode"
      >
        {#snippet itemContent({ item, active })}
          {@const Icon = item.icon}
          <Icon size={12} />
          <span class="mode-label">{item.label}</span>
          <span class="mode-digit" class:on={active}>{item.data.hint}</span>
        {/snippet}
      </Tabs>
    </div>
  {/snippet}

  <!-- Centered capture cluster (offset from the window controls, like merula). -->
  {#snippet trailing()}
    <div class="cap-cluster">
      {#if recorderStore.mode === 'record'}
        {#if recorderStore.recording}
          <button
            class="cap-pill live"
            onclick={() => recorderStore.stopRecording()}
            use:tooltip={{ content: 'Stop recording', shortcut: 'Ctrl+Enter' }}
            aria-label="Stop recording"
          >
            <Square size={12} fill="currentColor" />
            <span class="cap-label">Stop</span>
            <span class="cap-time">{formatDuration(recorderStore.elapsedMs)}</span>
          </button>
        {:else}
          <button
            class="cap-pill record"
            disabled={!ready}
            onclick={() => recorderStore.startRecording()}
            use:tooltip={ready ? { content: `Start recording — ${target}`, shortcut: 'Ctrl+Enter' } : notReadyTip}
            aria-label="Start recording"
          >
            <span class="rec-dot"></span>
            <span class="cap-label">Record</span>
          </button>
        {/if}
        <button
          class="cap-ghost"
          disabled={!ready}
          onclick={() => recorderStore.takeScreenshot()}
          use:tooltip={ready ? { content: 'Quick screenshot' } : notReadyTip}
          aria-label="Quick screenshot"
        ><Camera size={15} /></button>
      {:else}
        <button
          class="cap-pill shot"
          disabled={!ready}
          onclick={() => recorderStore.takeScreenshot()}
          use:tooltip={ready ? { content: `Take screenshot — ${target}`, shortcut: 'Ctrl+Enter' } : notReadyTip}
          aria-label="Take screenshot"
        >
          <Camera size={14} />
          <span class="cap-label">Screenshot</span>
        </button>
        <button
          class="cap-ghost record"
          disabled={!ready}
          onclick={() => recorderStore.startRecording()}
          use:tooltip={ready ? { content: 'Start recording' } : notReadyTip}
          aria-label="Start recording"
        ><Circle size={14} fill="currentColor" /></button>
      {/if}
    </div>
  {/snippet}

  {#snippet windowControls()}
    <WindowControls />
  {/snippet}
</TitleBar>

<style>
  /* Reduced mode switcher in the leading slot — a soft trough so the two pills
     read as one segmented control. */
  .mode-switch {
    display: flex;
    align-items: center;
    margin-left: 2px;
    padding: 3px;
    background: var(--bg-input);
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    -webkit-app-region: no-drag;
  }
  /* Fully-round the inner pills so the active (accent) one matches the round
     trough — its outer edge reads as a smooth capsule, not a boxed corner. */
  .mode-switch :global(.tabs-pill .tabs-strip) { gap: 3px; padding: 0; }
  .mode-switch :global(.tabs-pill .tabs-tab) { border-radius: 999px; padding: 3px 8px 3px 11px; gap: 5px; }
  .mode-switch :global(.tabs-pill .tabs-tab.tab-active) {
    background: var(--accent);
    color: var(--text-on-accent, #fff);
    box-shadow: 0 1px 6px color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .mode-label { line-height: 1; }
  /* Digit hint (Ctrl+1 / Ctrl+2) — a compact chip that adapts to the active
     accent pill instead of a boxed <kbd> that would clash on the fill. */
  .mode-digit {
    display: inline-flex; align-items: center; justify-content: center;
    min-width: 15px; height: 15px; padding: 0 3px;
    border-radius: 5px;
    font-size: 9.5px; font-weight: 700; line-height: 1;
    font-variant-numeric: tabular-nums;
    color: var(--text-muted);
    background: color-mix(in srgb, var(--text-muted) 16%, transparent);
  }
  .mode-digit.on {
    color: var(--text-on-accent, #fff);
    background: rgba(255, 255, 255, 0.24);
  }

  /* Offset from the window controls so the cluster reads as centred, not glued
     to the traffic-light buttons (merula does the same with its transport). */
  .cap-cluster {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-right: 76px;
    -webkit-app-region: no-drag;
  }

  /* Primary action — a LABELLED pill. Rounded, coloured, with text: unmistakably
     a button, never confusable with the round mac close dot. */
  .cap-pill {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 28px;
    padding: 0 13px;
    border: 1px solid transparent;
    border-radius: 999px;
    font-size: 12.5px;
    font-weight: 650;
    cursor: pointer;
    transition: filter var(--transition-fast), background var(--transition-fast),
                box-shadow var(--transition-fast), transform var(--transition-fast);
  }
  .cap-pill:hover:not(:disabled) { transform: translateY(-1px); }
  .cap-pill:active:not(:disabled) { transform: translateY(0); }
  .cap-pill:disabled { opacity: 0.45; cursor: default; }
  .cap-label { line-height: 1; }

  /* Record (idle) — accent-red pill with a solid dot. */
  .cap-pill.record {
    background: color-mix(in srgb, var(--error) 18%, transparent);
    color: var(--error);
    border-color: color-mix(in srgb, var(--error) 45%, transparent);
  }
  .cap-pill.record:hover:not(:disabled) { background: color-mix(in srgb, var(--error) 28%, transparent); }
  .rec-dot { width: 9px; height: 9px; border-radius: 50%; background: var(--error); box-shadow: 0 0 0 3px color-mix(in srgb, var(--error) 25%, transparent); }

  /* Recording (live) — filled red, pulsing, with the running time. */
  .cap-pill.live {
    background: var(--error);
    color: #fff;
    box-shadow: 0 2px 10px color-mix(in srgb, var(--error) 45%, transparent);
    animation: cap-pulse 1.6s ease-in-out infinite;
  }
  .cap-pill.live:hover { filter: brightness(1.08); }
  .cap-time {
    font-variant-numeric: tabular-nums;
    background: rgba(0, 0, 0, 0.22);
    padding: 1px 6px;
    border-radius: 999px;
    font-weight: 700;
  }
  @keyframes cap-pulse {
    0%, 100% { box-shadow: 0 2px 10px color-mix(in srgb, var(--error) 30%, transparent); }
    50%      { box-shadow: 0 2px 16px color-mix(in srgb, var(--error) 60%, transparent); }
  }

  /* Screenshot — accent-filled pill. */
  .cap-pill.shot {
    background: var(--accent);
    color: var(--text-on-accent, #fff);
    box-shadow: 0 2px 10px color-mix(in srgb, var(--accent) 40%, transparent);
  }
  .cap-pill.shot:hover:not(:disabled) { filter: brightness(1.08); }

  /* Secondary quick-action for the OTHER mode — subtle ghost icon button. */
  .cap-ghost {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 28px;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .cap-ghost:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); border-color: var(--border); }
  .cap-ghost.record { color: var(--error); }
  .cap-ghost.record:hover:not(:disabled) { background: color-mix(in srgb, var(--error) 14%, transparent); border-color: color-mix(in srgb, var(--error) 40%, transparent); }
  .cap-ghost:disabled { opacity: 0.4; cursor: default; }
</style>
