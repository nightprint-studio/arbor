<script lang="ts">
  /**
   * Tyto documentation — the product manual, built on the shared `DocsShell`
   * (same searchable panel the rest of the suite uses). App-specific sections
   * live in `./docs/`; this file only wires the nav.
   */
  import { BookOpen, Rocket, Clapperboard, MousePointerSquareDashed, Crop, Images, Keyboard } from 'lucide-svelte';
  import DocsShell, { type DocsNavItem, type DocsNavGroup } from '$lib/components/shared/DocsShell.svelte';
  import GettingStarted from './docs/GettingStarted.svelte';
  import Capturing from './docs/Capturing.svelte';
  import Sources from './docs/Sources.svelte';
  import Region from './docs/Region.svelte';
  import Library from './docs/Library.svelte';
  import Shortcuts from './docs/Shortcuts.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const topItems: DocsNavItem[] = [
    { id: 'getting-started', label: 'Getting Started', icon: Rocket },
  ];

  const navGroups: DocsNavGroup[] = [
    {
      id: 'capture', label: 'Capture', icon: Clapperboard, items: [
        { id: 'capturing', label: 'Recording & screenshots', icon: Clapperboard },
        { id: 'sources', label: 'Sources', icon: MousePointerSquareDashed },
        { id: 'region', label: 'Region selection', icon: Crop },
      ],
    },
    {
      id: 'library', label: 'Library', icon: Images, items: [
        { id: 'library', label: 'Captures library', icon: Images },
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
    'capturing': Capturing,
    'sources': Sources,
    'region': Region,
    'library': Library,
    'shortcuts': Shortcuts,
  };
</script>

<DocsShell
  {topItems}
  {navGroups}
  {sections}
  {onClose}
  title="Tyto Documentation"
  headerIcon={BookOpen}
  initialSection="getting-started"
/>
