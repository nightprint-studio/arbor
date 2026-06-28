<script lang="ts">
  /**
   * merula documentation — the full manual (Getting Started, the language, the
   * studio, keyboard reference). Built on the shared `DocsShell`; the quick
   * in-editor language catalogue stays in the right-rail "Language reference"
   * panel (this is the prose manual, that is the lookup).
   */
  import {
    BookOpen, Rocket, Braces, FileCode2, Hash, Music, WandSparkles,
    Boxes, ListTree, SlidersHorizontal, FileInput, Keyboard, FlaskConical,
    Music4, LayoutGrid, Dices, Library, PenLine, Folders,
  } from 'lucide-svelte';
  import DocsShell, { type DocsNavItem, type DocsNavGroup } from '$lib/components/shared/DocsShell.svelte';
  import GettingStarted from '../docs/GettingStarted.svelte';
  import Projects from '../docs/Projects.svelte';
  import Language from '../docs/Language.svelte';
  import MiniNotation from '../docs/MiniNotation.svelte';
  import NotesChords from '../docs/NotesChords.svelte';
  import Generators from '../docs/Generators.svelte';
  import Transforms from '../docs/Transforms.svelte';
  import Libraries from '../docs/Libraries.svelte';
  import Sounds from '../docs/Sounds.svelte';
  import Arrangement from '../docs/Arrangement.svelte';
  import Launcher from '../docs/Launcher.svelte';
  import Mixer from '../docs/Mixer.svelte';
  import Editing from '../docs/Editing.svelte';
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
      { id: 'generators',  label: 'Generators',     icon: Dices },
      { id: 'transforms',  label: 'Transforms',     icon: WandSparkles },
      { id: 'libraries',   label: 'Libraries',      icon: Library },
    ] },
    { id: 'studio', label: 'Studio', icon: Boxes, items: [
      { id: 'projects',    label: 'Projects & tabs', icon: Folders },
      { id: 'sounds',      label: 'Sound bank',     icon: Music4 },
      { id: 'arrangement', label: 'Arrangement',    icon: ListTree },
      { id: 'launcher',    label: 'Clip launcher',  icon: LayoutGrid },
      { id: 'mixer',       label: 'Mixer & render', icon: SlidersHorizontal },
      { id: 'editing',     label: 'Editing & refactoring', icon: PenLine },
      { id: 'scratch',     label: 'Scratch & testing', icon: FlaskConical },
      { id: 'import',      label: 'Import',         icon: FileInput },
    ] },
    { id: 'reference', label: 'Reference', icon: Keyboard, items: [
      { id: 'shortcuts', label: 'Keyboard shortcuts', icon: Keyboard },
    ] },
  ];

  const sections = {
    'getting-started': GettingStarted,
    'projects': Projects,
    'lang-basics': Language,
    'mini': MiniNotation,
    'notes': NotesChords,
    'generators': Generators,
    'transforms': Transforms,
    'libraries': Libraries,
    'sounds': Sounds,
    'arrangement': Arrangement,
    'launcher': Launcher,
    'mixer': Mixer,
    'editing': Editing,
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
  title="merula Documentation"
  headerIcon={BookOpen}
  initialSection="getting-started"
/>
