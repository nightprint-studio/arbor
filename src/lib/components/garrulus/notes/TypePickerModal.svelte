<script lang="ts">
  /**
   * Which note type? — the first step of "New note of a type" and the whole of
   * "Give this note a type".
   *
   * Typing filters by name, Enter takes the first, so the common case is
   * `Ctrl+Shift+N, b, Enter`.
   */
  import CommandPaletteShell, {
    type PaletteSection,
  } from '$lib/components/shared/ui/CommandPaletteShell.svelte';
  import { paletteIcon } from '$lib/utils/plugin-icons';
  import { garrulusVaultStore } from '$lib/stores/garrulus/vault.svelte';
  import { iconKey } from './note-naming';

  interface Props {
    /** What the choice is for, shown as the placeholder. */
    placeholder: string;
    onPick: (typeId: string) => void;
    onClose: () => void;
  }

  let { placeholder, onPick, onClose }: Props = $props();

  let query = $state('');

  const sections = $derived.by<PaletteSection[]>(() => {
    const q = query.trim().toLowerCase();
    const items = garrulusVaultStore.types
      .filter((t) => !q || t.name.toLowerCase().includes(q) || t.id.toLowerCase().includes(q))
      .map((t) => ({
        id: t.id,
        title: t.name,
        subtitle: t.folder ? `${t.folder}/` : undefined,
        icon: iconKey(t.icon) || 'FileText',
        iconColor: t.accent,
        action: () => onPick(t.id),
      }));
    return items.length ? [{ id: 'types', label: 'Note types', items }] : [];
  });
</script>

<CommandPaletteShell {onClose} iconResolver={paletteIcon} {sections} bind:query {placeholder} width="min(480px, 90vw)">
  {#snippet emptyMessage()}
    No note type matches “{query.trim()}”.
  {/snippet}
</CommandPaletteShell>
