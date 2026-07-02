<script lang="ts">
  /**
   * Bennu documentation — the manual (Getting Started, Projects & capabilities,
   * Editing & navigation, keyboard reference). Built on the shared `DocsShell`,
   * the same surface Arbor's and merula's docs use.
   */
  import { BookOpen, Rocket, Boxes, PenLine, Keyboard, FolderGit2 } from 'lucide-svelte';
  import DocsShell, { type DocsNavItem, type DocsNavGroup } from '$lib/components/shared/DocsShell.svelte';
  import GettingStarted from './docs/GettingStarted.svelte';
  import Projects from './docs/Projects.svelte';
  import Editing from './docs/Editing.svelte';
  import Shortcuts from './docs/Shortcuts.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const topItems: DocsNavItem[] = [
    { id: 'getting-started', label: 'Getting Started', icon: Rocket },
  ];

  const navGroups: DocsNavGroup[] = [
    { id: 'workspace', label: 'Workspace', icon: Boxes, items: [
      { id: 'projects', label: 'Projects & capabilities', icon: FolderGit2 },
      { id: 'editing',  label: 'Editing & navigation',    icon: PenLine },
    ] },
    { id: 'reference', label: 'Reference', icon: Keyboard, items: [
      { id: 'shortcuts', label: 'Keyboard shortcuts', icon: Keyboard },
    ] },
  ];

  const sections = {
    'getting-started': GettingStarted,
    'projects': Projects,
    'editing': Editing,
    'shortcuts': Shortcuts,
  };
</script>

<DocsShell
  {topItems}
  {navGroups}
  {sections}
  {onClose}
  title="Bennu Documentation"
  headerIcon={BookOpen}
  initialSection="getting-started"
/>
