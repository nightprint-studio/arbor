<script lang="ts">
  /**
   * BennuMoveTargetPicker — which type a member is moving into.
   *
   * ## Why this exists at all
   *
   * *Pull up*, *push down* and *move member* differ in one thing only: the type the member lands
   * in. Where the answer is written in the file — the `extends` clause, a subtype declared right
   * there — the menu already carries a row per target and this never opens. What it answers is the
   * case the buffer cannot: a superclass in another file, a subtype the index knows about, any of
   * the thousands of types a project declares.
   *
   * ## Why it is the command palette and not a modal of its own
   *
   * The gesture is "pick one of many, by typing" — which is the palette's whole job, and it already
   * owns the overlay, the keyboard model (↑↓ Enter Esc), the loading state and the empty copy.
   * A second dialog doing the same thing differently is a second place to fix the day arrow keys
   * stop working in one of them.
   *
   * ## The subtitle is load-bearing
   *
   * Two types called `Builder` is the normal case in a real project, so the row shows the qualified
   * name under the simple one — it is the only thing that tells them apart. And a target that will
   * **widen** the member to `protected` says so there, because a visibility change nobody was told
   * about is the kind of thing found later, in a review.
   */
  import CommandPaletteShell, {
    type PaletteSection,
  } from '$lib/components/shared/ui/CommandPaletteShell.svelte';
  import type { IconComponent } from '$lib/types/icon';
  import { Box, Braces, Hexagon, Rows3, Command } from 'lucide-svelte';
  import { fuzzyMatchPair } from '$lib/utils/fuzzy';
  import type { MoveTarget } from '$lib/ipc/bennu/refactor';

  interface Props {
    /** What the menu row said — `Pull member up to…` — shown as the placeholder's verb. */
    title: string;
    /** The member being moved, for the placeholder. */
    member: string;
    targets: MoveTarget[];
    loading: boolean;
    onPick: (target: MoveTarget) => void;
    onClose: () => void;
  }

  let { title, member, targets, loading, onPick, onClose }: Props = $props();

  let query = $state('');

  // One glyph per type kind, the same four the project tree and the class index use.
  const ICONS: Record<string, IconComponent> = {
    class: Box,
    interface: Braces,
    enum: Hexagon,
    record: Rows3,
    annotation: Braces,
  };
  const iconResolver = (name: string): IconComponent => ICONS[name] ?? Command;

  /**
   * Ranked by the shared matcher, so this list orders the way go-to-file and the palette do.
   *
   * With an empty query the project's own order stands — for a pull up that is the order the class
   * writes its supertypes in, which is the order the reader already has in mind.
   */
  const ranked = $derived.by(() => {
    const q = query.trim();
    if (!q) return targets;
    return targets
      .map((t) => ({ t, m: fuzzyMatchPair(t.name, t.qualified, q) }))
      .filter((r): r is { t: MoveTarget; m: NonNullable<ReturnType<typeof fuzzyMatchPair>> } =>
        r.m !== null,
      )
      .sort((a, b) => b.m.score - a.m.score)
      .map((r) => r.t);
  });

  const sections = $derived<PaletteSection[]>([
    {
      id: 'targets',
      label: 'Move into',
      items: ranked.map((t) => ({
        id: `${t.qualified}|${t.file}`,
        title: t.name,
        subtitle: t.widens ? `${t.qualified} — widened to protected` : t.qualified,
        icon: t.kind || 'class',
        mono: true,
        action: () => onPick(t),
      })),
    },
  ]);
</script>

<CommandPaletteShell
  {onClose}
  {iconResolver}
  {sections}
  bind:query
  {loading}
  loadingLabel="Looking for types…"
  placeholder={`${title.replace(/…$/, '')} ${member} — type a type name`}
>
  {#snippet emptyMessage()}
    {#if targets.length === 0}
      No type here can take this member
    {:else}
      No type matches
    {/if}
  {/snippet}
</CommandPaletteShell>
