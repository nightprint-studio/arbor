<script lang="ts">
  /**
   * A file the agent opened for itself, because its own build reported an error there (Autonomous mode).
   *
   * Nothing is asked, so the thread has to say it: which file, why, and that the access is for this run
   * only. While the run goes on it can be taken back; edits already made stay, what ends is the right to
   * touch the file again. When the run is over the card is a record of what was opened.
   */
  import { KeyRound, Undo2 } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import type { AccessGrant, GrantState } from './assistant-types';

  let { grant }: { grant: AccessGrant } = $props();

  const STATE: Record<Exclude<GrantState, 'active'>, { label: string; tone: 'neutral' | 'info' }> = {
    revoked: { label: 'Revoked', tone: 'neutral' },
    expired: { label: 'Ended with the run', tone: 'info' },
  };
</script>

<div class="grant" class:ended={grant.state !== 'active'}>
  <KeyRound size={12} />
  <span class="text">
    <span class="reason">{grant.reason}</span>
    <code class="file">{grant.file}</code>
  </span>
  {#if grant.state === 'active'}
    <Button variant="ghost" size="sm" tooltip="Take the access back — edits already made stay" onclick={() => bennuAssistantStore.revokeGrant(grant.id)}>
      <Undo2 size={12} /> Revoke
    </Button>
  {:else}
    <Badge variant="tone" tone={STATE[grant.state].tone} size="sm">{STATE[grant.state].label}</Badge>
  {/if}
</div>

<style>
  .grant {
    display: flex; align-items: center; gap: 8px;
    padding: 7px 8px 7px 11px;
    border: 1px solid color-mix(in srgb, var(--warning) 35%, var(--border-subtle));
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--warning) 6%, var(--bg-elevated));
    color: var(--warning);
  }
  .grant.ended { border-color: var(--border-subtle); background: var(--bg-elevated); color: var(--text-muted); }
  .text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .reason { font-size: var(--font-size-xs); color: var(--text-secondary); }
  .file { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-primary); overflow-wrap: anywhere; }
</style>
