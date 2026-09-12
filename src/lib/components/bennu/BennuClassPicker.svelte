<script lang="ts">
  /**
   * BennuClassPicker — one of the project's classes, picked by typing its name.
   *
   * The command palette's overlay, like `BennuMoveTargetPicker`: "one of thousands, by typing" is the
   * palette's job, and it already owns the keyboard (↑↓ Enter Esc) and the loading state. The list is
   * Go to class's, so a class shows up here as soon as it shows up there.
   *
   * The qualified name is the subtitle because two classes called `Builder` is the normal case in a
   * real project, and it is the only thing that tells them apart.
   */
  import { onMount } from 'svelte';
  import CommandPaletteShell, {
    type PaletteSection,
  } from '$lib/components/shared/ui/CommandPaletteShell.svelte';
  import { fuzzyMatchPair } from '$lib/utils/fuzzy';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
  import type { ClassEntry } from '$lib/types/bennu';
  import { typeKindIcon } from './type-kind-icons';

  interface Props {
    placeholder: string;
    /** The type kinds offered (`class`, `record`, …); every kind when absent. */
    kinds?: readonly string[];
    onPick: (entry: ClassEntry) => void;
    onClose: () => void;
  }

  let { placeholder, kinds, onPick, onClose }: Props = $props();

  /** Past this many rows nobody is reading the list, they are typing one more letter. */
  const MAX_ROWS = 200;

  let query = $state('');
  let classes = $state<ClassEntry[]>([]);
  let loading = $state(true);

  onMount(() => {
    const root = projectStore.project?.root;
    if (!root) {
      loading = false;
      return;
    }
    bennuIndexStore
      .classesForRoot(root)
      .then((list) => { classes = list; })
      .catch(() => { classes = []; })
      .finally(() => { loading = false; });
  });

  const offered = $derived.by(() => {
    const allowed = kinds;
    return allowed ? classes.filter((c) => !c.kind || allowed.includes(c.kind)) : classes;
  });

  /** Ranked by the shared matcher, so this list orders the way Go to class does. */
  const ranked = $derived.by(() => {
    const q = query.trim();
    if (!q) return offered.slice(0, MAX_ROWS);
    const scored: { entry: ClassEntry; score: number }[] = [];
    for (const entry of offered) {
      const match = fuzzyMatchPair(entry.simple, entry.fqcn, q);
      if (match) scored.push({ entry, score: match.score });
    }
    return scored
      .sort((a, b) => b.score - a.score)
      .slice(0, MAX_ROWS)
      .map((r) => r.entry);
  });

  const sections = $derived<PaletteSection[]>([
    {
      id: 'classes',
      label: 'Classes',
      items: ranked.map((entry) => ({
        id: `${entry.fqcn}|${entry.file}`,
        title: entry.simple,
        subtitle: entry.fqcn,
        icon: entry.kind || 'class',
        mono: true,
        action: () => onPick(entry),
      })),
    },
  ]);
</script>

<CommandPaletteShell
  {onClose}
  iconResolver={typeKindIcon}
  {sections}
  bind:query
  {loading}
  loadingLabel="Looking for classes…"
  {placeholder}
>
  {#snippet emptyMessage()}
    {#if offered.length === 0}
      The project has no classes indexed yet
    {:else}
      No class matches
    {/if}
  {/snippet}
</CommandPaletteShell>
