<script lang="ts">
  /**
   * nemus documentation — the full manual (Getting Started, the language, the
   * studio, keyboard reference). Built on the shared `DocsShell`; the quick
   * in-editor language catalogue stays in the right-rail "Language reference"
   * panel (this is the prose manual, that is the lookup).
   */
  import {
    BookOpen, Rocket, Braces, FileCode2, Hash, Music, WandSparkles,
    Boxes, ListTree, SlidersHorizontal, FileInput, Keyboard, FlaskConical,
  } from 'lucide-svelte';
  import DocsShell, { type DocsNavItem, type DocsNavGroup } from '$lib/components/shared/DocsShell.svelte';
  import GettingStarted from '../docs/GettingStarted.svelte';
  import Language from '../docs/Language.svelte';
  import MiniNotation from '../docs/MiniNotation.svelte';
  import NotesChords from '../docs/NotesChords.svelte';
  import Transforms from '../docs/Transforms.svelte';
  import Arrangement from '../docs/Arrangement.svelte';
  import Mixer from '../docs/Mixer.svelte';
  import Import from '../docs/Import.svelte';
  import Scratch from '../docs/Scratch.svelte';
  import Shortcuts from '../docs/Shortcuts.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const topItems: DocsNavItem[] = [
    { id: 'getting-started', label: 'Getting Started', icon: Rocket },
  ];

  const navGroups: DocsNavGroup[] = [
    { id: 'language', label: 'Language', icon: Braces, items: [
      { id: 'lang-basics', label: 'Basics',         icon: FileCode2 },
      { id: 'mini',        label: 'Mini-notation',  icon: Hash },
      { id: 'notes',       label: 'Notes & chords', icon: Music },
      { id: 'transforms',  label: 'Transforms',     icon: WandSparkles },
    ] },
    { id: 'studio', label: 'Studio', icon: Boxes, items: [
      { id: 'arrangement', label: 'Arrangement', icon: ListTree },
      { id: 'mixer',       label: 'Mixer & render', icon: SlidersHorizontal },
      { id: 'scratch',     label: 'Scratch & testing', icon: FlaskConical },
      { id: 'import',      label: 'Import',      icon: FileInput },
    ] },
    { id: 'reference', label: 'Reference', icon: Keyboard, items: [
      { id: 'shortcuts', label: 'Keyboard shortcuts', icon: Keyboard },
    ] },
  ];

  const sections = {
    'getting-started': GettingStarted,
    'lang-basics': Language,
    'mini': MiniNotation,
    'notes': NotesChords,
    'transforms': Transforms,
    'arrangement': Arrangement,
    'mixer': Mixer,
    'scratch': Scratch,
    'import': Import,
    'shortcuts': Shortcuts,
  };
</script>

<DocsShell
  {topItems}
  {navGroups}
  {sections}
  {onClose}
  title="nemus Documentation"
  headerIcon={BookOpen}
  initialSection="getting-started"
/>
