<script lang="ts">
  /**
   * Picus documentation — built on the shared `DocsShell`, the same searchable
   * panel the rest of the suite uses. Topics live in `./docs/`; this file only
   * wires the navigation.
   */
  import { BookOpen, Rocket, Database, FolderTree, FormInput, TriangleAlert, Keyboard, Lightbulb } from 'lucide-svelte';
  import DocsShell, { type DocsNavItem, type DocsNavGroup } from '$lib/components/shared/DocsShell.svelte';
  import GettingStarted from './docs/GettingStarted.svelte';
  import Connections from './docs/Connections.svelte';
  import Scripts from './docs/Scripts.svelte';
  import Generating from './docs/Generating.svelte';
  import Consistency from './docs/Consistency.svelte';
  import Editing from './docs/Editing.svelte';
  import Shortcuts from './docs/Shortcuts.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const topItems: DocsNavItem[] = [
    { id: 'getting-started', label: 'Getting Started', icon: Rocket },
    // The editor spans both halves of the product — a query tab and a script file
    // behave the same — so it sits above the Database / Scripts split rather than
    // being filed under one of them.
    { id: 'editing', label: 'The SQL editor', icon: Lightbulb },
  ];

  const navGroups: DocsNavGroup[] = [
    {
      id: 'database', label: 'Database', icon: Database, items: [
        { id: 'connections', label: 'Connections & queries', icon: Database },
      ],
    },
    {
      id: 'scripts', label: 'Scripts', icon: FolderTree, items: [
        { id: 'scripts', label: 'Repository & encoding', icon: FolderTree },
        { id: 'generating', label: 'Generating DML', icon: FormInput },
        { id: 'consistency', label: 'Consistency rules', icon: TriangleAlert },
      ],
    },
    {
      id: 'reference', label: 'Reference', icon: Keyboard, items: [
        { id: 'shortcuts', label: 'Keyboard shortcuts', icon: Keyboard },
      ],
    },
  ];

  const sections = {
    'getting-started': GettingStarted,
    'editing': Editing,
    'connections': Connections,
    'scripts': Scripts,
    'generating': Generating,
    'consistency': Consistency,
    'shortcuts': Shortcuts,
  };
</script>

<DocsShell
  {topItems}
  {navGroups}
  {sections}
  {onClose}
  title="Picus Documentation"
  headerIcon={BookOpen}
  initialSection="getting-started"
/>
