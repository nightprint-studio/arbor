<script lang="ts">
  /**
   * Hirundo titlebar — the shared `TitleBar` chrome:
   *   logo · hamburger · workspace switcher · [container tabs] · [gap] · environment · docs · palette · controls
   *
   * The environment sits at the right end: it is a fact about the whole window (every request goes to it),
   * and the type of the environment colours its selector.
   */
  import { BookOpen, Command, FolderOpen, Layers } from 'lucide-svelte';
  import ProductIcon from '$lib/components/shared/internal/ProductIcon.svelte';
  import TitleBar from '$lib/components/shared/ui/TitleBar.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import WindowControls from '$lib/components/shared/WindowControls.svelte';
  import WorkspaceTabs from '$lib/components/shared/internal/WorkspaceTabs.svelte';
  import { createNativeMenuPublisher } from '$lib/utils/native-menu';
  import { windowMenuItems } from '$lib/utils/window-menu';
  import { surfaceStore } from '$lib/stores/surfaces.svelte';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import EnvSelector from '../editor/EnvSelector.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import HirundoWorkspaceSwitcher from './HirundoWorkspaceSwitcher.svelte';

  const publishNativeMenu = createNativeMenuPublisher('Hirundo');

  const hamburgerMenu = $derived<DropdownItem[]>([
    { kind: 'item', id: 'open-project', label: 'Open project…', icon: FolderOpen, shortcut: 'Ctrl+Shift+O', onclick: () => hirundoUiStore.open({ kind: 'project-picker' }) },
    { kind: 'item', id: 'workspaces', label: 'Manage workspaces…', icon: Layers, onclick: () => hirundoUiStore.open({ kind: 'workspaces' }) },
    ...windowMenuItems(),
  ]);
</script>

<TitleBar
  logoTooltip="Hirundo — HTTP client"
  menu={hamburgerMenu}
  onNativeMenu={publishNativeMenu}
  nativeMenuEnabled={surfaceStore.hasFocus('hirundo')}
  menuWidth="250px"
>
  {#snippet logo()}
    <ProductIcon id="hirundo" size={22} />
  {/snippet}

  {#snippet center()}
    <WorkspaceTabs />
  {/snippet}

  {#snippet leading()}
    <HirundoWorkspaceSwitcher />
  {/snippet}

  {#snippet trailing()}
    <div class="htb-actions">
      {#if hirundoStore.project}<span class="htb-env"><EnvSelector /></span>{/if}
      <IconButton tooltip="Documentation" shortcut="F1" size={28} onclick={() => hirundoUiStore.toggleDocs()}><BookOpen size={16} /></IconButton>
      <IconButton tooltip="Command palette" shortcut="Ctrl+K" size={28} onclick={() => hirundoUiStore.togglePalette()}><Command size={16} /></IconButton>
    </div>
  {/snippet}

  {#snippet windowControls()}
    <WindowControls />
  {/snippet}
</TitleBar>

<style>
  .htb-actions { display: flex; align-items: center; gap: 2px; margin-right: 6px; }
  .htb-env { margin-right: 8px; }
</style>
