<script lang="ts">
  import { UserCircle, AlertTriangle } from 'lucide-svelte';
  import Modal from '../shared/Modal.svelte';
  import ModalHeader from '../shared/ModalHeader.svelte';
  import Button from '../shared/ui/Button.svelte';
  import FormField from '../shared/ui/FormField.svelte';
  import Input from '../shared/ui/Input.svelte';
  import { getGitIdentity, setGitIdentity } from '$lib/ipc/corvus/graph';

  // Shown when an operation (typically a commit) needs a configured git
  // identity and none is set. Collects user.name / user.email and writes them
  // to the global git config; `onSaved` lets the caller retry the operation.
  let {
    reason = 'Git needs an author name and email before you can commit.',
    onSaved,
    onCancel,
  }: {
    reason?:  string;
    onSaved:  () => void;
    onCancel: () => void;
  } = $props();

  let name    = $state('');
  let email   = $state('');
  let saving  = $state(false);
  let error   = $state<string | null>(null);

  // Prefill whatever partial identity is already configured (e.g. only the
  // name is set) so the user only fills the missing half.
  $effect(() => {
    getGitIdentity()
      .then(([n, e]) => { if (n) name = n; if (e) email = e; })
      .catch(() => { /* leave blank */ });
  });

  const canSave = $derived(name.trim().length > 0 && email.trim().length > 0 && !saving);

  async function save() {
    if (!canSave) return;
    saving = true;
    error  = null;
    try {
      await setGitIdentity(name.trim(), email.trim());
      onSaved();
    } catch (err) {
      error  = String(err);
      saving = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key !== 'Enter' || e.shiftKey) return;
    const t = e.target as HTMLElement | null;
    if (t instanceof HTMLButtonElement) return;
    if (canSave) { e.preventDefault(); save(); }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<Modal onClose={onCancel} width="520px" ariaLabel="Set Git identity">
  {#snippet header()}
    <ModalHeader onClose={onCancel}>
      <div class="header-icon"><UserCircle size={16} /></div>
      <div class="header-text">
        <span class="modal-title">Set your Git identity</span>
        <span class="header-sub">{reason}</span>
      </div>
    </ModalHeader>
  {/snippet}

  <div class="gi-body">
    <FormField label="Name">
      <Input placeholder="Your Name" bind:value={name} />
    </FormField>
    <FormField label="Email">
      <Input type="email" placeholder="you@example.com" bind:value={email} />
    </FormField>

    <p class="gi-note">
      Saved to your global git config (<code>~/.gitconfig</code>), so it applies to every repository.
    </p>

    {#if error}
      <div class="gi-error">
        <AlertTriangle size={13} />
        <span>{error}</span>
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <Button variant="ghost" onclick={onCancel}>Cancel</Button>
    <Button variant="primary" disabled={!canSave} loading={saving} onclick={save}>
      Save &amp; commit
    </Button>
  {/snippet}
</Modal>

<style>
  .header-icon {
    width: 28px; height: 28px;
    border-radius: var(--radius-md);
    background: var(--accent-subtle);
    color: var(--accent);
    display: flex; align-items: center; justify-content: center;
    flex-shrink: 0;
  }
  .header-text { display: flex; flex-direction: column; gap: 3px; min-width: 0; flex: 1; }
  .header-sub  { font-size: var(--font-size-xs); color: var(--text-secondary); line-height: 1.4; white-space: normal; }

  .gi-body { display: flex; flex-direction: column; gap: 12px; }
  .gi-note {
    margin: 0;
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    line-height: 1.4;
  }
  .gi-note code {
    font-family: var(--font-code);
    background: var(--bg-base);
    padding: 1px 4px;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-2xs);
  }
  .gi-error {
    display: flex; gap: 6px; align-items: flex-start;
    font-size: var(--font-size-xs);
    color: var(--error);
    background: color-mix(in srgb, var(--error) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--error) 40%, transparent);
    border-radius: var(--radius-md);
    padding: 8px 10px;
  }
  .gi-error :global(svg) { flex-shrink: 0; margin-top: 1px; }
</style>
