<script lang="ts">
  /**
   * Bennu documentation — the manual (Getting Started, Projects & capabilities,
   * Editing & navigation, Building & running, Testing, Structural search, keyboard reference). Built on the shared `DocsShell`,
   * the same surface Arbor's and merula's docs use.
   */
  import { BookOpen, Rocket, Boxes, PenLine, Keyboard, FolderGit2, FlaskConical, Play, Replace } from 'lucide-svelte';
  import DocsShell, { type DocsNavItem, type DocsNavGroup } from '$lib/components/shared/DocsShell.svelte';
  import GettingStarted from './docs/GettingStarted.svelte';
  import Projects from './docs/Projects.svelte';
  import Editing from './docs/Editing.svelte';
  import Running from './docs/Running.svelte';
  import Testing from './docs/Testing.svelte';
  import Structural from './docs/Structural.svelte';
  import Shortcuts from './docs/Shortcuts.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const topItems: DocsNavItem[] = [
    { id: 'getting-started', label: 'Getting Started', icon: Rocket },
  ];

  const navGroups: DocsNavGroup[] = [
    { id: 'workspace', label: 'Workspace', icon: Boxes, items: [
      { id: 'projects', label: 'Projects & capabilities', icon: FolderGit2 },
      { id: 'editing',  label: 'Editing & navigation',    icon: PenLine },
      { id: 'running',  label: 'Building & running',      icon: Play },
      { id: 'testing',  label: 'Testing',                 icon: FlaskConical },
      { id: 'structural', label: 'Structural search',      icon: Replace },
    ] },
    { id: 'reference', label: 'Reference', icon: Keyboard, items: [
      { id: 'shortcuts', label: 'Keyboard shortcuts', icon: Keyboard },
    ] },
  ];

  const sections = {
    'getting-started': GettingStarted,
    'projects': Projects,
    'editing': Editing,
    'running': Running,
    'testing': Testing,
    'structural': Structural,
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
