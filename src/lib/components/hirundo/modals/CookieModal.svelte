<script lang="ts">
  /**
   * CookieModal — add or edit one cookie of the active environment's jar: name, value, where it applies
   * (domain, path, and whether sub-domains count), when it expires, and its Secure / HttpOnly marks. Changing
   * the name, domain or path of an existing cookie makes it a new one (the old is removed).
   */
  import { onMount } from 'svelte';
  import { Cookie as CookieIcon } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { cookieDelete, cookieSave } from '$lib/ipc/hirundo';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import type { Cookie } from '$lib/types/hirundo';
  import { fromLocalInput, toLocalInput } from '../schedule-model';

  interface Props {
    /** The cookie to edit; `null` for a new one. */
    cookie: Cookie | null;
    envId: string;
    onClose: () => void;
    onSaved: () => void;
  }

  let { cookie, envId, onClose, onSaved }: Props = $props();

  // svelte-ignore state_referenced_locally
  let draft = $state<Cookie>(cookie ? { ...cookie } : { name: '', value: '', domain: '', path: '/', expires_ms: null, secure: false, http_only: false, host_only: true });
  let session = $state(cookie ? cookie.expires_ms === null : true);
  let error = $state('');
  let nameEl = $state<HTMLInputElement | undefined>(undefined);
  onMount(() => nameEl?.focus());

  async function save() {
    const project = hirundoStore.project;
    if (!project) return;
    error = '';
    try {
      const next: Cookie = { ...draft, domain: draft.domain.trim().toLowerCase().replace(/^\./, ''), expires_ms: session ? null : (draft.expires_ms ?? Date.now() + 86400_000) };
      if (cookie && (cookie.name !== next.name || cookie.domain !== next.domain || cookie.path !== next.path)) await cookieDelete(project, envId, cookie);
      await cookieSave(project, envId, next);
      onSaved();
      onClose();
    } catch (e) {
      error = String(e);
    }
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); void save(); }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} width="560px" height="560px" ariaLabel="Cookie">
  {#snippet header()}
    <ModalHeader {onClose}>
      <CookieIcon size={14} />
      <span class="modal-title">{cookie ? 'Edit cookie' : 'New cookie'}</span>
    </ModalHeader>
  {/snippet}

  <div class="ck">
    {#if error}<Alert variant="error" compact text={error} dismissible onclose={() => (error = '')} />{/if}
    <FormSection label="Cookie" first>
      <FormField label="Name"><Input value={draft.name} bind:element={nameEl} oninput={(v) => (draft.name = v)} /></FormField>
      <FormField label="Value"><Input value={draft.value} oninput={(v) => (draft.value = v)} /></FormField>
    </FormSection>
    <FormSection label="Where it applies">
      <FormField label="Domain" hint="Without a leading dot: example.com"><Input value={draft.domain} placeholder="example.com" oninput={(v) => (draft.domain = v)} /></FormField>
      <FormField label="Path"><Input value={draft.path} oninput={(v) => (draft.path = v || '/')} /></FormField>
      <FormRow label="Also sub-domains" description="Off: only the exact host.">
        <Toggle checked={!draft.host_only} ariaLabel="Also sub-domains" onchange={(v) => (draft.host_only = !v)} />
      </FormRow>
    </FormSection>
    <FormSection label="Lifetime and marks">
      <FormRow label="Session cookie" description="Kept until the jar is cleared.">
        <Toggle checked={session} ariaLabel="Session cookie" onchange={(v) => (session = v)} />
      </FormRow>
      {#if !session}
        <FormField label="Expires"><Input type="datetime-local" value={toLocalInput(draft.expires_ms ?? Date.now() + 86400_000)} oninput={(v) => { const ms = fromLocalInput(v); if (ms !== null) draft.expires_ms = ms; }} /></FormField>
      {/if}
      <FormRow label="Secure" description="Sent over https only."><Toggle checked={draft.secure} ariaLabel="Secure" onchange={(v) => (draft.secure = v)} /></FormRow>
      <FormRow label="HttpOnly" description="A mark for the record; Hirundo is not a script."><Toggle checked={draft.http_only} ariaLabel="HttpOnly" onchange={(v) => (draft.http_only = v)} /></FormRow>
    </FormSection>
  </div>

  {#snippet footer()}
    <ModalFooter hint="to save">
      <Button variant="ghost" onclick={onClose}>Cancel</Button>
      <Button variant="primary" disabled={!draft.name.trim() || !draft.domain.trim()} onclick={() => void save()}>Save</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .ck { display: flex; flex-direction: column; gap: 4px; padding: 4px 16px 12px; overflow: auto; height: 100%; }
</style>
