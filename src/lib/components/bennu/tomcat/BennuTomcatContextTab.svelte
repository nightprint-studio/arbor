<script lang="ts">
  /**
   * The application's `context.xml` — the data sources, environment entries and parameters Tomcat
   * serves it through JNDI.
   *
   * Three columns, and each has one job: **the resources** (selectable, each with its own trash),
   * **what you are editing** (one resource, several at once, or one of the helpers below), and **the
   * XML that will be written**, live. The helpers — read the project's own declarations, take a
   * Picus connection, paste an existing context — hang off one "Add" menu in the toolbar: they are
   * ways of *adding*, not permanent furniture next to the list.
   *
   * **Guided** generates the file from the resources. **Raw** hands over the text itself, for the
   * things the form does not model (a `Valve`, a `Manager`). The two are one document seen twice:
   * entering raw starts from what the form would write (unless the text was edited by hand and the
   * form has not changed since), and leaving it reads the text back into the form — so a
   * `<Environment>` typed in raw is a row of the list afterwards. What the form could not carry
   * across is named first and needs a yes.
   */
  import { untrack } from 'svelte';
  import { ClipboardPaste, Database, Link2, Plus, ScanSearch, Variable, FileCode2 } from 'lucide-svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { xmlLanguage } from '../languages';
  import { runConfigToDto, type RunConfig } from '$lib/stores/bennu/run-config.svelte';
  import {
    newResource,
    type ResourceKind,
    type TomcatResource,
    type TomcatSettings,
  } from '$lib/stores/bennu/tomcat-config';
  import { tomcatRenderContext, tomcatSetSecret } from '$lib/ipc/bennu/tomcat';
  import BennuTomcatResourceList from './BennuTomcatResourceList.svelte';
  import BennuTomcatResourceForm from './BennuTomcatResourceForm.svelte';
  import BennuTomcatBulkForm from './BennuTomcatBulkForm.svelte';
  import BennuTomcatDetectPanel from './BennuTomcatDetectPanel.svelte';
  import BennuTomcatPastePanel from './BennuTomcatPastePanel.svelte';
  import { jdbcFor, jndiNameFor, resourceFromPicus } from './tomcat-picus';
  import type { ConnectionRow } from '$lib/ipc/picus/db';
  import {
    mergeImport,
    NOTHING_FOUND,
    parseContextXml,
    unmodeledTags,
    type ContextImport,
  } from './tomcat-context-xml';
  import type { TomcatForm } from './tomcat-form.svelte';

  let {
    config,
    patchT,
    form,
    root,
  }: {
    config: RunConfig;
    patchT: (p: Partial<TomcatSettings>) => void;
    form: TomcatForm;
    root: string;
  } = $props();

  const t = $derived(config.tomcat);
  const resources = $derived(t.resources);

  // Picus's connections: the menu's picker and each data source's check both read them, so they are
  // asked for once, as this tab opens.
  $effect(() => {
    void form.refreshPicus();
  });

  // ── selection ───────────────────────────────────────────────────────────────
  let selected = $state<string[]>([]);
  let anchor = $state<string | null>(null);
  /** What the middle column shows instead of the editor. */
  let view = $state<'edit' | 'detect' | 'paste'>('edit');

  // Always something selected when there is something to select, and never a ghost id.
  $effect(() => {
    const alive = selected.filter((id) => resources.some((r) => r.id === id));
    if (alive.length !== selected.length) selected = alive;
    if (!alive.length && resources.length && view === 'edit') selected = [resources[0].id];
  });

  function select(id: string, mode: 'replace' | 'toggle' | 'range' | 'all') {
    if (mode === 'all') {
      selected = resources.map((r) => r.id);
      return;
    }
    view = 'edit';
    if (mode === 'replace') {
      selected = [id];
      anchor = id;
    } else if (mode === 'toggle') {
      selected = selected.includes(id) ? selected.filter((s) => s !== id) : [...selected, id];
      anchor = id;
    } else {
      const a = resources.findIndex((r) => r.id === (anchor ?? selected[0] ?? id));
      const b = resources.findIndex((r) => r.id === id);
      const [lo, hi] = a <= b ? [a, b] : [b, a];
      selected = resources.slice(Math.max(0, lo), hi + 1).map((r) => r.id);
    }
  }

  const picked = $derived(resources.filter((r) => selected.includes(r.id)));
  const warned = $derived(
    new Set(
      resources
        .filter((r) => r.kind === 'datasource' && (!r.name.trim() || !r.url.trim()))
        .map((r) => r.id),
    ),
  );

  // ── editing ─────────────────────────────────────────────────────────────────
  function setResources(next: TomcatResource[]) {
    patchT({ resources: next });
  }

  function edit(ids: string[], make: (r: TomcatResource) => Partial<TomcatResource>) {
    setResources(resources.map((r) => (ids.includes(r.id) ? { ...r, ...make(r) } : r)));
  }

  function add(kind: ResourceKind, over: Partial<TomcatResource> = {}): TomcatResource {
    const r = newResource(kind, over);
    setResources([...resources, r]);
    selected = [r.id];
    anchor = r.id;
    view = 'edit';
    return r;
  }

  /** Forget the stored password of each of `ids` that has one: an orphan secret is a thing nobody
   *  would ever think to look for. */
  function forgetSecrets(ids: string[]) {
    for (const id of ids) {
      if (form.hasSecret(id)) {
        void tomcatSetSecret(root, config.id, id, '').then(() => form.markSecret(id, false)).catch(() => {});
      }
    }
  }

  function remove(ids: string[]) {
    setResources(resources.filter((r) => !ids.includes(r.id)));
    forgetSecrets(ids);
    selected = selected.filter((s) => !ids.includes(s));
  }

  function addRequired(names: string[]) {
    const wanted = (form.scan?.requirements ?? []).filter((q) => names.includes(q.name));
    const made = wanted.map((q) =>
      q.kind === 'env' ? newResource('env', { name: q.name }) : newResource('datasource', { name: q.name }),
    );
    if (!made.length) return;
    setResources([...resources, ...made]);
    selected = [made[0].id];
    anchor = made[0].id;
    view = 'edit';
  }

  /** The JNDI name an imported connection takes: the first one the project asks for that the
   *  context does not define yet — which is what the person is most likely importing it for. */
  const nextRequiredName = $derived(
    (form.scan?.requirements ?? []).find(
      (q) => q.kind === 'datasource' && !resources.some((r) => r.name === q.name),
    )?.name ?? null,
  );

  function importFromPicus(row: ConnectionRow) {
    const made = resourceFromPicus(row, nextRequiredName ?? jndiNameFor(row));
    if (!made) return;
    setResources([...resources, made]);
    selected = [made.id];
    anchor = made.id;
    view = 'edit';
  }

  async function storePasswords(passwords: Map<string, string>) {
    await Promise.all(
      [...passwords].map(async ([id, pw]) => {
        await tomcatSetSecret(root, config.id, id, pw);
        form.markSecret(id, true);
      }),
    ).catch(() => {});
  }

  async function importResources(result: ContextImport) {
    // Same name = same resource: pasting over an existing one replaces it rather than doubling it.
    const byName = new Map(resources.map((r) => [r.name, r.id]));
    const next = [...resources];
    for (const r of result.resources) {
      const at = byName.get(r.name) ? next.findIndex((x) => x.name === r.name) : -1;
      if (at >= 0) {
        const old = next[at].id;
        const pw = result.passwords.get(r.id);
        result.passwords.delete(r.id);
        if (pw !== undefined) result.passwords.set(old, pw);
        next[at] = { ...r, id: old };
      } else next.push(r);
    }
    setResources(next);
    await storePasswords(result.passwords);
    selected = result.resources.map((r) => next.find((x) => x.name === r.name)?.id ?? r.id);
    view = 'edit';
  }

  // ── the Add menu ────────────────────────────────────────────────────────────
  const missingCount = $derived(
    (form.scan?.requirements ?? []).filter((q) => !resources.some((r) => r.name === q.name)).length,
  );
  /** Picus's connections as a picker: one row each, the ones with no JDBC driver here disabled. */
  const picusItems = $derived<DropdownItem[]>(
    form.picus.map((c) => {
      const ok = jdbcFor(c) !== null;
      return {
        kind: 'item',
        id: `picus-${c.id}`,
        label: c.name,
        subtitle: `${c.engine} · ${c.host}:${c.port} / ${c.database} · ${c.user}`,
        icon: Database,
        meta: ok ? undefined : 'no JDBC driver',
        disabled: !ok,
        onclick: () => importFromPicus(c),
      };
    }),
  );
  const addItems = $derived<DropdownItem[]>([
    { kind: 'item', id: 'ds', label: 'Data source', icon: Database, onclick: () => add('datasource') },
    { kind: 'item', id: 'env', label: 'Environment entry', icon: Variable, onclick: () => add('env') },
    { kind: 'item', id: 'param', label: 'Parameter', icon: Link2, onclick: () => add('param') },
    { kind: 'separator' },
    {
      kind: 'item',
      id: 'detect',
      label: 'Detect from the project…',
      icon: ScanSearch,
      meta: missingCount ? `${missingCount} missing` : undefined,
      metaColor: missingCount ? 'var(--warning)' : undefined,
      onclick: () => (view = 'detect'),
    },
    {
      kind: 'submenu',
      id: 'picus',
      label: 'From a Picus connection',
      icon: Database,
      disabled: picusItems.length === 0,
      items: picusItems,
    },
    { kind: 'item', id: 'paste', label: 'Paste XML…', icon: ClipboardPaste, onclick: () => (view = 'paste') },
  ]);

  // ── the live XML ────────────────────────────────────────────────────────────
  let xml = $state('');
  let renderError = $state('');
  let renderSeq = 0;

  /** What the form alone would write — whatever mode the configuration is in, since in raw mode
   *  the backend answers with the raw text itself. */
  function generate(dto = runConfigToDto(config)): Promise<string> {
    return tomcatRenderContext(root, { ...dto, tomcat: { ...dto.tomcat, context_mode: 'guided' } });
  }

  $effect(() => {
    // Read here, so a change to anything the file is built from re-renders it. Not while the text
    // is being typed in raw mode: entering raw renders once on its own.
    const dto = runConfigToDto(config);
    if (config.tomcat.contextMode !== 'guided') return;
    const mine = ++renderSeq;
    // Debounced: the backend is asked once the typing settles, not on every key.
    const timer = setTimeout(() => {
      generate(dto)
        .then((text) => {
          if (mine === renderSeq) {
            xml = text;
            renderError = '';
          }
        })
        .catch((e) => {
          if (mine === renderSeq) renderError = e instanceof Error ? e.message : String(e);
        });
    }, 250);
    return () => clearTimeout(timer);
  });

  // ── raw mode ────────────────────────────────────────────────────────────────
  let raw = $state(untrack(() => config.tomcat.contextRaw));
  /** The text `raw` was last seeded with: while it still equals it, nobody has edited by hand and
   *  the form is free to refresh it. */
  let seeded = '';
  let syncError = $state('');
  /** Tags the form would drop, while the person is asked whether that is fine. */
  let dropping = $state<string[] | null>(null);
  $effect(() => {
    const v = raw;
    untrack(() => {
      if (v !== config.tomcat.contextRaw) patchT({ contextRaw: v });
    });
  });

  async function toRaw() {
    try {
      const fresh = await generate();
      xml = fresh;
      if (!raw.trim() || raw === seeded) {
        raw = fresh;
        seeded = fresh;
      }
      syncError = '';
    } catch (e) {
      renderError = e instanceof Error ? e.message : String(e);
    }
    patchT({ contextMode: 'raw' });
  }

  /** Read the text back into the form. `force`: the person has already said the unmodelled parts may go. */
  async function toGuided(force = false) {
    if (!raw.trim()) {
      patchT({ contextMode: 'guided' });
      return;
    }
    const read = parseContextXml(raw);
    if (read.error && read.error !== NOTHING_FOUND) {
      syncError = read.error;
      return;
    }
    const lost = unmodeledTags(raw);
    if (lost.length && !force) {
      dropping = lost;
      return;
    }
    const merged = mergeImport(resources, read);
    forgetSecrets(resources.filter((r) => !merged.resources.some((m) => m.id === r.id)).map((r) => r.id));
    syncError = '';
    patchT({ resources: merged.resources, contextMode: 'guided' });
    await storePasswords(merged.passwords);
  }

  function setMode(mode: 'guided' | 'raw') {
    if (mode === 'raw') void toRaw();
    else void toGuided();
  }

  async function regenerate() {
    try {
      const fresh = await generate();
      xml = fresh;
      raw = fresh;
      seeded = fresh;
      syncError = '';
    } catch (e) {
      renderError = e instanceof Error ? e.message : String(e);
    }
  }

  const guided = $derived(t.contextMode === 'guided');
  const dropList = $derived((dropping ?? []).slice(0, 5).join(', ') + ((dropping?.length ?? 0) > 5 ? '…' : ''));
</script>

<div class="bar">
  <RadioGroup
    value={t.contextMode}
    size="sm"
    options={[{ value: 'guided', label: 'Guided' }, { value: 'raw', label: 'Raw XML' }]}
    onchange={(v) => setMode(v === 'raw' ? 'raw' : 'guided')}
  />
  <span class="hint">
    {guided ? 'The file is generated from the resources below.' : 'The text is written exactly as typed.'}
  </span>
  <span class="sp"></span>
  {#if guided}
    <Dropdown items={addItems} position="fixed" direction="down" width="280px">
      {#snippet trigger({ toggle, open })}
        <Button variant="secondary" size="sm" onclick={toggle} ariaHaspopup="menu" ariaExpanded={open}>
          {#snippet iconStart()}<Plus size={13} />{/snippet}
          Add
        </Button>
      {/snippet}
    </Dropdown>
  {:else}
    <Button variant="secondary" size="sm" onclick={regenerate} tooltip={{ content: 'Replace the text with what the form would generate' }}>
      {#snippet iconStart()}<FileCode2 size={13} />{/snippet}
      Regenerate from the form
    </Button>
  {/if}
</div>

{#if guided}
  <div class="cols">
    <section class="col list" aria-label="Resources">
      <BennuTomcatResourceList {resources} {selected} {warned} onselect={select} ondelete={remove} />
    </section>

    <section class="col edit" aria-label="Editor">
      {#if view === 'detect'}
        <BennuTomcatDetectPanel
          requirements={form.scan?.requirements ?? []}
          defined={new Set(resources.map((r) => r.name))}
          onadd={addRequired}
          onclose={() => (view = 'edit')}
        />
      {:else if view === 'paste'}
        <BennuTomcatPastePanel onimport={importResources} onclose={() => (view = 'edit')} />
      {:else if picked.length > 1}
        <BennuTomcatBulkForm resources={picked} configId={config.id} {root} {form} onapply={edit} ondelete={remove} />
      {:else if picked.length === 1}
        {#key picked[0].id}
          <BennuTomcatResourceForm
            resource={picked[0]}
            configId={config.id}
            {root}
            {form}
            onchange={(p) => edit([picked[0].id], () => p)}
          />
        {/key}
      {:else}
        <EmptyState message="Add a resource, or detect the ones the project asks for." compact />
      {/if}
    </section>

    <section class="col xml" aria-label="Generated context.xml">
      <header>context.xml · generated</header>
      {#if renderError}<p class="err">{renderError}</p>{/if}
      <div class="code">
        <CodeEditor value={xml} language={xmlLanguage} readOnly />
      </div>
    </section>
  </div>
{:else}
  <div class="raw">
    {#if syncError}<Alert variant="error" compact text={syncError} />{/if}
    <div class="code tall">
      <CodeEditor value={raw} language={xmlLanguage} oninput={(v) => (raw = v)} />
    </div>
    <p class="hint">
      Going back to Guided reads the text into the form. Tomcat resolves <code>&#36;&#123;property&#125;</code> placeholders from system
      properties — a password kept out of the file can be referenced that way and given in the JVM options.
    </p>
  </div>
{/if}

{#if dropping}
  <ConfirmModal
    title="Parts the form cannot show"
    message={`The text has ${dropList}. Guided writes only data sources, environment entries and parameters, so these would no longer be in the file.`}
    variant="warning"
    confirmLabel="Switch and drop them"
    onConfirm={() => { dropping = null; void toGuided(true); }}
    onCancel={() => (dropping = null)}
  />
{/if}

<style>
  .bar { display: flex; align-items: center; gap: 12px; margin-bottom: 12px; flex: none; }
  .hint { color: var(--text-muted); font-size: var(--font-size-xs); }
  .sp { flex: 1; }
  /* The three columns take the height the pane gives them — one height, so the list is a sidebar and
     not as tall as its rows — and each scrolls on its own. */
  .cols {
    display: grid; grid-template-columns: 230px minmax(0, 1fr) 320px; gap: 14px; align-items: stretch;
    flex: 1; min-height: 240px;
  }
  .col { min-width: 0; min-height: 0; }
  .list { border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-base); padding: 4px; overflow: auto; }
  .edit { overflow: auto; }
  .xml { border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-base); display: flex; flex-direction: column; overflow: hidden; }
  .xml header { padding: 6px 10px; border-bottom: 1px solid var(--border-subtle, var(--border)); color: var(--text-muted); font-size: var(--font-size-xs); }
  .code { display: flex; flex: 1; min-height: 0; min-width: 0; }
  .code > :global(*) { flex: 1; min-width: 0; min-height: 0; }
  .code.tall {
    border: 1px solid var(--border); border-radius: var(--radius-md); overflow: hidden;
  }
  .err { margin: 8px 10px 0; color: var(--danger); font-size: var(--font-size-xs); }
  .raw { display: flex; flex-direction: column; gap: 8px; flex: 1; min-height: 0; }
  code { font-family: var(--font-code); }
</style>
