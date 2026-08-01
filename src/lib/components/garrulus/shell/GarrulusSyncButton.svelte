<script lang="ts">
  /**
   * The sync control — the whole of Garrulus's sync UI
   * (`docs/garrulus-design.md` §4.3).
   *
   * One `SplitButton` in the title bar: the main half performs the action the
   * current state makes obvious, the caret opens the rest. It is also the *only*
   * place sync state is displayed, which is what makes it readable — there is
   * exactly one thing to look at, so it is worth looking at.
   *
   * **Colour is state, never decoration.** Every visual on this control (icon,
   * tint, count, verb) is a function of one `SyncState`, computed in the table
   * below and nowhere else. Adding a state means adding a row.
   *
   * **Nothing here writes on its own** (§4.2): every branch of `run` is a click
   * handler. The `synced` and `offline` states' main action is a read-only
   * probe, which is the strongest form of that promise — the button that looks
   * the most like "sync" when there is nothing to do cannot change a byte.
   */
  import {
    AlertTriangle,
    ArrowDownToLine,
    ArrowUpFromLine,
    Check,
    CloudOff,
    MessageSquare,
    RefreshCw,
    Settings2,
  } from 'lucide-svelte';
  import SplitButton, { type SplitOption } from '$lib/components/shared/ui/SplitButton.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import type { IconComponent } from '$lib/types/icon';
  import { isMac } from '$lib/utils/platform';
  import { garrulusSyncStore } from '$lib/stores/garrulus/sync.svelte';
  import { garrulusUiStore } from '$lib/stores/garrulus/ui.svelte';

  interface Props {
    /**
     * Where "Conflicts" goes.
     *
     * A prop rather than a store call because the conflicts view (§4.4, a
     * side-by-side diff in the bottom dock) is not this control's to own. Until
     * the shell has somewhere to send the user, the entry and the `conflict`
     * state's main action are disabled: a control that looks live and does
     * nothing is worse than one that says it cannot yet.
     */
    onConflicts?: () => void;
  }

  let { onConflicts }: Props = $props();

  const sync = garrulusSyncStore;
  const modKey = isMac ? '⌘⇧S' : 'Ctrl+Shift+S';

  /** Plural-aware note count — the label says "1 note", never "1 notes". */
  function notes(n: number): string {
    return `${n} note${n === 1 ? '' : 's'}`;
  }

  /** Stand-in for `onConflicts` while there is no conflicts view. Nothing is
   *  lost in a conflict — the note keeps the local text and the remote one is
   *  parked beside it — so the useful thing to hand over is where to look. */
  function sayWhereConflictsAre() {
    toastStore.show(
      'Each conflicting note kept your version, and the one from the other machine is parked beside it in the vault. Settling them from here arrives with the conflicts view.',
      'info',
      7000,
    );
  }

  /** Everything the control shows, for one state. One row per `SyncState`. */
  interface Presentation {
    icon: IconComponent;
    label: string;
    /** What the main half does when clicked. */
    run: () => void;
    /** The main half's colour. A CSS variable, because it *is* the state. */
    color: string;
    /** Count bubble: its tone, and the number. `0` means no bubble. */
    tone: 'accent' | 'info' | 'warning' | 'error';
    badge: number;
    /** Sentence for the tooltip, in front of the shortcut. */
    tip: string;
  }

  const view = $derived.by((): Presentation => {
    const where = sync.remoteLabel;
    switch (sync.tag) {
      case 'has-changes':
      case 'ahead':
        return {
          icon: ArrowUpFromLine,
          label: `${notes(sync.count)} to send`,
          run: () => void sync.syncNow(),
          color: 'var(--accent)',
          tone: 'accent',
          badge: sync.count,
          tip: 'Commit, pull and push',
        };
      case 'behind':
        return {
          icon: ArrowDownToLine,
          label: where ? `${notes(sync.count)} coming in from ${where}` : `${notes(sync.count)} coming in`,
          run: () => void sync.pull(),
          color: 'var(--info)',
          tone: 'info',
          badge: sync.count,
          tip: 'Bring the incoming notes in',
        };
      case 'diverged':
        return {
          icon: RefreshCw,
          label: `${sync.ahead} to send · ${sync.behind} coming in`,
          run: () => void sync.syncNow(),
          color: 'var(--warning)',
          tone: 'warning',
          // The label already carries both halves; a third number would only
          // ask the reader which of the three it is.
          badge: 0,
          tip: 'Both sides moved — commit, pull and push',
        };
      case 'conflict':
        return {
          icon: AlertTriangle,
          label: `${sync.count} conflict${sync.count === 1 ? '' : 's'} to resolve`,
          // With no view to open, say where the conflicts actually are. They are
          // real files sitting beside their notes, so this is directions rather
          // than an apology — and the vault still opens in any editor.
          run: onConflicts ?? sayWhereConflictsAre,
          color: 'var(--error)',
          tone: 'error',
          badge: sync.count,
          tip: onConflicts
            ? 'Open the conflicts'
            : 'Conflicts are waiting in the vault — the view that settles them is not here yet',
        };
      case 'offline':
        return {
          icon: CloudOff,
          label: 'Unreachable · retrying',
          run: () => void sync.refresh(),
          color: 'var(--text-muted)',
          tone: 'warning',
          badge: 0,
          tip: 'Try the destination again',
        };
      case 'no-remote':
        return {
          icon: CloudOff,
          label: 'No destination',
          run: () => garrulusUiStore.openRemoteConfig(),
          color: 'var(--text-muted)',
          tone: 'accent',
          badge: 0,
          tip: 'Choose where this vault syncs to',
        };
      default:
        return {
          icon: Check,
          label: where ? `Synced · ${where}` : 'Synced',
          run: () => void sync.refresh(),
          color: 'var(--text-secondary)',
          tone: 'accent',
          badge: 0,
          tip: 'Check the destination again',
        };
    }
  });

  const Glyph = $derived(view.icon);
  const noRemote = $derived(sync.tag === 'no-remote');

  const options = $derived<SplitOption[]>([
    {
      id: 'pull',
      label: 'Pull only',
      icon: ArrowDownToLine,
      description: 'Bring changes in without sending anything',
      disabled: noRemote,
    },
    {
      id: 'push',
      label: 'Push only',
      icon: ArrowUpFromLine,
      description: 'Send what is here without pulling first',
      disabled: noRemote,
    },
    {
      id: 'commit',
      label: 'Commit with a message…',
      icon: MessageSquare,
      description: 'Sync, writing the commit message yourself',
      disabled: noRemote,
    },
    {
      id: 'conflicts',
      label: 'Conflicts',
      icon: AlertTriangle,
      iconColor: sync.tag === 'conflict' ? 'var(--error)' : undefined,
      description: onConflicts ? undefined : 'Arrives with the conflicts view',
      disabled: !onConflicts,
    },
    {
      id: 'configure',
      label: 'Configure destination…',
      icon: Settings2,
      description: sync.remoteLabel ?? 'This vault is local-only',
    },
  ]);

  function select(id: string) {
    switch (id) {
      case 'pull': void sync.pull(); break;
      case 'push': void sync.push(); break;
      case 'commit': garrulusUiStore.openCommitMessage(); break;
      case 'conflicts': onConflicts?.(); break;
      case 'configure': garrulusUiStore.openRemoteConfig(); break;
    }
  }
</script>

<SplitButton
  variant="secondary"
  size="md"
  direction="down"
  position="fixed"
  width="260px"
  class="gsync"
  color={view.color}
  loading={sync.busy}
  tooltip={`${sync.busy ? 'Syncing' : view.tip} — ${modKey}`}
  {options}
  onclick={view.run}
  onselect={select}
>
  {#if !sync.busy}
    <Glyph size={14} />
  {/if}
  <span class="gsync-label">{sync.busy ? 'Syncing…' : view.label}</span>
  {#if !sync.busy && view.badge > 0}
    <Badge variant="tone" tone={view.tone} size="sm" label={String(view.badge)} />
  {/if}
</SplitButton>

<style>
  .gsync-label {
    white-space: nowrap;
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* The one rule that has to reach inside the widget.
     `SplitButton` already carries the colour to its root as `--split-color`
     (that is what its `color` prop is for) but only its `primary` variant
     consumes it, as a fill. This control needs the tint on the *text* of a
     bordered button — which is what the prop's own doc comment promises for
     non-primary variants and the stylesheet never implemented. Passing the
     class through the widget's `class` prop and completing the rule here is
     the documented escape hatch; forking the widget would not be.

     The `:hover` twin is not redundant: the `secondary` variant repaints the
     label `--text-primary` on hover at a higher specificity than the base rule,
     so without it the state colour would vanish under the pointer — which is
     exactly when it is being read. */
  :global(.gsync.split-has-color .split-main:not(:disabled)),
  :global(.gsync.split-has-color .split-main:not(:disabled):hover) {
    color: var(--split-color);
  }
</style>
