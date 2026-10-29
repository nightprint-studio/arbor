<script lang="ts">
  /**
   * BennuModulePicker — which module a palette command should act on.
   *
   * Shown only when the answer is genuinely ambiguous: several modules of the ecosystem, and neither
   * a pinned selection nor the active file says which (`BennuWindow.withModule`). The command
   * palette's overlay, like the class picker: type to narrow by project, folder or kind, ↑↓ move,
   * Enter picks, Esc closes.
   */
  import CommandPaletteShell, {
    type PaletteSection,
  } from '$lib/components/shared/ui/CommandPaletteShell.svelte';
  import { Package } from 'lucide-svelte';
  import type { IconComponent } from '$lib/types/icon';
  import { fuzzyMatchPair } from '$lib/utils/fuzzy';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import type { ModuleRef } from './project-modules';

  interface Props {
    title: string;
    modules: readonly ModuleRef[];
    onPick: (module: ModuleRef) => void;
    onClose: () => void;
  }

  let { title, modules, onPick, onClose }: Props = $props();

  let query = $state('');

  const showProject = $derived(new Set(modules.map((m) => m.root)).size > 1);

  function nameOf(m: ModuleRef): string {
    const project = projectStore.projectNameOf(m.root);
    if (m.dir === '.') return project;
    return showProject ? `${project} / ${m.dir}` : m.dir;
  }

  const ranked = $derived.by(() => {
    const q = query.trim();
    if (!q) return [...modules];
    return modules
      .map((m) => ({ m, match: fuzzyMatchPair(nameOf(m), m.toolRoot, q) }))
      .filter((r) => r.match)
      .sort((a, b) => (b.match?.score ?? 0) - (a.match?.score ?? 0))
      .map((r) => r.m);
  });

  const sections = $derived<PaletteSection[]>([
    {
      id: 'modules',
      label: title,
      items: ranked.map((m) => ({
        id: m.id,
        title: nameOf(m),
        subtitle: `${m.kind} · ${m.toolRoot}`,
        icon: 'module',
        action: () => onPick(m),
      })),
    },
  ]);

  const resolveIcon = (): IconComponent => Package as unknown as IconComponent;
</script>

<CommandPaletteShell
  {onClose}
  iconResolver={resolveIcon}
  {sections}
  bind:query
  placeholder={`${title} — pick a module…`}
>
  {#snippet emptyMessage()}No module matches{/snippet}
</CommandPaletteShell>
