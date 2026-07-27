<script lang="ts">
  /**
   * Window switcher — jump to any other open Arbor window from the keyboard.
   *
   * Mounted by `+page.svelte` in EVERY non-overlay window, so the same chord
   * works from Corvus, Bennu, Merula, the File Explorer, Tyto and Canopy alike.
   * That ubiquity is the feature: the OS switchers are uneven (Windows gives
   * each window a taskbar button, macOS gives none), so Arbor carries its own.
   *
   * Deliberately NOT a Command Palette entry per product — one implementation
   * on the shared palette engine, one keybinding, every window.
   */
  import { onMount } from 'svelte';
  import {
    GitBranch, FolderTree, Music, Video, Coffee, LayoutGrid, AppWindow, EyeOff,
  } from 'lucide-svelte';
  import CommandPaletteShell, {
    type PaletteSection,
  } from '$lib/components/shared/ui/CommandPaletteShell.svelte';
  import type { IconComponent } from '$lib/types/icon';
  import { windowsStore } from '$lib/stores/windows.svelte';
  import { keybindingsStore } from '$lib/stores/keybindings.svelte';
  import { matchesBinding } from '$lib/utils/keybindings';
  import type { ArborWindow, SurfaceKind } from '$lib/ipc/window';

  let query = $state('');
  const open = $derived(windowsStore.switcherOpen);

  // Keep the directory live for the whole window, not just while the overlay is
  // up: the title bar's Window menu reads the same store and must be correct the
  // instant it opens (a menu can't wait for a round-trip). One listener per
  // window, refreshed only when a window actually opens, closes or is retitled.
  $effect(() => windowsStore.watch());

  onMount(() => {
    const onKeydown = (e: KeyboardEvent) => {
      if (matchesBinding(e, keybindingsStore.getBinding('switch_window'))) {
        e.preventDefault();
        // Consume it: the product shells run their own global matcher (AppShell
        // preventDefaults every matched action), and this chord is handled here
        // for every window — it must not reach them a second time.
        e.stopPropagation();
        query = '';
        windowsStore.toggleSwitcher();
      }
    };
    // Capture phase: modals and editors stop propagation on their own keydowns,
    // and "get me out of this window" has to work from inside them too.
    window.addEventListener('keydown', onKeydown, true);
    return () => window.removeEventListener('keydown', onKeydown, true);
  });

  const PRODUCT_ICONS: Record<string, IconComponent> = {
    corvus: GitBranch,
    sitta:  FolderTree,
    merula: Music,
    tyto:   Video,
    bennu:  Coffee,
  };

  const ICONS: Record<string, IconComponent> = {
    ...PRODUCT_ICONS,
    canopy: LayoutGrid,
    window: AppWindow,
    hidden: EyeOff,
  };

  function iconResolver(name: string): IconComponent {
    return ICONS[name] ?? AppWindow;
  }

  function iconFor(w: ArborWindow): string {
    if (w.kind === 'launcher') return 'canopy';
    return w.product ?? 'window';
  }

  /** Human label for the group a window lands in. */
  const GROUP_LABEL: Record<SurfaceKind, string> = {
    launcher:  'Canopy',
    workspace: 'Products',
    utility:   'Tools',
    ambient:   'Tools',
    overlay:   'Tools',
  };
  const GROUP_ORDER: SurfaceKind[] = ['launcher', 'workspace', 'utility'];

  const matches = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return windowsStore.others;
    return windowsStore.others.filter(
      (w) => w.title.toLowerCase().includes(q) || (w.product ?? '').includes(q),
    );
  });

  const sections = $derived.by((): PaletteSection[] => {
    const out: PaletteSection[] = [];
    for (const kind of GROUP_ORDER) {
      // `utility` collects the ambient surfaces too — both read as "Tools" to
      // the user, and a one-row section per kind would be noise.
      const items = matches.filter((w) =>
        kind === 'utility' ? w.kind === 'utility' || w.kind === 'ambient' : w.kind === kind,
      );
      if (!items.length) continue;
      out.push({
        id: kind,
        label: GROUP_LABEL[kind],
        items: items.map((w) => ({
          id: w.label,
          title: w.title,
          subtitle: w.visible ? undefined : 'Hidden — closed to tray',
          icon: w.visible ? iconFor(w) : 'hidden',
          action: () => { void windowsStore.focus(w.label); windowsStore.closeSwitcher(); },
        })),
      });
    }
    return out;
  });
</script>

{#if open}
  <CommandPaletteShell
    onClose={() => windowsStore.closeSwitcher()}
    {iconResolver}
    {sections}
    bind:query
    placeholder="Go to window…"
    loading={windowsStore.loading && windowsStore.others.length === 0}
    loadingLabel="Listing windows…"
    width="min(520px, 90vw)"
  >
    {#snippet emptyMessage()}
      {query ? 'No window matches' : 'No other Arbor window is open'}
    {/snippet}
  </CommandPaletteShell>
{/if}
