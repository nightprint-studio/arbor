<script lang="ts">
  /**
   * Garrulus documentation — built on the shared `DocsShell`, the same searchable
   * panel the rest of the suite uses. Topics live in `./docs/`; this file only
   * wires the navigation.
   *
   * `product` and `fileBase` are the two facts the shell cannot infer: they name
   * the exported README and its heading, so what leaves the window says Garrulus
   * rather than the name of the suite it happens to ship in.
   *
   * The topics are flat rather than grouped: there are few of them, and burying
   * *Syncing* one level down would hide the page the product exists for.
   */
  import { BookOpen, Keyboard, Layers, Pencil, RefreshCw, Rocket } from 'lucide-svelte';
  import DocsShell, { type DocsNavItem, type DocsNavGroup } from '$lib/components/shared/DocsShell.svelte';
  import GettingStarted from './docs/GettingStarted.svelte';
  import Editing from './docs/Editing.svelte';
  import NoteTypes from './docs/NoteTypes.svelte';
  import Sync from './docs/Sync.svelte';
  import Shortcuts from './docs/Shortcuts.svelte';

  let { onClose, initialSection = 'getting-started' }: {
    onClose: () => void;
    /** Topic to land on. The palette addresses topics by name. */
    initialSection?: string;
  } = $props();

  const topItems: DocsNavItem[] = [
    { id: 'getting-started', label: 'Getting Started', icon: Rocket },
    { id: 'editing', label: 'Editing', icon: Pencil },
    { id: 'types', label: 'Note types', icon: Layers },
    { id: 'sync', label: 'Syncing', icon: RefreshCw },
  ];

  const navGroups: DocsNavGroup[] = [
    {
      id: 'reference', label: 'Reference', icon: Keyboard, items: [
        { id: 'shortcuts', label: 'Keyboard shortcuts', icon: Keyboard },
      ],
    },
  ];

  const sections = {
    'getting-started': GettingStarted,
    'editing': Editing,
    'types': NoteTypes,
    'sync': Sync,
    'shortcuts': Shortcuts,
  };
</script>

<DocsShell
  {topItems}
  {navGroups}
  {sections}
  {onClose}
  title="Garrulus Documentation"
  headerIcon={BookOpen}
  product="Garrulus"
  fileBase="garrulus-docs"
  {initialSection}
/>
