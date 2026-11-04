<script lang="ts">
  /** RequestSettingsModal — timeout, redirects and TLS verification for the open request. */
  import { Settings2 } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const tab = hirundoStore.active;

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); onClose(); }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} width="540px" height="470px" ariaLabel="Request settings">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Settings2 size={14} />
      <span class="modal-title">Request settings</span>
      {#if tab}<span class="rs-sub">{tab.request.name}</span>{/if}
    </ModalHeader>
  {/snippet}

  {#if tab}
    <div class="rs">
      <FormSection label="Network" first>
        <FormRow label="Timeout" description="How long to wait for the whole answer, in seconds.">
          <NumberStepper
            value={Math.round(tab.request.settings.timeout_ms / 1000)}
            min={1}
            max={600}
            step={5}
            onchange={(v: number) => { tab.request.settings.timeout_ms = Math.max(1, v) * 1000; hirundoStore.touch(tab); }}
          />
        </FormRow>
        <FormRow label="Follow redirects" description="Up to 10 hops.">
          <Toggle checked={tab.request.settings.follow_redirects} ariaLabel="Follow redirects" onchange={(v) => { tab.request.settings.follow_redirects = v; hirundoStore.touch(tab); }} />
        </FormRow>
      </FormSection>
      <FormSection label="Cookies">
        <FormRow label="Use the cookie jar" description="Send the cookies of the active environment, and keep the ones the answer sets.">
          <Toggle checked={tab.request.settings.cookie_jar} ariaLabel="Use the cookie jar" onchange={(v) => { tab.request.settings.cookie_jar = v; hirundoStore.touch(tab); }} />
        </FormRow>
      </FormSection>
      <FormSection label="Security">
        <FormRow label="Verify TLS certificates" description="Turn off only for a development server with a self-signed certificate.">
          <Toggle checked={tab.request.settings.verify_tls} ariaLabel="Verify TLS certificates" onchange={(v) => { tab.request.settings.verify_tls = v; hirundoStore.touch(tab); }} />
        </FormRow>
        {#if !tab.request.settings.verify_tls}
          <Alert variant="warning" compact text="Certificates are not checked for this request: anyone on the path could answer in place of the server." />
        {/if}
      </FormSection>
    </div>
  {/if}

  {#snippet footer()}
    <ModalFooter hint="to close">
      <Button variant="primary" onclick={onClose}>Done</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .rs { display: flex; flex-direction: column; gap: 4px; padding: 4px 16px 12px; }
  .rs-sub { font-size: var(--font-size-xs); color: var(--text-muted); margin-left: 8px; }
</style>
