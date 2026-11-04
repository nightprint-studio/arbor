<script lang="ts">
  /**
   * EnvironmentsModal — environments, their types, and this machine's own values, edited together and saved
   * together (one Ctrl+Enter). Three tabs:
   *
   *  • **Environments** — `staging-eu`, `prod`, your laptop: a name, a type, variables;
   *  • **Types** — what kinds of place there are (local, dev, staging, prod and your own): a colour, whether
   *    changing requests ask first, default variables;
   *  • **Local** — variables that exist only on this machine and are never committed.
   *
   * Nothing is written until Save; Cancel drops every edit. Removing a type does not remove its
   * environments — they just lose the colour and the protection.
   */
  import { Copy, Globe, Layers, Plus, Trash2, User } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { environmentDelete, environmentSave, envTypesSave, localSave } from '$lib/ipc/hirundo';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import type { EnvType, Environment, Local } from '$lib/types/hirundo';
  import KeyValueEditor from '../editor/KeyValueEditor.svelte';
  import { idFromName, themeHex } from '../request-model';
  import EnvEditor from './EnvEditor.svelte';
  import SecretValues from './SecretValues.svelte';
  import TypeEditor from './TypeEditor.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const clone = <T,>(v: T): T => JSON.parse(JSON.stringify(v)) as T;

  // svelte-ignore state_referenced_locally
  let types = $state<EnvType[]>(clone(hirundoStore.envs.types));
  // svelte-ignore state_referenced_locally
  let environments = $state<Environment[]>(clone(hirundoStore.envs.environments));
  // svelte-ignore state_referenced_locally
  let local = $state<Local>(clone(hirundoStore.envs.local));
  const originalEnvIds = hirundoStore.envs.environments.map((e) => e.id);
  const originalTypeIds = hirundoStore.envs.types.map((t) => t.id);

  let tab = $state<'environments' | 'types' | 'local'>('environments');
  let envIndex = $state(Math.max(0, environments.findIndex((e) => e.id === local.active_env)));
  let typeIndex = $state(0);
  let saving = $state(false);
  /** What the backend refused (a bad colour, two types with one id), said where it can be fixed. */
  let error = $state('');
  const projectName = hirundoStore.project?.split(/[\\/]/).filter(Boolean).pop() ?? '';

  const isNewEnv = (e: Environment) => !originalEnvIds.includes(e.id);
  const isNewType = (t: EnvType) => !originalTypeIds.includes(t.id);

  function uniqueId(base: string, taken: string[]): string {
    const root = idFromName(base) || 'item';
    let id = root;
    for (let n = 2; taken.includes(id); n++) id = `${root}-${n}`;
    return id;
  }

  function addEnvironment() {
    const name = 'New environment';
    environments.push({ id: uniqueId(name, environments.map((e) => e.id)), name, type_id: types[0]?.id ?? '', vars: [] });
    envIndex = environments.length - 1;
  }

  function duplicateEnvironment(i: number) {
    const src = environments[i];
    const name = `${src.name} copy`;
    environments.push({ ...clone(src), id: uniqueId(name, environments.map((e) => e.id)), name });
    envIndex = environments.length - 1;
  }

  function renameEnvironment(i: number, name: string) {
    const e = environments[i];
    e.name = name;
    if (isNewEnv(e)) e.id = uniqueId(name, environments.filter((_, at) => at !== i).map((x) => x.id));
  }

  function removeEnvironment(i: number) {
    const [gone] = environments.splice(i, 1);
    if (local.active_env === gone.id) local.active_env = '';
    envIndex = Math.max(0, Math.min(envIndex, environments.length - 1));
  }

  function addType() {
    const name = 'New type';
    types.push({ id: uniqueId(name, types.map((t) => t.id)), name, color: themeHex('--ws-color-3'), confirm_mutating: false, vars: [] });
    typeIndex = types.length - 1;
  }

  function renameType(i: number, name: string) {
    const t = types[i];
    t.name = name;
    if (isNewType(t)) {
      const id = uniqueId(name, types.filter((_, at) => at !== i).map((x) => x.id));
      for (const e of environments) if (e.type_id === t.id) e.type_id = id;
      t.id = id;
    }
  }

  function removeType(i: number) {
    types.splice(i, 1);
    typeIndex = Math.max(0, Math.min(typeIndex, types.length - 1));
  }

  async function save() {
    const project = hirundoStore.project;
    if (!project || saving) return;
    saving = true;
    error = '';
    try {
      await envTypesSave(project, types);
      for (const e of environments) await environmentSave(project, e);
      for (const id of originalEnvIds) if (!environments.some((e) => e.id === id)) await environmentDelete(project, id);
      await localSave(project, local);
      await hirundoStore.refreshEnvs();
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      void save();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} width="980px" height="680px" ariaLabel="Environments">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Globe size={14} />
      <span class="modal-title">Environments</span>
      {#if projectName}<span class="em-sub">{projectName}</span>{/if}
    </ModalHeader>
  {/snippet}

  <div class="em">
    {#if error}<div class="em-error"><Alert variant="error" compact text={error} dismissible onclose={() => (error = '')} /></div>{/if}
    <div class="em-tabs">
      <Tabs
        items={[{ id: 'environments', label: 'Environments', icon: Globe, iconSize: 13, badge: environments.length }, { id: 'types', label: 'Types', icon: Layers, iconSize: 13, badge: types.length }, { id: 'local', label: 'Local values', icon: User, iconSize: 13 }]}
        value={tab}
        variant="underline"
        size="sm"
        ariaLabel="Environment sections"
        onSelect={(id) => (tab = id as typeof tab)}
      />
    </div>

    {#if tab === 'environments'}
      <div class="em-cols">
        <div class="em-list">
          {#each environments as e, i (e.id)}
            <SidebarItem selected={i === envIndex} onclick={() => (envIndex = i)}>
              <span class="em-dot" style:background={types.find((t) => t.id === e.type_id)?.color ?? 'var(--text-muted)'}></span>
              {e.name}
              {#snippet badges()}<span class="em-type">{types.find((t) => t.id === e.type_id)?.name ?? ''}</span>{/snippet}
              {#snippet actions()}<IconButton tooltip="Duplicate" size={20} onclick={(ev) => { ev.stopPropagation(); duplicateEnvironment(i); }}><Copy size={12} /></IconButton><IconButton tooltip="Remove" size={20} variant="danger" onclick={(ev) => { ev.stopPropagation(); removeEnvironment(i); }}><Trash2 size={12} /></IconButton>{/snippet}
            </SidebarItem>
          {/each}
          <div class="em-add"><Button variant="outline" size="sm" onclick={addEnvironment}>{#snippet iconStart()}<Plus size={12} />{/snippet}Add environment</Button></div>
        </div>
        <div class="em-edit">
          {#if environments[envIndex]}
            {#key envIndex}
              <EnvEditor bind:env={environments[envIndex]} {types} isNew={isNewEnv(environments[envIndex])} onrename={(n) => renameEnvironment(envIndex, n)} />
            {/key}
          {:else}
            <div class="em-intro">
              <EmptyState message="No environments yet" description="An environment is a place the API runs — your machine, staging, production — and holds the values that change between them." />
              <ol class="em-steps">
                <li>Add an environment and pick its <b>type</b> (it sets the colour and whether to ask before changing anything).</li>
                <li>Give it a variable <code>baseUrl</code> with the address of the server, for example <code>https://staging.example.com</code>.</li>
                <li>Write requests as <code>{'{{'}baseUrl{'}}'}/orders</code>: the same request then goes wherever the selector in the title bar says.</li>
              </ol>
              <p class="em-note">A name is looked up in layers, the first that defines it winning: this machine, the environment, its type, the folders, the collection.</p>
            </div>
          {/if}
        </div>
      </div>
    {:else if tab === 'types'}
      <div class="em-cols">
        <div class="em-list">
          {#each types as t, i (t.id)}
            <SidebarItem selected={i === typeIndex} onclick={() => (typeIndex = i)}>
              <span class="em-dot" style:background={t.color}></span>
              {t.name}
              {#snippet actions()}<IconButton tooltip="Remove" size={20} variant="danger" onclick={(ev) => { ev.stopPropagation(); removeType(i); }}><Trash2 size={12} /></IconButton>{/snippet}
            </SidebarItem>
          {/each}
          <div class="em-add"><Button variant="outline" size="sm" onclick={addType}>{#snippet iconStart()}<Plus size={12} />{/snippet}Add type</Button></div>
        </div>
        <div class="em-edit">
          {#if types[typeIndex]}
            {#key typeIndex}
              <TypeEditor bind:type={types[typeIndex]} isNew={isNewType(types[typeIndex])} onrename={(n) => renameType(typeIndex, n)} />
            {/key}
          {:else}
            <EmptyState message="No types" description="Types give environments a colour, a protection and default variables." />
          {/if}
        </div>
      </div>
    {:else}
      <div class="em-edit em-local">
        <p class="em-note">Values that exist only on this machine — never committed. They beat the collection, the type and the environment. A test that calls <code>hirundo.env.set</code> writes here.</p>
        <KeyValueEditor rows={local.vars} allowSecret onchange={(rows) => (local.vars = rows)} />
        <SecretValues scope="local" id="local" rows={local.vars} />
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter hint="to save">
      <Button variant="ghost" onclick={onClose}>Cancel</Button>
      <Button variant="primary" loading={saving} onclick={() => void save()}>Save</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .em { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .em-tabs { padding: 0 12px; border-bottom: 1px solid var(--border-subtle); }
  .em-cols { display: grid; grid-template-columns: 230px 1fr; flex: 1; min-height: 0; }
  .em-list { border-right: 1px solid var(--border-subtle); padding: 8px 4px; overflow: auto; }
  .em-add { padding: 8px; }
  .em-edit { overflow: auto; min-width: 0; }
  .em-local { padding: 8px 0; }
  .em-intro { display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 24px 32px; max-width: 520px; margin: 0 auto; }
  .em-steps { margin: 0; padding-left: 20px; font-size: var(--font-size-sm); color: var(--text-secondary); display: flex; flex-direction: column; gap: 6px; }
  .em-steps code { font-family: var(--font-mono); }
  .em-note { margin: 0 12px 8px; font-size: var(--font-size-xs); color: var(--text-muted); }
  .em-note code { font-family: var(--font-mono); }
  .em-sub { font-size: var(--font-size-xs); color: var(--text-muted); margin-left: 8px; }
  .em-type { font-size: var(--font-size-xs); color: var(--text-muted); }
  .em-error { padding: 8px 12px 0; }
  .em-dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; margin-right: 6px; }
</style>
