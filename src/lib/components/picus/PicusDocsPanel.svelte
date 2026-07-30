<script lang="ts">
  /**
   * Picus documentation — built on the shared `DocsShell`, the same searchable
   * panel the rest of the suite uses. Topics live in `./docs/`; this file only
   * wires the navigation.
   *
   * `product` and `fileBase` are the two facts the shell cannot infer: they name
   * the exported README and its heading, so what leaves the window says Picus
   * rather than the name of the suite it happens to ship in.
   */
  import { BookOpen, Rocket, Database, FolderTree, FormInput, Replace, TriangleAlert, Keyboard, Lightbulb, Zap } from 'lucide-svelte';
  import DocsShell, { type DocsNavItem, type DocsNavGroup } from '$lib/components/shared/DocsShell.svelte';
  import GettingStarted from './docs/GettingStarted.svelte';
  import Connections from './docs/Connections.svelte';
  import Scripts from './docs/Scripts.svelte';
  import Generating from './docs/Generating.svelte';
  import Restructuring from './docs/Restructuring.svelte';
  import Consistency from './docs/Consistency.svelte';
  import Editing from './docs/Editing.svelte';
  import Abbreviations from './docs/Abbreviations.svelte';
  import Shortcuts from './docs/Shortcuts.svelte';

  let { onClose, initialSection = 'getting-started' }: {
    onClose: () => void;
    /** Topic to land on. The palette addresses topics by name. */
    initialSection?: string;
  } = $props();

  const topItems: DocsNavItem[] = [
    { id: 'getting-started', label: 'Getting Started', icon: Rocket },
    // The editor spans both halves of the product — a query tab and a script file
    // behave the same — so it sits above the Database / Scripts split rather than
    // being filed under one of them.
    { id: 'editing', label: 'The SQL editor', icon: Lightbulb },
    // Beside it rather than inside it: the shorthand is a language of its own, and
    // a page nobody can find is the failure mode of a feature nobody can guess at.
    { id: 'abbreviations', label: 'SQL abbreviations', icon: Zap },
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
        { id: 'restructuring', label: 'Structural replace', icon: Replace },
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
    'abbreviations': Abbreviations,
    'connections': Connections,
    'scripts': Scripts,
    'generating': Generating,
    'restructuring': Restructuring,
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
  product="Picus"
  fileBase="picus-docs"
  {initialSection}
/>
