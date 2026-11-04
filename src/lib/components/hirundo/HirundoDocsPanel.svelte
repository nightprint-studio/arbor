<script lang="ts">
  /**
   * Hirundo documentation — built on the shared `DocsShell`. Topics live in `./docs/`; this file only wires
   * the navigation.
   */
  import { BookOpen, FlaskConical, Globe, Keyboard, Rocket } from 'lucide-svelte';
  import DocsShell, { type DocsNavItem, type DocsNavGroup } from '$lib/components/shared/DocsShell.svelte';
  import GettingStarted from './docs/GettingStarted.svelte';
  import Environments from './docs/Environments.svelte';
  import Tests from './docs/Tests.svelte';
  import Shortcuts from './docs/Shortcuts.svelte';

  let { onClose, initialSection = 'getting-started' }: { onClose: () => void; initialSection?: string } = $props();

  const topItems: DocsNavItem[] = [
    { id: 'getting-started', label: 'Getting Started', icon: Rocket },
    { id: 'environments', label: 'Environments', icon: Globe },
    { id: 'tests', label: 'Tests and runner', icon: FlaskConical },
  ];

  const navGroups: DocsNavGroup[] = [
    { id: 'reference', label: 'Reference', icon: Keyboard, items: [{ id: 'shortcuts', label: 'Keyboard shortcuts', icon: Keyboard }] },
  ];

  const sections = { 'getting-started': GettingStarted, environments: Environments, tests: Tests, shortcuts: Shortcuts };
</script>

<DocsShell {topItems} {navGroups} {sections} {onClose} title="Hirundo Documentation" headerIcon={BookOpen} product="Hirundo" fileBase="hirundo-docs" {initialSection} />
