<script lang="ts">
  /**
   * OpenApiImportModal — an OpenAPI 3 spec (JSON) becomes requests, and the user chooses which.
   *
   * Step one takes the spec as a file (Browse…), a URL or pasted text. Step two lists its operations grouped
   * by tag — a checkbox on each tag and each operation, a filter, all / none, a count — and says where they
   * go: a new collection, or a folder inside an existing one, named after the spec, with a sub-folder per tag.
   * The spec's server becomes `{{baseUrl}}` (in the active environment when it has none yet, else on the
   * collection) and its security scheme becomes the auth of the folder.
   */
  import { onMount } from 'svelte';
  import { FileJson, FolderSearch } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Checkbox from '$lib/components/shared/ui/Checkbox.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FilterInput from '$lib/components/shared/ui/FilterInput.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { environmentSave, openapiApply, openapiRead } from '$lib/ipc/hirundo';
  import { fuzzyMatch } from '$lib/utils/fuzzy';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import type { KeyValue, OpenApiPlan } from '$lib/types/hirundo';
  import { methodColor, methodShort } from '../request-model';

  let { onClose }: { onClose: () => void } = $props();

  let source = $state('');
  let sourceEl = $state<HTMLTextAreaElement | undefined>(undefined);
  let browsing = $state(false);
  let reading = $state(false);
  let error = $state('');
  let plan = $state<OpenApiPlan | null>(null);

  let chosen = $state<Record<string, boolean>>({});
  let query = $state('');
  let name = $state('');
  let into = $state<'new' | 'folder'>('new');
  let parent = $state(hirundoStore.tree[0]?.path ?? '');
  let saving = $state(false);

  onMount(() => sourceEl?.focus());

  const matches = (id: string, tag: string, summary: string) => !query.trim() || fuzzyMatch(`${id} ${summary} ${tag}`, query) !== null;

  /** The operations of the plan that the filter lets through, by tag. */
  const groups = $derived(
    (plan?.tags ?? [])
      .map((tag) => ({ tag, ops: (plan?.operations ?? []).filter((o) => o.tag === tag && matches(o.id, o.tag, o.summary)) }))
      .filter((g) => g.ops.length),
  );
  const visible = $derived(groups.flatMap((g) => g.ops));
  const total = $derived(plan?.operations.length ?? 0);
  const count = $derived((plan?.operations ?? []).filter((o) => chosen[o.id]).length);
  const collections = $derived(hirundoStore.tree.map((c) => ({ value: c.path, label: c.name })));

  async function read() {
    if (!source.trim() || reading) return;
    reading = true;
    error = '';
    try {
      plan = await openapiRead(source);
      chosen = Object.fromEntries(plan.operations.map((o) => [o.id, true]));
      name = plan.title;
      query = '';
    } catch (e) {
      plan = null;
      error = String(e);
    } finally {
      reading = false;
    }
  }

  function setMany(ids: string[], value: boolean) {
    for (const id of ids) chosen[id] = value;
  }

  /** Put `baseUrl` where the requests will find it: the active environment when it has none, else the collection. */
  async function baseUrlVars(project: string): Promise<KeyValue[]> {
    if (!plan?.base_url) return [];
    const kv: KeyValue = { key: 'baseUrl', value: plan.base_url, description: `From ${plan.title}`, enabled: true };
    const env = hirundoStore.activeEnv;
    if (!env) return [kv];
    if (!env.vars.some((v) => v.key === 'baseUrl')) {
      await environmentSave(project, { ...env, vars: [...env.vars, kv] });
      await hirundoStore.refreshEnvs();
    }
    return [];
  }

  async function save() {
    const project = hirundoStore.project;
    if (!project || !plan || saving || count === 0) return;
    saving = true;
    error = '';
    try {
      const vars = await baseUrlVars(project);
      const done = await openapiApply({
        project,
        parent: into === 'folder' ? parent : null,
        name: name.trim() || plan.title,
        description: plan.description,
        auth: plan.auth,
        vars,
        operations: plan.operations.filter((o) => chosen[o.id]),
      });
      await hirundoStore.refreshTree();
      hirundoUiStore.selectSection('collections');
      toastStore.show(`${done.count} request${done.count === 1 ? '' : 's'} imported.`, 'success');
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (browsing || !(e.ctrlKey || e.metaKey) || e.key !== 'Enter') return;
    e.preventDefault();
    if (plan) void save(); else void read();
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} width="820px" height="700px" padBody={false} ariaLabel="Import OpenAPI">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FileJson size={14} />
      <span class="modal-title">Import OpenAPI</span>
      {#if plan}<span class="oa-sub">{plan.title}</span>{/if}
    </ModalHeader>
  {/snippet}

  {#if !plan}
    <div class="oa-source">
      <FormSection label="Spec" hint="OpenAPI 3 in JSON: paste it, give its URL, or browse to the file. YAML is not supported yet." first>
        <Textarea bind:value={source} bind:element={sourceEl} minRows={8} maxRows={14} framed placeholder="https://api.example.com/openapi.json   or   C:\specs\openapi.json   or the JSON itself" />
        <div class="oa-row">
          <Button variant="outline" size="sm" onclick={() => (browsing = true)}>{#snippet iconStart()}<FolderSearch size={13} />{/snippet}Browse…</Button>
        </div>
      </FormSection>
      {#if error}<div class="oa-pad"><Alert variant="error" text={error} /></div>{/if}
    </div>
  {:else}
    <div class="oa-pick">
      <div class="oa-bar">
        <FilterInput bind:value={query} placeholder="Filter operations — path, summary, tag…" ariaLabel="Filter operations" />
        <Button variant="outline" size="sm" onclick={() => setMany(visible.map((o) => o.id), true)}>All</Button>
        <Button variant="outline" size="sm" onclick={() => setMany(visible.map((o) => o.id), false)}>None</Button>
        <span class="oa-count">{count} of {total} selected</span>
      </div>

      {#if plan.warnings.length}
        <div class="oa-pad">{#each plan.warnings as w (w)}<Alert variant="warning" compact text={w} />{/each}</div>
      {/if}

      <div class="oa-list">
        {#if groups.length === 0}
          <EmptyState message="No operation matches" compact />
        {/if}
        {#each groups as g (g.tag)}
          {@const ids = g.ops.map((o) => o.id)}
          {@const on = ids.filter((id) => chosen[id]).length}
          <Collapsible open chevron>
            {#snippet header()}
              <span class="oa-tag" role="presentation" onclick={(e) => e.stopPropagation()}>
                <Checkbox checked={on === ids.length} indeterminate={on > 0 && on < ids.length} ariaLabel={`Select every operation of ${g.tag}`} onchange={(v) => setMany(ids, v)} />
              </span>
              <b>{g.tag}</b>
              <span class="oa-muted">{on}/{ids.length}</span>
            {/snippet}
            <ul class="oa-ops">
              {#each g.ops as o (o.id)}
                <li>
                  <Checkbox checked={!!chosen[o.id]} ariaLabel={o.id} onchange={(v) => (chosen[o.id] = v)} />
                  <span class="oa-method" style:color={methodColor(o.request.method)}>{methodShort(o.request.method)}</span>
                  <code class="oa-path">{o.path}</code>
                  {#if o.summary}<span class="oa-muted oa-summary">{o.summary}</span>{/if}
                </li>
              {/each}
            </ul>
          </Collapsible>
        {/each}
      </div>

      <div class="oa-target">
        <FormSection label="Where">
          <div class="oa-where">
            <FormField label="Name"><Input value={name} placeholder={plan.title} oninput={(v) => (name = v)} /></FormField>
            <FormField label="Into">
              <RadioGroup value={into} options={[{ value: 'new', label: 'A new collection' }, { value: 'folder', label: 'A folder in…' }]} appearance="segment" size="sm" onchange={(v) => (into = v as typeof into)} />
            </FormField>
            {#if into === 'folder'}<FormField label="Collection"><Select value={parent} options={collections} onchange={(v) => (parent = v)} /></FormField>{/if}
          </div>
          {#if plan.base_url}<p class="oa-muted">{'{{baseUrl}}'} = <code>{plan.base_url}</code> · auth: {plan.auth.type === 'inherit' ? 'none in the spec' : plan.auth.type}</p>{/if}
          {#if error}<Alert variant="error" compact text={error} />{/if}
        </FormSection>
      </div>
    </div>
  {/if}

  {#snippet footer()}
    {#if plan}
      <ModalFooter hint="to import">
        <Button variant="ghost" onclick={() => (plan = null)}>Back</Button>
        <Button variant="primary" loading={saving} disabled={count === 0 || (into === 'folder' && !parent)} onclick={() => void save()}>{`Import ${count} operation${count === 1 ? '' : 's'}`}</Button>
      </ModalFooter>
    {:else}
      <ModalFooter hint="to read">
        <Button variant="ghost" onclick={onClose}>Cancel</Button>
        <Button variant="primary" loading={reading} disabled={!source.trim()} onclick={() => void read()}>Read</Button>
      </ModalFooter>
    {/if}
  {/snippet}
</Modal>

{#if browsing}
  <FileExplorerModal mode="file" extensions={['json']} title="Choose the OpenAPI file" onConfirm={(p: string) => { browsing = false; source = p; void read(); }} onCancel={() => (browsing = false)} onClose={() => (browsing = false)} />
{/if}

<style>
  .oa-source { padding: 4px 16px 12px; display: flex; flex-direction: column; gap: 6px; }
  .oa-row { display: flex; gap: 8px; margin-top: 6px; }
  .oa-pad { padding: 6px 16px 0; display: flex; flex-direction: column; gap: 4px; }
  .oa-sub { font-size: var(--font-size-xs); color: var(--text-muted); margin-left: 8px; }
  .oa-pick { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .oa-bar { display: flex; align-items: center; gap: 8px; padding: 10px 16px 6px; }
  .oa-bar > :global(:first-child) { flex: 1; min-width: 0; }
  .oa-count { font-size: var(--font-size-xs); color: var(--text-secondary); white-space: nowrap; }
  .oa-list { flex: 1; min-height: 0; overflow: auto; padding: 4px 16px; }
  .oa-tag { display: inline-flex; margin-right: 6px; }
  .oa-ops { list-style: none; margin: 0; padding: 2px 0 8px 22px; display: flex; flex-direction: column; gap: 2px; }
  .oa-ops li { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .oa-method { font: 600 10px var(--font-mono); width: 30px; flex: none; }
  .oa-path { font-family: var(--font-mono); font-size: var(--font-size-xs); white-space: nowrap; }
  .oa-summary { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .oa-muted { font-size: var(--font-size-xs); color: var(--text-muted); }
  .oa-target { border-top: 1px solid var(--border-subtle); padding: 0 16px 8px; }
  .oa-where { display: grid; grid-template-columns: 1fr auto; gap: 8px 16px; align-items: end; }
</style>
