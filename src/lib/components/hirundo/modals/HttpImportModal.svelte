<script lang="ts">
  /**
   * HttpImportModal — a `.http` / `.rest` file (IntelliJ's HTTP Client format) becomes requests, in the shared
   * `ImportShell`: the text in the editor, what it holds on the right. JavaScript response handlers are read and
   * reported, never run. Given a *file*, `< ./body.json` bodies are read from beside it, and the environments
   * of the `http-client.env.json` next to it (and its private twin) can come along.
   */
  import { FileText } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Checkbox from '$lib/components/shared/ui/Checkbox.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { languageForPath } from '$lib/components/bennu/languages';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { httpEnvImport, httpImport, requestSave } from '$lib/ipc/hirundo';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import type { HttpEntry } from '$lib/types/hirundo';
  import { methodColor } from '../request-model';
  import ImportShell from './ImportShell.svelte';
  import { lineError, positionFromMessage, type TextError } from './import-diagnostics';

  let { onClose }: { onClose: () => void } = $props();

  const EXAMPLE = `@host = https://httpbin.org

### Get something
GET {{host}}/get
Accept: application/json

### Login
POST {{host}}/post
Content-Type: application/json

{ "user": "bob" }
`;
  const language = languageForPath('requests.http');

  let text = $state('');
  let filePath = $state('');
  /** The text as it was when read from `filePath`: while unedited, the backend reads the file itself so `< ./body.json` resolves beside it. */
  let fileText = $state('');
  let reading = $state(false);
  let error = $state('');
  let errorAt = $state<TextError | null>(null);
  let entries = $state<HttpEntry[]>([]);
  let picked = $state<boolean[]>([]);
  let collection = $state(hirundoStore.active?.parent ?? hirundoStore.tree[0]?.path ?? '');
  let withEnvs = $state(true);
  let saving = $state(false);

  const collections = $derived(hirundoStore.tree.map((c) => ({ value: c.path, label: c.name })));
  const count = $derived(picked.filter(Boolean).length);
  /** The folder of the file the text was read from, so its environments can come along. */
  const folder = $derived(filePath.replace(/[\\/][^\\/]*$/, ''));

  async function read(source: string, shown: string) {
    reading = true;
    error = '';
    errorAt = null;
    try {
      entries = (await httpImport(source)).entries;
      picked = entries.map(() => true);
    } catch (e) {
      entries = [];
      error = String(e);
      const at = positionFromMessage(shown, error);
      errorAt = at === null ? lineError(shown, error) : { message: error, from: at, to: Math.min(shown.length, at + 1) };
    } finally {
      reading = false;
    }
  }

  // Read as it is typed or pasted, after a pause: the list on the right follows the text.
  $effect(() => {
    const t = text;
    if (!t.trim()) { entries = []; picked = []; error = ''; errorAt = null; return; }
    const timer = setTimeout(() => void read(filePath && t === fileText ? filePath : t, t), 350);
    return () => clearTimeout(timer);
  });

  async function save() {
    const project = hirundoStore.project;
    if (!project || !entries.length || !collection || saving) return;
    saving = true;
    error = '';
    try {
      let first: string | null = null;
      for (const [i, entry] of entries.entries()) {
        if (!picked[i]) continue;
        const path = await requestSave(project, entry.request, { parent: collection });
        first ??= path;
      }
      if (withEnvs && folder) {
        const added = await httpEnvImport(project, folder).catch(() => 0);
        if (added) { await hirundoStore.refreshEnvs(); toastStore.show(`${added} environment${added === 1 ? '' : 's'} imported.`, 'success'); }
      }
      await hirundoStore.refreshTree();
      if (first) await hirundoStore.openRequest(first, false);
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<ImportShell
  title="Import a .http file"
  icon={FileText}
  {language}
  bind:text
  placeholder={'### Login\nPOST {{host}}/api/login\nContent-Type: application/json\n\n{ "user": "bob" }'}
  modes={['paste', 'file', 'example']}
  example={EXAMPLE}
  fileExtensions={['http', 'rest']}
  fileTitle="Choose the .http file"
  onFile={(p) => { filePath = p; fileText = text; }}
  {reading}
  {error}
  {errorAt}
  sideTitle="Requests found"
  sideCount={entries.length || undefined}
  primaryLabel={`Add ${count} request${count === 1 ? '' : 's'}`}
  primaryDisabled={count === 0 || !collection}
  primaryLoading={saving}
  onPrimary={() => void save()}
  {onClose}
>
  {#snippet side()}
    {#if entries.length === 0}
      <div class="hi-empty"><EmptyState message={text.trim() ? 'No request found' : 'Nothing to import yet'} description="The requests of the file appear here as soon as it reads." compact /></div>
    {:else}
      <div class="hi-list">
        {#each entries as entry, i (i)}
          <div class="hi-item">
            <Checkbox checked={picked[i]} onchange={(v) => (picked[i] = v)} ariaLabel={entry.title} />
            <span class="hi-method" style:color={methodColor(entry.request.method)}>{entry.request.method}</span>
            <span class="hi-text"><b class="hi-name">{entry.title}</b><code class="hi-url">{entry.request.url}</code></span>
            {#each entry.warnings as w (w)}<Alert variant="warning" compact text={w} />{/each}
          </div>
        {/each}
      </div>
      <div class="hi-target">
        <FormSection label="Where">
          {#if folder}<Checkbox checked={withEnvs} label="Also import the environments of http-client.env.json beside the file" onchange={(v) => (withEnvs = v)} />{/if}
          <FormField label="Save into"><Select value={collection} options={collections} onchange={(v) => (collection = v)} /></FormField>
        </FormSection>
      </div>
    {/if}
  {/snippet}
</ImportShell>

<style>
  .hi-empty { flex: 1; display: grid; place-items: center; }
  .hi-list { flex: 1; min-height: 0; overflow: auto; }
  .hi-item { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 6px 12px; border-bottom: 1px solid var(--border-subtle); }
  .hi-method { font: 600 10px var(--font-mono); width: 44px; flex: none; }
  .hi-text { display: flex; flex-direction: column; min-width: 0; flex: 1; }
  .hi-name { font-size: var(--font-size-sm); }
  .hi-url { font-family: var(--font-mono); font-size: var(--font-size-xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hi-item > :global(.alert) { flex-basis: 100%; }
  .hi-target { border-top: 1px solid var(--border-subtle); padding: 0 12px 8px; }
</style>
