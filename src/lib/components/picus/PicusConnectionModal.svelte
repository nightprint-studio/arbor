<script lang="ts">
  /**
   * Connection editor — everything about a database session except its secret.
   *
   * Picus deliberately holds **no password**. Credentials live in Arbor's
   * keychain; this form collects only what identifies the session and hands the
   * secret off to the vault. That is why there is no password field here and why
   * one should never be added: a project file, a log or a config that can leak a
   * database password is a bug in the design, not a missing feature.
   *
   * The form is split the way the connection actually is: **identity** (what you
   * call it), **server** (where it is), **session** (how it behaves once open).
   * Host, port and service/database are separate fields rather than one string,
   * because the two engines spell that string differently
   * (`host:1521/SERVICE` vs `host:5432/database`) and a single box invites the
   * wrong one.
   *
   * Keyboard-first: the first field is focused on open, Tab walks the form in
   * reading order, Ctrl+Enter saves, Esc cancels.
   */
  import { untrack } from 'svelte';
  import { Database, KeyRound, Plug, ShieldCheck, CircleAlert, CheckCircle2, FolderOpen, X } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import ColorPalettePicker from '$lib/components/shared/ui/ColorPalettePicker.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import PicusDialectChip from './PicusDialectChip.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import {
    connectionsStore, connectionColorSlot, CONNECTION_COLOR_SLOTS,
  } from '$lib/stores/picus/connections.svelte';
  import {
    type ConnectionField,
    type ConnectionSpec,
    type DbProviderDescriptor,
    listProviders,
    testConnection,
  } from '$lib/ipc/picus/db';
  import type { Dialect } from '$lib/types/picus';

  interface Props {
    /** Connection to edit; `null` creates a new one. */
    connectionId: string | null;
    onClose: () => void;
  }

  let { connectionId, onClose }: Props = $props();

  const existing = $derived(connectionId ? connectionsStore.specById(connectionId) : null);

  // ── The per-engine descriptors ──────────────────────────────────────────────
  //
  // The form's labels, placeholders, defaults and required-ness come from the
  // backend rather than from `if (dialect === 'oracle')` scattered here. The
  // well-known field ids keep their place in the curated layout; anything else an
  // engine declares is rendered generically under Advanced, so a third engine that
  // wants a TNS alias gets one without this component changing.
  let providers = $state<DbProviderDescriptor[]>([]);
  $effect(() => {
    void listProviders()
      .then((list) => { providers = list; })
      .catch(() => { providers = []; });
  });

  let name = $state('');
  let alias = $state('');
  let dialect = $state<Dialect>('postgres');
  let host = $state('');
  let port = $state(5432);
  let service = $state('');
  let schema = $state('');
  let username = $state('');
  /** The first offered slot, never a hard-coded number: the default used to be `2`,
   *  which is the green this product reserves for "the session is open". */
  let colorIdx = $state(CONNECTION_COLOR_SLOTS[0]);
  let readOnly = $state(false);
  let tls = $state(false);
  /** The free-form driver parameters. The script root is not among them — it is a
   *  field of the spec in its own right, edited below with a folder picker. */
  let extraParams = $state('');

  /**
   * The folder of SQL scripts this database is installed from.
   *
   * A repository belongs to a connection: Picus is database-oriented, so opening
   * this connection is what brings these scripts into view. Optional by design —
   * a connection used only to run queries never needs one.
   */
  let scriptRoot = $state('');
  let rootPickerOpen = $state(false);

  /**
   * Fill the form from the connection being edited.
   *
   * The fields cannot be initialised from `existing` directly. It is derived from
   * a store, and a `$state(existing?.host ?? '')` initialiser captures only what
   * was there at **mount** — so a dialog opened while the connections were still
   * loading came up blank and stayed blank, describing a connection it had, and
   * then quietly saved those blanks over it. (Svelte says this out loud:
   * `state_referenced_locally`, eleven times in this file alone.)
   *
   * Seeded once per spec, and never again while it is the same one, so a
   * re-render cannot overwrite what is being typed. `seeded` is a plain `let` on
   * purpose — making it reactive would have writing it re-enter the effect that
   * writes it — and the field writes are untracked for the same reason.
   */
  let seeded: string | null = null;
  $effect(() => {
    const spec = existing;
    const key = spec?.id ?? '';
    if (key === seeded) return;
    seeded = key;
    untrack(() => {
      name = spec?.name ?? '';
      alias = spec?.alias ?? '';
      dialect = spec?.engine ?? 'postgres';
      host = spec?.host ?? '';
      port = spec?.port ?? 5432;
      service = spec?.database ?? '';
      schema = spec?.schema ?? '';
      username = spec?.user ?? '';
      // Through the mapper: an existing connection may carry a slot this product no
      // longer offers, and the form must open on a swatch that is actually in the row.
      colorIdx = connectionColorSlot(spec?.colorIdx);
      readOnly = spec?.readOnly ?? false;
      tls = spec?.tls ?? false;
      scriptRoot = spec?.scriptRoot ?? '';
      extraParams = Object.entries(spec?.params ?? {})
        .map(([k, v]) => `${k}=${v}`)
        .join('\n');
    });
  });

  /**
   * The password.
   *
   * `null` means "not touched" — the stored secret stays as it is. Typing anything
   * (including clearing the box to empty) makes it a string, which is then written.
   * Collapsing the two would delete a saved password on an unrelated edit.
   */
  let password = $state<string | null>(null);
  const hasStoredSecret = $derived(existing?.hasSecret ?? false);

  let testing = $state(false);
  let saving = $state(false);
  let testResult = $state<{ ok: boolean; message: string } | null>(null);

  let firstField = $state<HTMLInputElement | undefined>();
  $effect(() => { firstField?.focus(); });

  const descriptor = $derived(providers.find((p) => p.kind === dialect) ?? null);
  const dialectOptions = $derived(
    providers.length
      ? providers.map((p) => ({ value: p.kind, label: p.label }))
      : [{ value: 'postgres', label: 'PostgreSQL' }],
  );

  /** One declared field of the current engine, by id. */
  function field(id: string): ConnectionField | null {
    return descriptor?.fields.find((f) => f.id === id) ?? null;
  }

  /** Ids the curated layout already renders — everything else falls to Advanced. */
  const WELL_KNOWN = ['host', 'port', 'database', 'user', 'password', 'schema', 'tls'];
  const extraFields = $derived(
    (descriptor?.fields ?? []).filter((f) => !WELL_KNOWN.includes(f.id)),
  );

  /**
   * Whether this engine can be connected to at all.
   *
   * Oracle is fully supported for **scripts** — parsing, analysis, generation and
   * rewriting all work — and has no driver. Saying that plainly is better than
   * hiding the engine, which would make the product look like it doesn't know
   * about Oracle at all.
   */
  const connectable = $derived(descriptor?.capabilities.connect ?? true);

  const serviceLabel = $derived(field('database')?.label ?? 'Database');
  const servicePlaceholder = $derived(field('database')?.placeholder ?? 'appdb');
  const schemaPlaceholder = $derived(field('schema')?.default ?? 'public');
  const defaultPort = $derived(descriptor?.defaultPort ?? 5432);
  const showSchema = $derived(descriptor?.capabilities.schemas ?? true);

  function switchDialect(next: Dialect) {
    // Only move the port if it was still the previous engine's default — never
    // overwrite a port the user typed on purpose.
    const previousDefault = providers.find((p) => p.kind === dialect)?.defaultPort;
    const nextDefault = providers.find((p) => p.kind === next)?.defaultPort;
    if (nextDefault && port === previousDefault) port = nextDefault;
    dialect = next;
    testResult = null;
  }

  const composedHost = $derived(
    `${host.trim()}${port ? `:${port}` : ''}${service.trim() ? `/${service.trim()}` : ''}`,
  );

  const valid = $derived(name.trim() !== '' && host.trim() !== '' && service.trim() !== '');

  // ── Pages ───────────────────────────────────────────────────────────────────
  //
  // One scroll of five headed sections became four pages. The split is by *when*
  // you touch them, not by what they are made of: Identity is filled in once and
  // then never again, Server is what you come back to when something moved,
  // Scripts is a decision about a folder, Advanced is the engine's own vocabulary
  // and is usually empty.
  //
  // Identity opens first because Name is required and because it is what the
  // dialog is called after. The other two required fields live on Server, so both
  // pages carry a dot while anything on them is missing — a Save button that is
  // disabled for a reason on a page you cannot see is the failure a tabbed form
  // invites, and the dot is the whole defence against it.
  type Page = 'identity' | 'server' | 'scripts' | 'session';
  let page = $state<Page>('identity');

  const identityIncomplete = $derived(name.trim() === '');
  const serverIncomplete = $derived(host.trim() === '' || service.trim() === '');

  const pages = $derived<TabItem[]>([
    { id: 'identity', label: 'Identity', badge: identityIncomplete ? '!' : undefined },
    { id: 'server', label: 'Server', badge: serverIncomplete ? '!' : undefined },
    { id: 'scripts', label: 'Scripts' },
    { id: 'session', label: 'Session' },
  ]);

  /** Set one key in the `key=value` block, preserving the rest and the order. */
  function setParam(key: string, value: string): string {
    const lines = extraParams.split('\n').filter((l) => l.trim() !== '');
    const i = lines.findIndex((l) => l.split('=')[0]?.trim() === key);
    const line = `${key}=${value}`;
    if (i >= 0) lines[i] = line;
    else lines.push(line);
    return lines.join('\n');
  }

  /** `key=value` lines → the spec's `params` map. Blank and malformed lines drop. */
  function parseParams(): Record<string, string> {
    const out: Record<string, string> = {};
    for (const line of extraParams.split('\n')) {
      const [k, ...rest] = line.split('=');
      if (!k?.trim() || !rest.length) continue;
      out[k.trim()] = rest.join('=').trim();
    }
    return out;
  }

  /** The attached repository, or `undefined` when there is none.
   *
   *  `undefined` rather than `''`: the backend's field is an `Option`, and
   *  detaching a repository means absent, not "attached to nowhere". */
  function scriptRootOrNone(): string | undefined {
    return scriptRoot.trim() || undefined;
  }

  /**
   * The id a **new** connection will get, decided once when the modal opens.
   *
   * It used to be computed inside `toSpec()`, which is called by Save *and* by
   * Test — and Test saves the connection first, because the backend resolves the
   * password from the keychain by id rather than receiving it over the wire. So
   * testing and then saving minted two ids and left two connections behind. An id
   * is identity: it has to be decided once, not derived from the clock every time
   * somebody asks what the form contains.
   *
   * The random suffix guards the other way two could collide: creating two
   * connections inside the same millisecond.
   */
  const draftId = `conn-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;

  function toSpec(): ConnectionSpec {
    return {
      id: existing?.id ?? draftId,
      name: name.trim(),
      alias: alias.trim() || 'unnamed',
      engine: dialect,
      host: host.trim(),
      port,
      database: service.trim(),
      user: username.trim(),
      schema: schema.trim() || (showSchema ? String(schemaPlaceholder ?? '') : ''),
      colorIdx,
      readOnly,
      tls,
      scriptRoot: scriptRootOrNone(),
      params: parseParams(),
    };
  }

  /**
   * Open, report, close.
   *
   * A real probe: it opens a session with these exact settings and shuts it again,
   * deliberately without touching the session pool — testing must not silently
   * leave a connection open, nor disturb one already in use.
   *
   * The password has to be saved first, because the backend resolves it from the
   * keychain by connection id rather than receiving it over the wire.
   */
  async function test() {
    if (!valid || !connectable) return;
    testing = true;
    testResult = null;
    try {
      const spec = toSpec();
      if (password !== null) await connectionsStore.save(spec, password);
      const status = await testConnection(spec);
      testResult = {
        ok: true,
        message: status.serverVersion
          ? `Connected to ${status.serverVersion} at ${composedHost}.`
          : `Connected to ${composedHost}.`,
      };
    } catch (e) {
      testResult = { ok: false, message: String(e) };
    } finally {
      testing = false;
    }
  }

  async function save() {
    if (!valid || saving) return;
    saving = true;
    try {
      const spec = toSpec();
      await connectionsStore.save(spec, password ?? undefined);
      toastStore.show(`${spec.name} saved.`, 'success');
      onClose();
    } catch (e) {
      testResult = { ok: false, message: String(e) };
    } finally {
      saving = false;
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      void save();
    }
  }
</script>

<Modal {onClose} width="720px" height="640px" padBody={false} ariaLabel="Connection">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Database size={14} />
      <span class="modal-title">{existing ? `Edit ${existing.name}` : 'New connection'}</span>
      <PicusDialectChip engine={dialect} />
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="cm" onkeydown={onKeyDown} role="form">
    <div class="cm-tabs">
      <Tabs
        items={pages}
        value={page}
        variant="underline"
        size="sm"
        ariaLabel="Connection settings"
        onSelect={(id) => (page = id as Page)}
      />
    </div>

    {#if page === 'identity'}
    <section class="cm-section">
      <div class="cm-grid">
        <FormField label="Name" required>
          <Input bind:element={firstField} value={name} placeholder="ORCL-DEV" oninput={(v) => (name = v)} />
        </FormField>
        <FormField label="Alias" hint="How you refer to it out loud: development, staging, production.">
          <Input value={alias} placeholder="development" oninput={(v) => (alias = v)} />
        </FormField>
      </div>

      <FormField
        label="Colour"
        hint="Shown on the sidebar row, on every tab bound to this session, and in the status bar — the way two databases stay distinguishable. Green is not offered: it is what a connected session's dot is."
      >
        <!-- The picker is positional over the list it is given, so the two mappings
             are explicit: what it shows are the offered slots, and what it reports is
             a position in that list, turned back into a palette slot here. -->
        <ColorPalettePicker
          colors={CONNECTION_COLOR_SLOTS.map((slot) => `var(--ws-color-${slot})`)}
          value={CONNECTION_COLOR_SLOTS.indexOf(connectionColorSlot(colorIdx))}
          onChange={(i) => (colorIdx = CONNECTION_COLOR_SLOTS[i] ?? CONNECTION_COLOR_SLOTS[0])}
          ariaLabel="Connection colour"
        />
      </FormField>
    </section>
    {/if}

    {#if page === 'server'}
    <section class="cm-section">
      <FormField label="Engine">
        <Select value={dialect} options={dialectOptions} onchange={(v) => switchDialect(v as Dialect)} />
      </FormField>

      <div class="cm-address">
        <FormField label="Host" required>
          <Input value={host} placeholder="ora19-dev" oninput={(v) => { host = v; testResult = null; }} />
        </FormField>
        <FormField label="Port">
          <NumberStepper value={port} min={1} max={65535} onchange={(v) => (port = v)} />
        </FormField>
        <FormField label={serviceLabel} required>
          <Input value={service} placeholder={servicePlaceholder} oninput={(v) => { service = v; testResult = null; }} />
        </FormField>
      </div>

      <p class="cm-preview">
        <span>Address</span>
        <code>{composedHost || '—'}</code>
        {#if port !== defaultPort}
          <span class="cm-nondefault">non-default port</span>
        {/if}
      </p>

      <div class="cm-grid">
        {#if showSchema}
          <FormField label={field('schema')?.label ?? 'Schema'} hint={field('schema')?.help ?? 'Where unqualified names resolve.'}>
            <Input value={schema} placeholder={String(schemaPlaceholder ?? '')} oninput={(v) => (schema = v)} />
          </FormField>
        {/if}
        <FormField label={field('user')?.label ?? 'Username'} hint="Stored with the connection; the password is not.">
          <Input value={username} placeholder={field('user')?.placeholder ?? 'appuser'} oninput={(v) => (username = v)} />
        </FormField>
      </div>

      {#if !connectable}
        <Alert variant="warning" compact>
          <span class="cm-secret">
            <CircleAlert size={12} />
            <span>
              Picus has no driver for {descriptor?.label ?? 'this engine'} yet, so it cannot open a
              session to one. Its <b>scripts</b> are fully supported — read, analysed, generated
              into and rewritten — which is what a folder written for that engine actually needs.
            </span>
          </span>
        </Alert>
      {/if}
    </section>
    {/if}

    {#if page === 'scripts'}
    <section class="cm-section">
      <FormField
        label="Script repository"
        hint="The folder this database is installed from. Picus reads its directory tree as it is and works out which parts are written for which engine. Opening this connection brings its scripts, its inventory and its consistency report into the window. Optional: a connection used only for queries needs none."
      >
        <div class="cm-root">
          <Input
            value={scriptRoot}
            placeholder="C:\projects\prod-core\database"
            ariaLabel="Script repository folder"
            oninput={(v) => (scriptRoot = v)}
          />
          <Button
            variant="secondary"
            size="sm"
            ariaLabel="Choose the script folder"
            onclick={() => (rootPickerOpen = true)}
          >
            {#snippet iconStart()}<FolderOpen size={13} />{/snippet}
            Choose…
          </Button>
          {#if scriptRoot}
            <Button
              variant="ghost"
              size="sm"
              ariaLabel="Detach the script repository"
              tooltip={'Stop showing scripts for this connection. Nothing on disk is touched.'}
              onclick={() => (scriptRoot = '')}
            >
              {#snippet iconStart()}<X size={13} />{/snippet}
              Detach
            </Button>
          {/if}
        </div>
      </FormField>
    </section>
    {/if}

    {#if page === 'session'}
    <section class="cm-section">
      <FormField label="Read-only">
        <Toggle
          checked={readOnly}
          size="sm"
          label="Refuse every write on this connection"
          description="Enforced by the backend, not just hidden in the interface. Use it for production."
          onchange={(v) => (readOnly = v)}
        />
      </FormField>

      {#if field('tls')}
        <FormField label={field('tls')?.label ?? 'Require TLS'}>
          <Toggle
            checked={tls}
            size="sm"
            label="Encrypt the connection"
            description={field('tls')?.help ?? 'Managed cloud databases refuse plaintext connections.'}
            onchange={(v) => { tls = v; testResult = null; }}
          />
        </FormField>
      {/if}

      <Collapsible chevron>
        {#snippet header()}
          <span class="cm-advanced-head">Advanced</span>
        {/snippet}
        <div class="cm-advanced">
          <!-- Fields this engine declares that the curated layout above has no
               place for. Rendering them generically is what lets a third engine
               ask for something new without this component changing. -->
          {#each extraFields as f (f.id)}
            <FormField label={f.label} hint={f.help ?? undefined} required={f.required}>
              {#if f.type === 'toggle'}
                <Toggle
                  checked={parseParams()[f.id] === 'true'}
                  size="sm"
                  label={f.label}
                  onchange={(v) => { extraParams = setParam(f.id, String(v)); }}
                />
              {:else if f.type === 'select'}
                <Select
                  value={parseParams()[f.id] ?? f.default ?? ''}
                  options={(f.options ?? []).map((o) => ({ value: o.value, label: o.label }))}
                  onchange={(v) => { extraParams = setParam(f.id, v); }}
                />
              {:else}
                <Input
                  value={parseParams()[f.id] ?? f.default ?? ''}
                  placeholder={f.placeholder ?? ''}
                  oninput={(v) => { extraParams = setParam(f.id, v); }}
                />
              {/if}
            </FormField>
          {/each}

          <FormField
            label="Extra parameters"
            hint="Passed to the driver as-is. One `key=value` per line."
          >
            <Input value={extraParams} placeholder="sslmode=require" oninput={(v) => (extraParams = v)} />
          </FormField>
        </div>
      </Collapsible>
    </section>
    {/if}

    <!-- Credentials belong to **Server**: the password is part of reaching the
         database, not a category of its own, and separating them meant filling in
         a host on one page and the password that goes with it on another. It is a
         second block only because it is last in this file — the page it renders on
         is the one above. -->
    {#if page === 'server'}
    <section class="cm-section">
      <Alert variant="info" compact>
        <span class="cm-secret">
          <KeyRound size={12} />
          <span>
            The password is not stored here. Picus asks Arbor's keychain for it at connection
            time, so it never lands in a project file, a configuration or a log.
          </span>
        </span>
      </Alert>
      <FormField
        label={field('password')?.label ?? 'Password'}
        hint={hasStoredSecret && password === null
          ? 'A password is saved for this connection. Leave this empty to keep it; type to replace it.'
          : (field('password')?.help ?? "Goes straight to Arbor's keychain, never into the project.")}
      >
        <Input
          type="password"
          value={password ?? ''}
          placeholder={hasStoredSecret && password === null ? '••••••••' : ''}
          oninput={(v) => { password = v; testResult = null; }}
        />
      </FormField>

      {#if hasStoredSecret && password === ''}
        <Alert variant="warning" compact>
          <span class="cm-secret">
            <CircleAlert size={12} />
            <span>Saving now will <b>delete</b> the stored password for this connection.</span>
          </span>
        </Alert>
      {/if}

      {#if testResult}
        <div class="cm-test" class:cm-test-bad={!testResult.ok}>
          {#if testResult.ok}<CheckCircle2 size={13} />{:else}<CircleAlert size={13} />{/if}
          <span>{testResult.message}</span>
        </div>
      {/if}
    </section>
    {/if}
  </div>

  {#snippet footer()}
    <span class="cm-keychain">
      <ShieldCheck size={13} />
      <span>Password kept in Arbor's keychain</span>
    </span>
    <Button
      variant="secondary"
      size="sm"
      disabled={!valid || testing || !connectable}
      tooltip={!connectable ? { content: 'This engine has no driver — its scripts are still fully supported' } : undefined}
      onclick={() => void test()}
    >
      {#snippet iconStart()}
        {#if testing}<Spinner size={12} />{:else}<Plug size={13} />{/if}
      {/snippet}
      Test connection
    </Button>
    <span class="cm-spacer"></span>
    <Button variant="ghost" size="sm" onclick={onClose}>Cancel</Button>
    <Button
      variant="primary"
      size="sm"
      disabled={!valid || saving}
      tooltip={{ content: 'Save the connection', shortcut: 'Ctrl+Enter' }}
      onclick={() => void save()}
    >
      Save
    </Button>
  {/snippet}
</Modal>

{#if rootPickerOpen}
  <!-- Arbor's own folder picker, never the native dialog and never an
       <input type="file">. Stacked over the editor rather than replacing it: the
       rest of the form is half-filled and must survive the choice. -->
  <FileExplorerModal
    mode="folder"
    title="Choose the folder of SQL scripts"
    initialPath={scriptRoot || undefined}
    onConfirm={(path) => { scriptRoot = path; rootPickerOpen = false; }}
    onCancel={() => (rootPickerOpen = false)}
    onClose={() => (rootPickerOpen = false)}
  />
{/if}

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }

  .cm-keychain {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--font-size-xs);
    color: var(--text-tertiary);
  }

  .cm {
    display: flex;
    flex-direction: column;
    gap: 18px;
    height: 100%;
    overflow-y: auto;
    padding: 16px;
  }

  /* The page strip stays put while the page under it scrolls: on a short page it
     would otherwise sit in the middle of the dialog. */
  .cm-tabs {
    position: sticky;
    top: -16px;
    z-index: 1;
    margin: -16px -16px 0;
    padding: 0 16px;
    background: var(--bg-modal);
    border-bottom: 1px solid var(--border-subtle);
  }

  /* No `h2`: the page is named by the tab above it, and repeating that name as a
     heading inside the page is the same word twice on one screen. */
  .cm-section { display: flex; flex-direction: column; gap: 12px; }

  .cm-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 12px;
  }
  /* Host · port · service: one address, three fields, because the engines spell
     the last part differently. */
  .cm-address {
    display: grid;
    grid-template-columns: minmax(0, 2fr) 110px minmax(0, 1.6fr);
    gap: 12px;
  }

  .cm-preview {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
  .cm-preview code {
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }
  .cm-nondefault { color: var(--warning); }

  /* Path field first, then the picker and the detach — the typed path stays the
     source of truth, the picker is the convenient way to fill it. */
  .cm-root { display: flex; align-items: center; gap: 8px; }
  .cm-root > :global(:first-child) { flex: 1; min-width: 0; }

  .cm-advanced { display: flex; flex-direction: column; gap: 12px; padding-top: 4px; }
  .cm-advanced-head { font-size: var(--font-size-xs); font-weight: 600; color: var(--text-secondary); }

  .cm-secret { display: inline-flex; align-items: flex-start; gap: 6px; line-height: 1.5; }
  .cm-secret :global(svg) { margin-top: 2px; flex-shrink: 0; }

  .cm-test {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 7px 10px;
    background: var(--success-subtle);
    border: 1px solid color-mix(in srgb, var(--success) 30%, transparent);
    border-radius: var(--radius-md);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }
  .cm-test :global(svg) { color: var(--success); }
  .cm-test-bad {
    background: var(--error-subtle);
    border-color: color-mix(in srgb, var(--error) 30%, transparent);
  }
  .cm-test-bad :global(svg) { color: var(--error); }

  .cm-spacer { flex: 1; }
</style>
