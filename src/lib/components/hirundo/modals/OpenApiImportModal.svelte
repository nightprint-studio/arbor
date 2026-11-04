<script lang="ts">
  /**
   * OpenApiImportModal — an OpenAPI 3 spec (JSON) becomes requests, and the user chooses which. The layout is
   * the shared `ImportShell` (the spec in the editor, the operations on the right); this is what is specific:
   * reading the spec as it is typed, grouping its operations by tag with a checkbox on each tag and operation,
   * and where they go. The spec's server becomes `{{baseUrl}}` (in the active environment when it has none yet,
   * else on the collection) and its security scheme the auth of the folder.
   */
  import { FileJson } from 'lucide-svelte';
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
  import { languageForPath } from '$lib/components/bennu/languages';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { environmentSave, openapiApply, openapiRead } from '$lib/ipc/hirundo';
  import { fuzzyMatch } from '$lib/utils/fuzzy';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import type { KeyValue, OpenApiPlan } from '$lib/types/hirundo';
  import { methodColor, methodShort } from '../request-model';
  import ImportShell from './ImportShell.svelte';
  import { jsonError, type TextError } from './import-diagnostics';

  let { onClose }: { onClose: () => void } = $props();

  const EXAMPLE = `{
  "openapi": "3.0.3",
  "info": { "title": "Pet store", "version": "1.0.0" },
  "servers": [{ "url": "https://petstore.example.com/v1" }],
  "paths": {
    "/pets": {
      "get": { "tags": ["pets"], "summary": "List pets", "operationId": "listPets", "responses": { "200": { "description": "ok" } } },
      "post": { "tags": ["pets"], "summary": "Add a pet", "operationId": "addPet", "responses": { "201": { "description": "created" } } }
    },
    "/pets/{id}": {
      "get": { "tags": ["pets"], "summary": "One pet", "operationId": "getPet", "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "integer" } }], "responses": { "200": { "description": "ok" } } }
    }
  }
}
`;

  const json = languageForPath('openapi.json');

  let text = $state('');
  let fromUrl = $state<string | null>(null);
  let reading = $state(false);
  let error = $state('');
  let errorAt = $state<TextError | null>(null);
  let plan = $state<OpenApiPlan | null>(null);

  let chosen = $state<Record<string, boolean>>({});
  let query = $state('');
  let name = $state('');
  let into = $state<'new' | 'folder'>('new');
  let parent = $state(hirundoStore.tree[0]?.path ?? '');
  let saving = $state(false);

  const matches = (id: string, tag: string, summary: string) => !query.trim() || fuzzyMatch(`${id} ${summary} ${tag}`, query) !== null;
  const groups = $derived(
    (plan?.tags ?? [])
      .map((tag) => ({ tag, ops: (plan?.operations ?? []).filter((o) => o.tag === tag && matches(o.id, o.tag, o.summary)) }))
      .filter((g) => g.ops.length),
  );
  const visible = $derived(groups.flatMap((g) => g.ops));
  const total = $derived(plan?.operations.length ?? 0);
  const count = $derived((plan?.operations ?? []).filter((o) => chosen[o.id]).length);
  const collections = $derived(hirundoStore.tree.map((c) => ({ value: c.path, label: c.name })));

  async function load(source: string, shownAs: string | null = null) {
    reading = true;
    error = '';
    errorAt = null;
    fromUrl = shownAs;
    try {
      const p = await openapiRead(source);
      plan = p;
      chosen = Object.fromEntries(p.operations.map((o) => [o.id, true]));
      name = p.title;
      query = '';
      if (shownAs) fromUrl = `${shownAs} — ${p.operations.length} operation${p.operations.length === 1 ? '' : 's'} found. Pick the ones to import on the right.`;
    } catch (e) {
      plan = null;
      error = String(e);
      errorAt = shownAs ? null : jsonError(source);
    } finally {
      reading = false;
    }
  }

  // The editor's text is read as it is typed or pasted (after a pause), so the sidebar follows without a button.
  $effect(() => {
    const t = text;
    if (!t.trim()) { plan = null; error = ''; errorAt = null; return; }
    const timer = setTimeout(() => void load(t), 350);
    return () => clearTimeout(timer);
  });

  function setMany(ids: string[], value: boolean) {
    for (const id of ids) chosen[id] = value;
  }

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
</script>

<ImportShell
  title="Import OpenAPI"
  icon={FileJson}
  subtitle={plan?.title}
  language={json}
  bind:text
  placeholder="Paste an OpenAPI 3 spec (JSON) here, or choose another source above."
  modes={['paste', 'file', 'url', 'example']}
  example={EXAMPLE}
  fileExtensions={['json']}
  fileTitle="Choose the OpenAPI file"
  urlPlaceholder="https://api.example.com/v3/api-docs"
  onFetch={(u) => void load(u, u)}
  {fromUrl}
  {reading}
  {error}
  {errorAt}
  sideTitle="Operations"
  sideCount={total || undefined}
  primaryLabel={`Import ${count} operation${count === 1 ? '' : 's'}`}
  primaryDisabled={!plan || count === 0 || (into === 'folder' && !parent)}
  primaryLoading={saving}
  onPrimary={() => void save()}
  {onClose}
>
  {#snippet side()}
    {#if !plan}
      <div class="oa-empty"><EmptyState message="Nothing to import yet" description="The operations of the spec appear here as soon as it reads." compact /></div>
    {:else}
      <div class="oa-bar"><FilterInput bind:value={query} placeholder="Filter — path, summary, tag…" ariaLabel="Filter operations" /></div>
      <div class="oa-tools">
        <Button variant="ghost" size="sm" onclick={() => setMany(visible.map((o) => o.id), true)}>All</Button>
        <Button variant="ghost" size="sm" onclick={() => setMany(visible.map((o) => o.id), false)}>None</Button>
        <span class="oa-count">{count} of {total} selected</span>
      </div>
      {#if plan.warnings.length}
        <div class="oa-pad">{#each plan.warnings as w (w)}<Alert variant="warning" compact text={w} />{/each}</div>
      {/if}
      <div class="oa-list">
        {#if groups.length === 0}<EmptyState message="No operation matches" compact />{/if}
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
                  <span class="oa-opname"><code class="oa-path">{o.path}</code>{#if o.summary}<span class="oa-muted">{o.summary}</span>{/if}</span>
                </li>
              {/each}
            </ul>
          </Collapsible>
        {/each}
      </div>
      <div class="oa-target">
        <FormSection label="Where">
          <FormField label="Name"><Input value={name} placeholder={plan.title} oninput={(v) => (name = v)} /></FormField>
          <RadioGroup value={into} options={[{ value: 'new', label: 'A new collection' }, { value: 'folder', label: 'A folder in…' }]} appearance="segment" size="sm" onchange={(v) => (into = v as typeof into)} />
          {#if into === 'folder'}<FormField label="Collection"><Select value={parent} options={collections} onchange={(v) => (parent = v)} /></FormField>{/if}
          {#if plan.base_url}<p class="oa-muted">{'{{baseUrl}}'} = <code>{plan.base_url}</code> · auth: {plan.auth.type === 'inherit' ? 'none in the spec' : plan.auth.type}</p>{/if}
        </FormSection>
      </div>
    {/if}
  {/snippet}
</ImportShell>

<style>
  .oa-empty { flex: 1; display: grid; place-items: center; }
  .oa-bar { padding: 8px 12px 4px; }
  .oa-tools { display: flex; align-items: center; gap: 4px; padding: 0 8px 4px; }
  .oa-count { margin-left: auto; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .oa-pad { padding: 0 12px 4px; display: flex; flex-direction: column; gap: 4px; }
  .oa-list { flex: 1; min-height: 0; overflow: auto; padding: 0 8px; }
  .oa-tag { display: inline-flex; margin-right: 6px; }
  .oa-ops { list-style: none; margin: 0; padding: 2px 0 8px 22px; display: flex; flex-direction: column; gap: 3px; }
  .oa-ops li { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .oa-method { font: 600 10px var(--font-mono); width: 28px; flex: none; }
  .oa-opname { display: flex; flex-direction: column; min-width: 0; }
  .oa-path { font-family: var(--font-mono); font-size: var(--font-size-xs); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .oa-muted { font-size: var(--font-size-xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .oa-target { border-top: 1px solid var(--border-subtle); padding: 0 12px 8px; }
</style>
