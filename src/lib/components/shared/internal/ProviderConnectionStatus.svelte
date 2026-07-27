<script lang="ts">
  /**
   * The right-hand action area of a provider card in the settings sections.
   * Renders one of four states with the same visual language across every
   * OAuth-style integration (Git hosts, issue trackers, …):
   *
   *   - `checking`      → "Checking…" with a spinner
   *   - `connected`     → "Connected" badge + Disconnect button
   *   - `connecting`    → "<connectingLabel>" + Cancel button
   *   - `disconnected`  → consumer-supplied connect snippet (e.g. SplitButton)
   *
   * The same connect/disconnect button styling is reused inside the inline
   * forms below the status row — keeping it here means every provider gets
   * identical layout/typography out of the box.
   */
  import { CheckCircle2 } from 'lucide-svelte';
  import type { Snippet } from 'svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';

  export type ConnState = 'checking' | 'disconnected' | 'connecting' | 'connected';

  interface Props {
    state: ConnState;
    /** Label shown next to the spinner while `state === 'connecting'`.
     *  Default mirrors the most common phrasing ("Waiting…"); override for
     *  device-flow-style providers ("Waiting for authorisation…") or
     *  browser-redirect ones ("Waiting for browser…"). */
    connectingLabel?: string;
    /** Label for the connected badge — almost always "Connected". */
    connectedLabel?: string;
    /** Fired when the user clicks Disconnect while connected. */
    onDisconnect: () => void;
    /** Fired when the user clicks Cancel while connecting. */
    onCancel: () => void;
    /** Rendered when `state === 'disconnected'`. Typically a SplitButton or
     *  the legacy split-btn-wrap. */
    connect?: Snippet;
  }

  let {
    state,
    connectingLabel = 'Waiting…',
    connectedLabel  = 'Connected',
    onDisconnect,
    onCancel,
    connect,
  }: Props = $props();
</script>

<div class="provider-action">
  {#if state === 'checking'}
    <span class="status-checking"><Spinner size={12} /> Checking…</span>
  {:else if state === 'connected'}
    <span class="status-ok"><CheckCircle2 size={12} /> {connectedLabel}</span>
    <Button variant="outline" size="sm" color="var(--error)" onclick={onDisconnect}>Disconnect</Button>
  {:else if state === 'connecting'}
    <span class="status-wait"><Spinner size={12} /> {connectingLabel}</span>
    <Button variant="outline" size="sm" onclick={onCancel}>Cancel</Button>
  {:else if connect}
    {@render connect()}
  {/if}
</div>

<style>
  /* Cancel / Disconnect are the shared `Button` in its `outline` variant —
     Disconnect tinted `--error` so it stays neutral until hovered. They used to
     be bare `<button class="btn-ghost-danger">` styled by `SettingsPanel`'s
     scoped globals, which made a SHARED widget render correctly only inside
     that one consumer: mounted anywhere else (the cross-product Credentials
     modal) Disconnect lost its colours and came out plain white. */
  .provider-action {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .status-checking,
  .status-wait {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text-muted);
  }
  .status-ok {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    font-weight: 500;
    color: var(--success, #6aab73);
  }
</style>
