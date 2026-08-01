<script lang="ts">
  /**
   * One note two machines disagree about: who wrote what, and the three ways out.
   *
   * The promise the whole sync design rests on (`docs/garrulus-design.md` §4.4) —
   * that nothing was written into any note and no merge marker exists anywhere —
   * is stated once by `ConflictsPanel`, above the list. What is per-note, and so
   * belongs here, is *which file* holds the other version: the card names it, so
   * the guarantee above is checkable rather than merely reassuring.
   *
   * "Merge by hand" is deliberately not a third resolution: it opens the note, the
   * user edits it into whatever they want, and then chooses *Keep mine*. That is
   * also what the backend implements — `garrulus_resolve_conflict` accepts only
   * `mine` and `theirs` — so the UI does not invent a state the backend cannot
   * settle.
   */
  import { AlertTriangle, Check, Download, PencilLine } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import ConflictDiff from './ConflictDiff.svelte';
  import { noteFolder, noteName } from './note-path';
  import { computeDiff } from '$lib/utils/conflict/conflict-diff';
  import type { Conflict, ConflictResolution } from '$lib/ipc/garrulus';

  interface Props {
    conflict: Conflict;
    /** True while this card's own resolution is in flight. */
    busy?: boolean;
    onResolve: (resolution: ConflictResolution) => void;
    /** Open the note in the editor. Absent → the button explains it cannot. */
    onOpenNote?: (path: string) => void;
  }

  let { conflict, busy = false, onResolve, onOpenNote }: Props = $props();

  const regions = $derived(computeDiff(conflict.local, conflict.remote));
  const blocks = $derived(regions.filter((r) => r.kind === 'conflict').length);

  /** The note's name and where it lives — the same reading of a note's id every
   *  other Garrulus panel uses, so one path renders one way everywhere. */
  const title = $derived(noteName(conflict.path));
  const folder = $derived(noteFolder(conflict.path));

  /**
   * Which machine the remote side came from, and when.
   *
   * The `Conflict` itself carries only text, so the one place this is recorded is
   * the name `garrulus-sync` parks the side file under —
   * `<note> (conflitto — <device>, <dd-MM HH:mm>).<ext>`, minted by
   * `conflict.rs::side_file_name`. Reading it back out is a real coupling to that
   * spelling, so a name that does not match simply leaves the sentence generic
   * rather than showing a parse artefact.
   */
  const SIDE_FILE_RE = /\(conflitto — (.+), (\d{2}-\d{2} \d{2}:\d{2})\)\.[^.]+$/;

  const other = $derived.by(() => {
    const name = conflict.side_file?.split(/[/\\]/).pop();
    const m = name ? SIDE_FILE_RE.exec(name) : null;
    return m ? { device: m[1], at: m[2] } : null;
  });

  const sideFileName = $derived(conflict.side_file?.split(/[/\\]/).pop() ?? null);

  const remoteLabel = $derived(
    other ? `${other.device} — ${other.at}` : 'The other machine',
  );

  /** Both resolutions delete or move the side file, so neither can run without
   *  one. It is `Option` on the wire, and a conflict that arrived without it is
   *  a conflict this panel can show but not settle. */
  const settleable = $derived(!!conflict.side_file);
  const noSideFile =
    'The other version was not parked beside the note, so there is nothing to ' +
    'choose between here. Pull again to produce it.';
</script>

<section class="cc" data-conflict-card aria-label="Conflict in {title}">
  <header class="cc-head">
    <span class="cc-icon"><AlertTriangle size={15} /></span>
    <span class="cc-name" title={conflict.path}>{title}</span>
    {#if folder}<span class="cc-folder">{folder}</span>{/if}
    {#if blocks > 0}
      <Badge
        variant="tone"
        tone="warning"
        size="sm"
        label={blocks === 1 ? '1 block did not merge' : `${blocks} blocks did not merge`}
      />
    {/if}
    <span class="cc-spacer"></span>

    <Button
      variant="secondary"
      size="xs"
      disabled={busy || !settleable}
      tooltip={settleable
        ? { content: 'Keep this machine’s version and drop the other one' }
        : { content: noSideFile }}
      onclick={() => onResolve('mine')}
    >
      {#snippet iconStart()}<Check size={12} />{/snippet}
      Keep mine
    </Button>
    <Button
      variant="secondary"
      size="xs"
      disabled={busy || !settleable}
      tooltip={settleable
        ? { content: `Overwrite the note with ${other?.device ?? 'the other machine'}’s version` }
        : { content: noSideFile }}
      onclick={() => onResolve('theirs')}
    >
      {#snippet iconStart()}<Download size={12} />{/snippet}
      Take theirs
    </Button>
    <Button
      variant="ghost"
      size="xs"
      disabled={busy || !onOpenNote}
      tooltip={onOpenNote
        ? { content: 'Open the note and edit it yourself — then choose Keep mine' }
        : { content: 'No editor is attached to this panel yet' }}
      onclick={() => onOpenNote?.(conflict.path)}
    >
      {#snippet iconStart()}<PencilLine size={12} />{/snippet}
      Merge by hand
    </Button>
  </header>

  <ConflictDiff
    path={conflict.path}
    {regions}
    localLabel="This machine — in the note now"
    {remoteLabel}
  />

  <!-- The per-note half of the panel's guarantee: see the header comment. -->
  <p class="cc-note">
    {#if sideFileName}
      <code>{conflict.path}</code> still holds your version;
      <code class="cc-side">{sideFileName}</code> beside it holds the other one,
      until you choose.
    {:else}
      <code>{conflict.path}</code> still holds your version, but the other one was
      not parked beside it — pull again to produce it.
    {/if}
  </p>
</section>

<style>
  .cc {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
  }
  /* The card the keyboard is on. `:focus-within` rather than a selected index:
     the focus ring is already where the user is, and a second highlight tracking
     it would be a second thing that can be wrong. */
  .cc:focus-within { border-color: color-mix(in srgb, var(--accent) 45%, transparent); }

  .cc-head {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .cc-icon { display: flex; color: var(--error); flex-shrink: 0; }

  .cc-name {
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cc-folder {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .cc-spacer { flex: 1; }

  .cc-note {
    margin: 0;
    font-size: var(--font-size-xs);
    line-height: 1.5;
    color: var(--text-muted);
  }
  /* Both names are file names — monospace so a trailing space or a lookalike
     character in one of them is visible rather than merely present. */
  .cc-note code {
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-secondary);
  }
  /* The one the reader has to go and look at if they do not believe the panel. */
  .cc-note code.cc-side { color: var(--text-primary); }
</style>
