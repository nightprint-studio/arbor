<script lang="ts">
  /**
   * Whether a connection's session is open — readable **without hovering anything**.
   *
   * The sidebar used to say this only through the connect/disconnect button, which
   * lives in the row's hover actions: with several connections configured, the
   * answer to "which of these is actually open" needed a pass with the mouse over
   * every row. It is a fact about the row, so it belongs beside the row's name.
   *
   * ## The dot is round, and round means state
   *
   * Picus draws a connection's **identity** as a coloured marker too — its slot in
   * the shared workspace palette, on the pill and on every tab bound to it — and one
   * of the twelve palette colours (`--ws-color-2`) is the same green as `--success`.
   * A round identity marker would therefore be indistinguishable from this one, so
   * the two vocabularies are split by SHAPE: identity is a bar, state is a circle.
   * Keep it that way — a green circle in Picus means connected and nothing else.
   *
   * ## Colour is never the only signal
   *
   * Off is **hollow** and on is **filled**, so the two survive a monochrome screen
   * and a colour-blind reader; the colour is what makes it fast, not what makes it
   * legible. The label is on the element for a screen reader and in a tooltip for
   * everyone else — the tooltip adds detail, it does not carry the meaning.
   */
  import { tooltip } from '$lib/actions/tooltip';
  import type { ConnectionState } from '$lib/types/picus';

  interface Props {
    state: ConnectionState;
  }

  let { state }: Props = $props();

  /** `read-only` is an OPEN session that refuses writes — the lock badge says the
   *  rest. This dot answers one question only: is there a session at all. */
  const open = $derived(state === 'connected' || state === 'read-only');
  const busy = $derived(state === 'connecting');
  const label = $derived(busy ? 'Connecting…' : open ? 'Connected' : 'Not connected');
</script>

<span
  class="csd"
  class:csd-on={open}
  class:csd-busy={busy}
  role="img"
  aria-label={label}
  use:tooltip={label}
></span>

<style>
  .csd {
    width: 7px;
    height: 7px;
    box-sizing: border-box;
    border-radius: 50%;
    border: 1.5px solid var(--text-disabled);
    background: transparent;
    flex-shrink: 0;
  }
  .csd-on {
    border-color: var(--success);
    background: var(--success);
  }
  .csd-busy {
    border-color: var(--warning);
    background: var(--warning);
    animation: csd-pulse 1.1s ease-in-out infinite;
  }
  @keyframes csd-pulse {
    0%, 100% { opacity: 1; }
    50%      { opacity: 0.3; }
  }
  /* The pulse is the nicety; the colour and the fill already say "connecting". */
  @media (prefers-reduced-motion: reduce) {
    .csd-busy { animation: none; }
  }
</style>
