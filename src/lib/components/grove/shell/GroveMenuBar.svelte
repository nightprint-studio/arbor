<script lang="ts">
  /**
   * Grove hamburger menu — mirrors Arbor's MenuBar (custom panel with section
   * labels, click-away backdrop, fly-in, danger Exit). The file/project actions
   * are delegated to the shared `projectActions` store (same flows the titlebar
   * and keyboard shortcuts use); "Recent Projects…" opens a real Arbor modal.
   */
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { AlignJustify, FilePlus2, FolderOpen, FolderPlus, Clock, Download, Save, LogOut } from 'lucide-svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { animStore } from '$lib/stores/animations.svelte';
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';
  import RecentProjectsModal from './RecentProjectsModal.svelte';
  import { projectActions } from '../stores/project-actions.svelte';

  let open = $state(false);
  let recentOpen = $state(false);

  function close() { open = false; }
  async function exit() { close(); await getCurrentWindow().close(); }
  function openRecent() { close(); recentOpen = true; }
  /** Close the menu, then run a project action (opens its picker). */
  function run(action: () => void) { close(); action(); }
</script>

{#if open}
  <div class="backdrop" onclick={close} role="presentation"></div>
{/if}

<div class="menubar-root">
  <button
    class="hamburger"
    class:active={open}
    onclick={(e) => { e.stopPropagation(); open = !open; }}
    use:tooltip={'Main menu'}
    aria-label="Open main menu"
    aria-expanded={open}
  >
    <AlignJustify size={18} strokeWidth={2} />
  </button>

  {#if open}
    <div class="menu-panel" role="menu" transition:fly={{ y: -8, duration: animStore.dBase, easing: cubicOut }}>
      <div class="menu-section-label">File</div>
      <button class="menu-item" role="menuitem" onclick={() => run(projectActions.newProject)}>
        <FolderPlus size={13} /><span>New Project…</span><kbd class="menu-kbd">Ctrl+Shift+N</kbd>
      </button>
      <button class="menu-item" role="menuitem" onclick={() => run(projectActions.openProject)}>
        <FolderOpen size={13} /><span>Open Project…</span><kbd class="menu-kbd">Ctrl+O</kbd>
      </button>
      <button class="menu-item" role="menuitem" onclick={() => run(projectActions.openFile)}>
        <FilePlus2 size={13} /><span>Open File…</span><kbd class="menu-kbd">Ctrl+Shift+O</kbd>
      </button>
      <button class="menu-item" role="menuitem" onclick={openRecent}>
        <Clock size={13} /><span>Recent Projects…</span>
      </button>

      <div class="menu-sep"></div>

      <div class="menu-section-label">Project</div>
      <button class="menu-item" role="menuitem" onclick={() => run(projectActions.save)}>
        <Save size={13} /><span>Save</span><kbd class="menu-kbd">Ctrl+S</kbd>
      </button>
      <button class="menu-item" role="menuitem" onclick={() => run(projectActions.exportWav)}>
        <Download size={13} /><span>Export to WAV…</span><kbd class="menu-kbd">Ctrl+Shift+R</kbd>
      </button>

      <div class="menu-sep"></div>

      <button class="menu-item danger" role="menuitem" onclick={exit}>
        <LogOut size={13} /><span>Close Window</span>
      </button>
    </div>
  {/if}
</div>

{#if recentOpen}
  <RecentProjectsModal onClose={() => recentOpen = false} />
{/if}

<style>
  .backdrop { position: fixed; inset: 0; z-index: var(--z-menu); }
  .menubar-root { position: relative; z-index: calc(var(--z-menu) + 1); flex-shrink: 0; display: flex; align-items: center; }

  .hamburger {
    display: flex; align-items: center; justify-content: center;
    width: 34px; height: 34px;
    background: transparent; border: none; border-radius: var(--radius-sm);
    cursor: pointer; color: var(--text-secondary);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .hamburger:hover, .hamburger.active { background: var(--bg-overlay); color: var(--text-primary); }

  .menu-panel {
    position: absolute; top: calc(100% + 4px); left: 0;
    min-width: 240px;
    background: var(--bg-elevated); border: 1px solid var(--border);
    border-radius: var(--radius-md); box-shadow: var(--shadow-popup);
    padding: 4px 0; z-index: calc(var(--z-menu) + 1);
  }
  .menu-section-label {
    padding: 6px 12px 2px;
    font-size: 10px; font-weight: 700; letter-spacing: 0.6px; text-transform: uppercase;
    color: var(--text-muted); pointer-events: none;
  }
  .menu-item {
    display: flex; align-items: center; gap: 8px;
    width: 100%; padding: 6px 12px;
    background: transparent; border: none; cursor: pointer;
    font-family: var(--font-ui-sans); font-size: var(--font-size-sm);
    color: var(--text-secondary); text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .menu-item:hover { background: var(--bg-hover); color: var(--text-primary); }
  .menu-item.danger:hover { background: var(--error-subtle); color: var(--error); }
  .menu-kbd {
    margin-left: auto; padding-left: 12px;
    font-family: var(--font-code); font-size: 10.5px; letter-spacing: 0.2px;
    color: var(--text-muted); white-space: nowrap;
  }
  .menu-sep { height: 1px; background: var(--border); margin: 3px 0; }
</style>
