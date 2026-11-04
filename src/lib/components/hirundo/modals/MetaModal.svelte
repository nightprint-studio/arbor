<script lang="ts">
  /**
   * MetaModal — the settings of a collection or folder: auth for everything below it (a request may
   * inherit, replace or switch it off), headers every request gets, and variables for the whole subtree.
   * At the collection there is nothing above to inherit from, so "inherit" is not offered there.
   */
  import { Heading, KeyRound, Library, Folder, Variable } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { metaGet, metaSave } from '$lib/ipc/hirundo';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import type { Meta } from '$lib/types/hirundo';
  import AuthEditor from '../editor/AuthEditor.svelte';
  import KeyValueEditor from '../editor/KeyValueEditor.svelte';
  import SecretValues from '../env/SecretValues.svelte';
  import { authLabel } from '../request-model';

  let { path, onClose }: { path: string; onClose: () => void } = $props();

  let meta = $state<Meta | null>(null);
  let tab = $state('auth');
  let nameEl = $state<HTMLInputElement | undefined>(undefined);
  const isCollection = $derived(!path.includes('/'));
  const count = (rows: { key: string; enabled: boolean }[]) => rows.filter((r) => r.key && r.enabled).length || undefined;

  $effect(() => {
    const project = hirundoStore.project;
    if (!project) return;
    void metaGet(project, path).then((m) => (meta = m)).catch((e) => { toastStore.show(String(e), 'error'); onClose(); });
  });
  $effect(() => { if (meta) nameEl?.focus(); });

  async function save() {
    const project = hirundoStore.project;
    if (!project || !meta) return;
    try {
      await metaSave(project, path, meta);
      await hirundoStore.refreshTree();
      onClose();
    } catch (e) {
      toastStore.show(String(e), 'error');
    }
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); void save(); }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} width="720px" height="580px" ariaLabel={isCollection ? 'Collection settings' : 'Folder settings'}>
  {#snippet header()}
    <ModalHeader {onClose}>
      {#if isCollection}<Library size={14} />{:else}<Folder size={14} />{/if}
      <span class="modal-title">{isCollection ? 'Collection settings' : 'Folder settings'}</span>
      {#if meta}<span class="mm-sub">{meta.name}</span>{/if}
    </ModalHeader>
  {/snippet}

  {#if !meta}
    <Spinner block delay={0} />
  {:else}
    <div class="mm">
      <div class="mm-name">
        <FormField label="Name"><Input value={meta.name} bind:element={nameEl} oninput={(v) => (meta!.name = v)} /></FormField>
        <div class="mm-cookies"><Toggle checked={!meta.no_cookies} label="Use the cookie jar" description="Off: no request under here sends or keeps cookies." onchange={(v) => (meta!.no_cookies = !v)} /></div>
      </div>
      <div class="mm-tabs">
        <Tabs
          items={[
            { id: 'auth', label: meta.auth.type === 'inherit' ? 'Auth' : `Auth · ${authLabel(meta.auth)}`, icon: KeyRound, iconSize: 13 },
            { id: 'headers', label: 'Headers', icon: Heading, iconSize: 13, badge: count(meta.headers) },
            { id: 'vars', label: 'Variables', icon: Variable, iconSize: 13, badge: count(meta.vars) },
          ]}
          value={tab}
          variant="underline"
          size="sm"
          ariaLabel="Settings sections"
          onSelect={(id) => (tab = id)}
        />
      </div>
      <div class="mm-body">
        {#if tab === 'auth'}
          <AuthEditor auth={meta.auth} allowInherit={!isCollection} inheritedLabel="the collection's auth" onchange={(a) => (meta!.auth = a)} />
        {:else if tab === 'headers'}
          <div class="mm-note"><Alert variant="info" compact text="Sent with every request under here. A request's own header of the same name wins." /></div>
          <KeyValueEditor rows={meta.headers} keyPlaceholder="Header" onchange={(rows) => (meta!.headers = rows)} />
        {:else}
          <div class="mm-note"><Alert variant="info" compact text="Visible to every request under here; an environment's variable of the same name wins." /></div>
          <KeyValueEditor rows={meta.vars} allowSecret onchange={(rows) => (meta!.vars = rows)} />
          <SecretValues scope={isCollection ? 'collection' : 'folder'} id={path} rows={meta.vars} />
        {/if}
      </div>
    </div>
  {/if}

  {#snippet footer()}
    <ModalFooter hint="to save">
      <Button variant="ghost" onclick={onClose}>Cancel</Button>
      <Button variant="primary" disabled={!meta} onclick={() => void save()}>Save</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .mm { display: flex; flex-direction: column; min-height: 0; height: 100%; }
  .mm-name { padding: 12px 16px 4px; }
  .mm-tabs { padding: 0 12px; border-bottom: 1px solid var(--border-subtle); }
  .mm-body { flex: 1; min-height: 0; overflow: auto; }
  .mm-cookies { margin-top: 10px; }
  .mm-note { padding: 10px 12px 0; }
  .mm-sub { font-size: var(--font-size-xs); color: var(--text-muted); margin-left: 8px; }
</style>
