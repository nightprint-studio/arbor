<script lang="ts">
  /** Name a new test template, created as a copy of `from` and opened in the editor. */
  import { FilePlus2 } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { bennuDtoLabStore as lab } from '$lib/stores/bennu/dtolab.svelte';

  let { from, onClose }: { from: string; onClose: () => void } = $props();

  let name = $state('');
  let saving = $state(false);

  const trimmed = $derived(name.trim());
  const valid = $derived(/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(trimmed) && trimmed !== 'default');

  async function create() {
    if (!valid || saving) return;
    saving = true;
    const created = await lab.newTemplate(trimmed, from);
    saving = false;
    if (created) onClose();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      void create();
    }
  }
</script>

<Modal {onClose} width="520px" height="270px" ariaLabel="New test template">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FilePlus2 size={14} />
      <span class="modal-title">New test template</span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" onkeydown={onKeydown}>
    <FormField
      label="Name"
      hint={`A copy of “${from}”, kept in your profile for every project, and opened in the editor.`}
      error={trimmed && !valid ? 'Letters, digits, ".", "-" and "_" — and not "default", which is the built-in one.' : null}
    >
      <Input bind:value={name} placeholder="team-style" autofocus />
    </FormField>
  </div>

  {#snippet footer()}
    <ModalFooter>
      <Button variant="ghost" onclick={onClose}>Cancel</Button>
      <Button variant="primary" loading={saving} disabled={!valid} onclick={() => void create()}>Create and open</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .body { display: flex; flex-direction: column; gap: 10px; }
</style>
