<script lang="ts">
  import Modal from '../shared/Modal.svelte';
  import ModalHeader from '../shared/ModalHeader.svelte';
  import Button from '../shared/ui/Button.svelte';
  import FormField from '../shared/ui/FormField.svelte';
  import Input from '../shared/ui/Input.svelte';
  import TicketPickerModal from '../shared/TicketPickerModal.svelte';
  import { TicketCheck, Search } from 'lucide-svelte';
  import type { CommitNode } from '$lib/types/git';
  import type { Issue } from '$lib/types/issues';
  import { getRepoConfig } from '$lib/ipc/config';

  let {
    node,
    tabId,
    onClose,
    onCreate,
  }: {
    node:     CommitNode;
    tabId?:   string;
    onClose:  () => void;
    onCreate: (name: string) => void;
  } = $props();

  let name           = $state('');
  let ticketSelected = $state<Issue | null>(null);
  let ticketModalOpen = $state(false);
  let issueTracker   = $state<string | undefined>(undefined);

  // Load issue tracker config lazily
  $effect(() => {
    if (tabId) {
      getRepoConfig(tabId)
        .then(cfg => { issueTracker = cfg?.issue_tracker ?? undefined; })
        .catch(() => {});
    }
  });

  function onNameInput() {
    // Clear ticket assignment if user manually edits the name
    if (ticketSelected && name !== ticketSelected.identifier) {
      ticketSelected = null;
    }
  }

  function onTicketSelected(issue: Issue) {
    ticketSelected  = issue;
    name            = issue.identifier;
    ticketModalOpen = false;
  }
</script>

<Modal {onClose} ariaLabel="New Branch">
  {#snippet header()}
    <ModalHeader title="New Branch" {onClose} />
  {/snippet}
  <div style="display:flex; flex-direction:column; gap:12px">
    <p style="color:var(--text-secondary); font-size:var(--font-size-sm)">
      Create branch from commit <code style="font-family:var(--font-code)">{node.short_oid}</code>
    </p>

    <!-- Ticket picker (shown when issue tracker is configured) -->
    {#if issueTracker}
      <FormField label="Ticket" optionalText="(optional)">
        {#snippet icon()}<TicketCheck size={11} />{/snippet}
        <button
          type="button"
          class="ticket-pick-btn"
          onclick={() => (ticketModalOpen = true)}
        >
          {#if ticketSelected}
            <span class="ticket-pick-id">{ticketSelected.identifier}</span>
            <span class="ticket-pick-title">{ticketSelected.title}</span>
            <span class="ticket-pick-change">change</span>
          {:else}
            <Search size={10} style="opacity:0.5" />
            <span style="color:var(--text-muted)">Search issues…</span>
          {/if}
        </button>
      </FormField>
    {/if}

    <!-- Branch name input -->
    <FormField label={issueTracker ? 'Branch name' : undefined}>
      <Input
        placeholder="Branch name…"
        bind:value={name}
        oninput={onNameInput}
        onkeydown={(e) => e.key === 'Enter' && name.trim() && onCreate(name.trim())}
        autofocus={!issueTracker}
      />
    </FormField>
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={onClose}>Cancel</Button>
    <Button variant="primary" onclick={() => onCreate(name.trim())} disabled={!name.trim()}>Create</Button>
  {/snippet}
</Modal>

{#if ticketModalOpen}
  <TicketPickerModal
    onSelect={onTicketSelected}
    onClose={() => (ticketModalOpen = false)}
  />
{/if}

<style>
  .ticket-pick-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 6px 10px;
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    color: var(--text-primary);
    cursor: pointer;
    text-align: left;
    transition: background var(--transition-fast), border-color var(--transition-fast);
    min-width: 0;
  }
  .ticket-pick-btn:hover { background: var(--bg-hover); border-color: var(--accent); }
  .ticket-pick-id {
    flex-shrink: 0;
    font-family: var(--font-code);
    font-size: 10px;
    font-weight: 600;
    color: var(--accent);
    background: var(--accent-subtle);
    border: 1px solid rgba(77,120,204,0.25);
    border-radius: var(--radius-sm);
    padding: 0 4px;
    line-height: 15px;
  }
  .ticket-pick-title {
    flex: 1;
    min-width: 0;
    font-size: 11px;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ticket-pick-change {
    flex-shrink: 0;
    font-size: 9px;
    color: var(--text-muted);
    margin-left: auto;
  }
</style>
