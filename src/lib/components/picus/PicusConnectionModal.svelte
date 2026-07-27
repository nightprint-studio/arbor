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
  import { Database, KeyRound, Plug, ShieldCheck, CircleAlert, CheckCircle2 } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import ColorPalettePicker from '$lib/components/shared/ui/ColorPalettePicker.svelte';
  import PicusDialectChip from './PicusDialectChip.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import type { Connection, Dialect } from '$lib/types/picus';

  interface Props {
    /** Connection to edit; `null` creates a new one. */
    connectionId: string | null;
    onClose: () => void;
  }

  let { connectionId, onClose }: Props = $props();

  const existing = $derived(connectionId ? connectionsStore.byId(connectionId) : null);

  /** Split a stored `host:port/service` back into its three parts. */
  function splitHost(host: string | undefined, dialect: Dialect) {
    const fallbackPort = dialect === 'oracle' ? 1521 : 5432;
    if (!host) return { host: '', port: fallbackPort, service: '' };
    const [hostPort, service = ''] = host.split('/');
    const [h, p] = hostPort.split(':');
    return { host: h ?? '', port: Number(p) || fallbackPort, service };
  }

  // Seeded once from the connection under edit — this is a form, not a live view.
  const seed = splitHost(existing?.host, existing?.dialect ?? 'oracle');

  let name = $state(existing?.name ?? '');
  let alias = $state(existing?.alias ?? '');
  let dialect = $state<Dialect>(existing?.dialect ?? 'oracle');
  let host = $state(seed.host);
  let port = $state(seed.port);
  let service = $state(seed.service);
  let schema = $state(existing?.schema ?? '');
  let username = $state('');
  let colorIdx = $state(existing?.colorIdx ?? 2);
  let readOnly = $state(existing?.readOnly ?? false);
  let connectTimeout = $state(15);
  let extraParams = $state('');
  let savePassword = $state(true);

  /** MOCK: the connection test. Real probes come with the driver. */
  let testing = $state(false);
  let testResult = $state<{ ok: boolean; message: string } | null>(null);

  let firstField = $state<HTMLInputElement | undefined>();
  $effect(() => { firstField?.focus(); });

  const dialectOptions = [
    { value: 'oracle', label: 'Oracle' },
    { value: 'postgres', label: 'PostgreSQL' },
  ];

  // Each engine names the last part of the address differently — and defaults
  // to a different port. Switching the engine moves both.
  const serviceLabel = $derived(dialect === 'oracle' ? 'Service name' : 'Database');
  const servicePlaceholder = $derived(dialect === 'oracle' ? 'DEVPDB' : 'appprod');
  const schemaPlaceholder = $derived(dialect === 'oracle' ? 'APPPROD' : 'public');
  const defaultPort = $derived(dialect === 'oracle' ? 1521 : 5432);

  function switchDialect(next: Dialect) {
    // Only move the port if it was still the other engine's default — never
    // overwrite a port the user typed on purpose.
    const previousDefault = dialect === 'oracle' ? 1521 : 5432;
    if (port === previousDefault) port = next === 'oracle' ? 1521 : 5432;
    dialect = next;
    testResult = null;
  }

  const composedHost = $derived(
    `${host.trim()}${port ? `:${port}` : ''}${service.trim() ? `/${service.trim()}` : ''}`,
  );

  const valid = $derived(name.trim() !== '' && host.trim() !== '' && service.trim() !== '');

  function test() {
    if (!valid) return;
    testing = true;
    testResult = null;
    setTimeout(() => {
      testing = false;
      testResult = {
        ok: true,
        message: `Reached ${composedHost} — the real probe arrives with the driver.`,
      };
    }, 700);
  }

  function save() {
    if (!valid) return;
    const conn: Connection = {
      id: existing?.id ?? `conn-${Date.now().toString(36)}`,
      name: name.trim(),
      alias: alias.trim() || 'unnamed',
      dialect,
      schema: schema.trim() || schemaPlaceholder,
      host: composedHost,
      state: 'disconnected',
      dbVersion: existing?.dbVersion ?? '—',
      colorIdx,
      readOnly,
    };
    connectionsStore.upsert(conn);
    toastStore.show(`${conn.name} saved.`, 'success');
    onClose();
  }

  function onKeyDown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      save();
    }
  }
</script>

<Modal {onClose} width="720px" height="640px" padBody={false} ariaLabel="Connection">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Database size={14} />
      <span class="modal-title">{existing ? `Edit ${existing.name}` : 'New connection'}</span>
      <PicusDialectChip {dialect} />
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="cm" onkeydown={onKeyDown} role="form">
    <section class="cm-section">
      <h2>Identity</h2>
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
        hint="Shown on the sidebar row, on every tab bound to this session, and in the status bar — the way two databases stay distinguishable."
      >
        <ColorPalettePicker
          colors={Array.from({ length: 12 }, (_, i) => `var(--ws-color-${i})`)}
          value={colorIdx}
          onChange={(i) => (colorIdx = i)}
          ariaLabel="Connection colour"
        />
      </FormField>
    </section>

    <section class="cm-section">
      <h2>Server</h2>
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
        <FormField label="Schema" hint="Where unqualified names resolve.">
          <Input value={schema} placeholder={schemaPlaceholder} oninput={(v) => (schema = v)} />
        </FormField>
        <FormField label="Username" hint="Stored with the connection; the password is not.">
          <Input value={username} placeholder="appuser" oninput={(v) => (username = v)} />
        </FormField>
      </div>
    </section>

    <section class="cm-section">
      <h2>Session</h2>
      <FormField label="Read-only">
        <Toggle
          checked={readOnly}
          size="sm"
          label="Refuse every write on this connection"
          description="Enforced by the backend, not just hidden in the interface. Use it for production."
          onchange={(v) => (readOnly = v)}
        />
      </FormField>

      <Collapsible chevron>
        {#snippet header()}
          <span class="cm-advanced-head">Advanced</span>
        {/snippet}
        <div class="cm-advanced">
          <FormField label="Connect timeout" hint="Seconds before giving up on the handshake.">
            <NumberStepper value={connectTimeout} min={1} max={300} onchange={(v) => (connectTimeout = v)} />
          </FormField>
          <FormField
            label="Extra parameters"
            hint="Passed to the driver as-is (sslmode=require, oracle.net.CONNECT_TIMEOUT=…). One per line."
          >
            <Input value={extraParams} placeholder="sslmode=require" oninput={(v) => (extraParams = v)} />
          </FormField>
        </div>
      </Collapsible>
    </section>

    <section class="cm-section">
      <h2>Credentials</h2>
      <Alert variant="info" compact>
        <span class="cm-secret">
          <KeyRound size={12} />
          <span>
            The password is not stored here. Picus asks Arbor's keychain for it at connection
            time, so it never lands in a project file, a configuration or a log.
          </span>
        </span>
      </Alert>
      <FormField label="Remember in the keychain">
        <Toggle
          checked={savePassword}
          size="sm"
          label="Ask once, then reuse"
          description="Off means Picus prompts on every connect and keeps nothing."
          onchange={(v) => (savePassword = v)}
        />
      </FormField>

      {#if testResult}
        <div class="cm-test" class:cm-test-bad={!testResult.ok}>
          {#if testResult.ok}<CheckCircle2 size={13} />{:else}<CircleAlert size={13} />{/if}
          <span>{testResult.message}</span>
        </div>
      {/if}
    </section>
  </div>

  {#snippet footer()}
    <Button
      variant="secondary"
      size="sm"
      onclick={() => toastStore.show('Storing the secret in the keychain arrives with the driver milestone.', 'info')}
    >
      {#snippet iconStart()}<ShieldCheck size={13} />{/snippet}
      Set the password…
    </Button>
    <Button variant="secondary" size="sm" disabled={!valid || testing} onclick={test}>
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
      disabled={!valid}
      tooltip={{ content: 'Save the connection', shortcut: 'Ctrl+Enter' }}
      onclick={save}
    >
      Save
    </Button>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }

  .cm {
    display: flex;
    flex-direction: column;
    gap: 18px;
    height: 100%;
    overflow-y: auto;
    padding: 16px;
  }

  .cm-section { display: flex; flex-direction: column; gap: 12px; }
  .cm-section h2 {
    margin: 0;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

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
    font-size: 11px;
    color: var(--text-muted);
  }
  .cm-preview code {
    font-family: var(--font-code);
    font-size: 11.5px;
    color: var(--text-secondary);
  }
  .cm-nondefault { color: var(--warning); }

  .cm-advanced { display: flex; flex-direction: column; gap: 12px; padding-top: 4px; }
  .cm-advanced-head { font-size: 11.5px; font-weight: 600; color: var(--text-secondary); }

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
    font-size: 11.5px;
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
