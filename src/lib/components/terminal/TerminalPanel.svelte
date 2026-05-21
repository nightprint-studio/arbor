<script lang="ts">
  import { Plus, ChevronDown, TerminalSquare, Settings as SettingsIcon } from 'lucide-svelte';
  import { terminalCreate, terminalClose } from '$lib/ipc/terminal';
  import { terminalStore } from '$lib/stores/terminal.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import TerminalInstance from './TerminalInstance.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let showShellMenu = $state(false);
  let creating      = $state(false);
  let shellPickerEl = $state<HTMLButtonElement | null>(null);
  let dropdownTop   = $state(0);
  let dropdownRight = $state(0);

  function currentCwd(): string | undefined {
    const tab = tabsStore.activeTab;
    return tab?.path ?? undefined;
  }

  const pickerOptions = $derived(terminalStore.pickerOptions());

  async function openTerminal(shellId?: string) {
    if (creating) return;
    creating = true;
    showShellMenu = false;
    try {
      const info = await terminalCreate({ shell: shellId, cwd: currentCwd() });
      terminalStore.addTab(info.id, info.shell, info.cwd);
    } catch (err) {
      uiStore.showToast(`Failed to open terminal: ${err}`, 'error');
    } finally {
      creating = false;
    }
  }

  function openTerminalSettings() {
    showShellMenu = false;
    uiStore.setPanel('settings');
  }

  async function closeTab(id: string) {
    terminalStore.removeTab(id);
    try { await terminalClose(id); } catch { /* already gone */ }
  }

  // Close the shell dropdown on outside click
  function onWindowClick(e: MouseEvent) {
    if (!(e.target as Element).closest('.shell-menu-anchor')) {
      showShellMenu = false;
    }
  }

  // Expose a method to focus the current terminal (called from AppShell keybinding)
  export function focusActive() {
    // The active TerminalInstance's xterm will auto-focus via the $effect.
  }

  // ── Tab items for the shared widget ────────────────────────────────────
  // The terminal-specific bits (icon + project chip) render via the
  // `itemContent` snippet so we still own that look while the wrapper /
  // close X / drag affordance stay shared with the rest of the app.
  const items = $derived<TabItem[]>(
    terminalStore.tabs.map(t => ({
      id:       t.id,
      label:    t.title,
      title:    `${t.title} — ${t.cwd}`,
      closable: true,
      data:     t,
    })),
  );
</script>

<svelte:window onclick={onWindowClick} />

<div class="terminal-panel">
  <!-- ── Standardized bottom-panel header ─────────────────────────────────
       Title + global terminal actions (new tab + shell picker). The X
       close button is provided by `BottomPanelHeader` itself, so we no
       longer carry our own close button in the tab bar. -->
  <BottomPanelHeader title="Terminal">
    {#snippet icon()}<TerminalSquare size={14} />{/snippet}
    {#snippet children()}
      {#if terminalStore.tabs.length > 0}
        <div class="terminal-tabs">
          <Tabs
            {items}
            value={terminalStore.activeId}
            variant="panel"
            size="md"
            closable
            ariaLabel="Terminal sessions"
            onSelect={(id) => terminalStore.setActive(id)}
            onClose={(id) => closeTab(id)}
          >
            {#snippet itemContent({ item })}
              {@const tab = item.data as (typeof terminalStore.tabs)[number]}
              <TerminalSquare size={12} class="tab-icon" />
              <span class="tab-label">{tab.title}</span>
              {#if tab.cwd}
                {@const projName = tab.cwd.split(/[/\\]/).filter(Boolean).pop()}
                {#if projName}
                  <span class="tab-project">{projName}</span>
                {/if}
              {/if}
            {/snippet}
          </Tabs>
        </div>
      {/if}
    {/snippet}
    {#snippet actions()}
      <div class="shell-menu-anchor">
        <button
          class="ps-btn"
          use:tooltip={'New terminal'}
          disabled={creating}
          onclick={() => openTerminal()}
          aria-label="New terminal"
        >
          <Plus size={13} />
        </button>
        <button
          class="ps-btn"
          use:tooltip={'Choose shell'}
          bind:this={shellPickerEl}
          onclick={(e) => {
            e.stopPropagation();
            if (!showShellMenu && shellPickerEl) {
              const r = shellPickerEl.getBoundingClientRect();
              dropdownTop   = r.bottom + 4;
              dropdownRight = window.innerWidth - r.right;
            }
            showShellMenu = !showShellMenu;
          }}
          aria-label="Choose shell"
          aria-haspopup="listbox"
          aria-expanded={showShellMenu}
        >
          <ChevronDown size={11} />
        </button>
      </div>
    {/snippet}
  </BottomPanelHeader>

  <!-- ── Shell dropdown (fixed so it escapes overflow:hidden parents) ───── -->
  {#if showShellMenu}
    <div
      class="shell-dropdown"
      role="listbox"
      aria-label="Select shell"
      style="top: {dropdownTop}px; right: {dropdownRight}px"
    >
      {#if pickerOptions.length === 0}
        <div class="shell-empty">
          {terminalStore.detectionDone
            ? 'No shells detected.'
            : 'Detecting shells…'}
        </div>
      {:else}
        {#each pickerOptions as s (s.id)}
          <button
            role="option"
            aria-selected="false"
            class="shell-option"
            class:custom={s.custom}
            onclick={() => openTerminal(s.id)}
          >
            <TerminalSquare size={12} />
            <span class="shell-label">{s.name}</span>
            {#if s.custom}<span class="custom-pill">custom</span>{/if}
          </button>
        {/each}
      {/if}
      <div class="shell-divider"></div>
      <button class="shell-option settings-link" onclick={openTerminalSettings}>
        <SettingsIcon size={12} />
        Terminal Settings…
      </button>
    </div>
  {/if}

  <!-- ── Terminal body ───────────────────────────────────────────────────── -->
  <div class="terminal-body">
    {#if terminalStore.tabs.length === 0}
      <!-- Empty state: auto-open a terminal -->
      <div class="empty-state">
        <TerminalSquare size={28} />
        <p>No terminal open</p>
        <button class="open-btn" onclick={() => openTerminal()}>
          <Plus size={12} /> New Terminal
        </button>
      </div>
    {:else}
      {#each terminalStore.tabs as tab (tab.id)}
        <TerminalInstance
          id={tab.id}
          active={tab.id === terminalStore.activeId}
        />
      {/each}
    {/if}
  </div>
</div>

<style>
  /* ── Panel root ──────────────────────────────────────────────────────────── */
  .terminal-panel {
    display:        flex;
    flex-direction: column;
    height:         100%;
    width:          100%;
    overflow:       hidden;
    background:     var(--terminal-bg);
  }

  /* ── Tabs strip embedded in BottomPanelHeader ───────────────────────────
     The tab strip lives inside `.bp-header`, so the row's background,
     height (34px) and bottom border come from BottomPanelHeader itself —
     we only need to absorb the available width and override the active
     tab background so it visually merges with the terminal body below.
     Active background stays `--terminal-bg` so the active tab "extends"
     into the terminal area below the header. */
  .terminal-tabs {
    /* High flex-grow so the strip dominates `.bp-spacer` (flex:1) and
       pushes the action buttons all the way to the right. */
    flex: 100 1 0;
    min-width: 0;
    align-self: stretch;       /* fill the 34px header height */
    display: flex;
    align-items: stretch;
    overflow: hidden;
    /* Visible vertical separator between the "TERMINAL" title and the
       tab strip — pronounced enough to read as a section divider, not
       just a hairline. */
    margin-left: 10px;
    padding-left: 10px;
    border-left: 1px solid var(--border);
  }
  .terminal-tabs :global(.tabs) {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
  }
  .terminal-tabs :global(.tabs-strip) {
    overflow-x: auto;
    scrollbar-width: none;
    flex: 0 1 auto;
  }
  .terminal-tabs :global(.tabs-strip)::-webkit-scrollbar { display: none; }
  .terminal-tabs :global(.tabs-tab) {
    height: 34px;
    padding: 0 10px;
    border-right: 1px solid var(--border-subtle);
    border-radius: 0;
    color: var(--text-muted);
    max-width: none;
  }
  .terminal-tabs :global(.tabs-tab:hover:not(.tab-disabled):not(.tab-active)) {
    color: var(--text-secondary);
  }
  .terminal-tabs :global(.tabs-tab.tab-active) {
    background: var(--terminal-bg);
    color: var(--text-primary);
  }
  /* Replace the panel variant's animated ::after with a static accent
     underline — the terminal tab strip predates the animation and the
     existing UX expects an instant indicator. */
  .terminal-tabs :global(.tabs-tab.tab-active::after) { transform: none; height: 2px; }
  /* Per-tab close uses terminal-red on hover instead of the global error. */
  .terminal-tabs :global(.tabs-tab .tab-close:hover) {
    background: color-mix(in srgb, var(--terminal-red) 20%, transparent);
    color: var(--terminal-bright-red);
  }

  :global(.terminal-tabs .tab-icon) { color: var(--text-muted); flex-shrink: 0; }

  .tab-label {
    max-width: 80px;
    overflow:  hidden;
    text-overflow: ellipsis;
  }

  .tab-project {
    font-size: 10px;
    color: var(--terminal-bright-blue);
    background: color-mix(in srgb, var(--terminal-blue) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--terminal-blue) 28%, transparent);
    border-radius: var(--radius-sm);
    padding: 1px 5px;
    max-width: 80px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex-shrink: 0;
    line-height: 1.4;
  }

  /* New-terminal + shell-picker live in `BottomPanelHeader` actions slot;
     keeping the anchor wrapper for the dropdown's `getBoundingClientRect`. */
  .shell-menu-anchor { display: flex; gap: 0; }
  /* Pair the two buttons visually as a split-button. */
  .shell-menu-anchor :global(.ps-btn:first-child) {
    border-radius: var(--radius-sm) 0 0 var(--radius-sm);
  }
  .shell-menu-anchor :global(.ps-btn:last-child) {
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    width: 16px;
  }

  /* ── Shell dropdown (fixed positioning escapes overflow:hidden) ─────────── */
  .shell-dropdown {
    position:   fixed;
    z-index:    var(--z-top);
    background: var(--bg-overlay);
    border:     1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: 0 8px 24px rgba(0,0,0,0.5);
    padding:    4px;
    min-width:  160px;
    animation:  dropIn var(--anim-dur-base) ease;
  }

  @keyframes dropIn {
    from { opacity: 0; transform: translateY(-4px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  .shell-option {
    display:     flex;
    align-items: center;
    gap:         8px;
    width:       100%;
    padding:     6px 10px;
    border:      none;
    background:  transparent;
    color:       var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size:   var(--font-size-sm);
    text-align:  left;
    cursor:      pointer;
    border-radius: var(--radius-sm);
    transition:  background var(--transition-fast), color var(--transition-fast);
  }
  .shell-option:hover { background: var(--bg-hover); color: var(--text-primary); }
  .shell-label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .custom-pill {
    font-size: 9px; font-weight: 600;
    padding: 1px 5px; border-radius: var(--radius-sm);
    background: var(--accent-subtle); color: var(--accent);
    text-transform: uppercase; letter-spacing: 0.04em;
  }
  .shell-divider {
    height: 1px; background: var(--border-subtle);
    margin: 4px 0;
  }
  .shell-option.settings-link { color: var(--text-muted); font-size: 11.5px; }
  .shell-empty {
    padding: 8px 10px;
    font-size: 11.5px; font-style: italic;
    color: var(--text-muted);
  }

  /* ── Terminal body ───────────────────────────────────────────────────────── */
  .terminal-body {
    flex:     1;
    min-height: 0;
    position: relative;
    overflow: hidden;
  }

  /* ── Empty state ─────────────────────────────────────────────────────────── */
  .empty-state {
    display:         flex;
    flex-direction:  column;
    align-items:     center;
    justify-content: center;
    height:          100%;
    gap:             12px;
    color:           var(--text-disabled);
    font-family:     var(--font-ui-sans);
    font-size:       var(--font-size-sm);
  }
  .open-btn {
    display:         flex;
    align-items:     center;
    gap:             5px;
    padding:         6px 14px;
    border:          1px solid var(--border);
    background:      var(--bg-elevated);
    color:           var(--text-secondary);
    border-radius:   var(--radius-md);
    font-family:     var(--font-ui-sans);
    font-size:       var(--font-size-sm);
    cursor:          pointer;
    transition:      background var(--transition-fast), color var(--transition-fast);
  }
  .open-btn:hover { background: var(--bg-hover); color: var(--text-primary); border-color: var(--border); }
</style>
