<script lang="ts">
  /**
   * Garrulus titlebar — composes the shared `TitleBar` chrome:
   *   logo · hamburger · vault chip · [container tabs] · [gap] · sync · window controls
   *
   * Two pieces make this window Garrulus rather than any other Arbor product.
   * The vault chip, because everything on screen belongs to one vault and this
   * is where you see which; and the sync control, which is the *entire* sync UI
   * (`docs/garrulus-design.md` §4.3) — there is exactly one place sync state is
   * displayed, and this is it. Everything else is the standard Arbor bar, on
   * purpose.
   *
   * The vault chip is a dropdown: the known vaults to switch between, and the
   * verbs to open, create or close one.
   */
  import ProductIcon from '$lib/components/shared/internal/ProductIcon.svelte';
  import TitleBar from '$lib/components/shared/ui/TitleBar.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import WindowControls from '$lib/components/shared/WindowControls.svelte';
  import WorkspaceTabs from '$lib/components/shared/internal/WorkspaceTabs.svelte';
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';
  import { createNativeMenuPublisher } from '$lib/utils/native-menu';
  import { windowMenuItems } from '$lib/utils/window-menu';
  import { surfaceStore } from '$lib/stores/surfaces.svelte';
  import { BookOpen, ChevronDown, Command, FolderOpen, FolderPlus, X } from 'lucide-svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import GarrulusSyncButton from './GarrulusSyncButton.svelte';
  import { garrulusUiStore } from '$lib/stores/garrulus/ui.svelte';
  import { garrulusVaultStore, vaultLastSeen } from '$lib/stores/garrulus/vault.svelte';

  interface Props {
    /** Display name of the open vault, or `null` when none is open. */
    vaultName?: string | null;
    /** Forwarded to the sync control — see its `onConflicts` prop. */
    onConflicts?: () => void;
  }

  let { vaultName = null, onConflicts }: Props = $props();

  // macOS: the hamburger becomes the real menu bar. No-op elsewhere.
  const publishNativeMenu = createNativeMenuPublisher('Garrulus');

  // Only the shared Window section for now — the vault and note verbs join it as
  // their flows land, rather than being listed here as dead entries.
  const hamburgerMenu = $derived<DropdownItem[]>([...windowMenuItems()]);

  /** The vault switcher: the vaults Garrulus knows (most recent first, the open one ticked), then the
   *  verbs that bring in or let go of one. */
  const vaultMenu = $derived.by<DropdownItem[]>(() => {
    const known: DropdownItem[] = garrulusVaultStore.entries.map((e) => ({
      kind: 'item',
      id: e.id,
      label: e.display_name,
      subtitle: e.path,
      meta: vaultLastSeen(e) ?? undefined,
      active: e.path === garrulusVaultStore.root,
      onclick: () => void garrulusVaultStore.openById(e.id),
    }));
    return [
      ...known,
      ...(known.length ? [{ kind: 'separator' } as DropdownItem] : []),
      { kind: 'item', id: 'open', label: 'Open a vault…', icon: FolderOpen, onclick: () => garrulusUiStore.openVaultPicker('open') },
      { kind: 'item', id: 'create', label: 'Create a vault…', icon: FolderPlus, onclick: () => garrulusUiStore.openVaultPicker('create') },
      ...(garrulusVaultStore.isOpen
        ? [{ kind: 'item', id: 'close', label: 'Close this vault', icon: X, danger: true, onclick: () => void garrulusVaultStore.close() } as DropdownItem]
        : []),
    ];
  });
</script>

<TitleBar
  logoTooltip="Garrulus — notes"
  menu={hamburgerMenu}
  onNativeMenu={publishNativeMenu}
  nativeMenuEnabled={surfaceStore.hasFocus('garrulus')}
  menuWidth="250px"
>
  {#snippet logo()}
    <ProductIcon id="garrulus" size={22} />
  {/snippet}

  {#snippet center()}
    <!-- Product tabs, when this window is the tabbed container. Empty in a
         standalone Garrulus window. -->
    <WorkspaceTabs />
  {/snippet}

  {#snippet leading()}
    <Dropdown items={vaultMenu} position="fixed" direction="down" width="300px">
      {#snippet trigger({ open, toggle })}
        <button
          type="button"
          class="gtb-vault"
          class:open
          onclick={toggle}
          use:tooltip={vaultName ? `Vault: ${vaultName} — switch vault` : 'No vault open — open one'}
          aria-haspopup="menu"
          aria-expanded={open}
        >
          <Monogram name={vaultName ?? 'Garrulus'} size={16} />
          <span class="gtb-vault-name">{vaultName ?? 'No vault'}</span>
          <ChevronDown size={12} />
        </button>
      {/snippet}
    </Dropdown>
  {/snippet}

  {#snippet trailing()}
    <div class="gtb-actions">
      <GarrulusSyncButton {onConflicts} />
      <button
        class="gtb-btn"
        type="button"
        onclick={() => garrulusUiStore.toggleDocs()}
        use:tooltip={'Documentation — F1'}
        aria-label="Documentation"
      >
        <BookOpen size={16} />
      </button>
      <button
        class="gtb-btn"
        type="button"
        onclick={() => garrulusUiStore.togglePalette()}
        use:tooltip={'Command palette — Ctrl+K'}
        aria-label="Command palette"
      >
        <Command size={16} />
      </button>
      <!-- Settings joins these two when there is a panel behind it. A third icon
           that opens nothing is worse than a missing one. -->
    </div>
  {/snippet}

  {#snippet windowControls()}
    <WindowControls />
  {/snippet}
</TitleBar>

<style>
  /* The right cluster has no gap of its own, and the sync control must not sit
     flush against the window controls' divider. Matches the mockup's 6px. */
  .gtb-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-right: 6px;
  }

  /* Same 28px square as every other window's title-bar action. */
  .gtb-btn {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border: none;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
  }
  .gtb-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  /* The vault switcher's trigger, drawn like the project switcher of the other products. */
  .gtb-vault {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px 0 6px;
    height: 26px;
    border: none;
    border-radius: var(--radius-md);
    background: transparent;
    font: inherit;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    white-space: nowrap;
    cursor: pointer;
  }
  .gtb-vault:hover,
  .gtb-vault.open {
    background: color-mix(in srgb, var(--text-primary) 8%, transparent);
    color: var(--text-primary);
  }

  .gtb-vault-name {
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
