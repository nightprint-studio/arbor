<script lang="ts">
  /**
   * One resource of the context: a JDBC data source, an environment entry or a parameter.
   *
   * ## The password
   *
   * In **secret** mode the field is write-only. What is typed goes to the backend's secret store
   * (outside the repository) when the field is left, the field empties, and "Saved" says it is
   * there — the form never gets the value back, so there is nothing for the screen or a screenshot to
   * leak. In **clear** mode it is an ordinary field and the value is written into the generated
   * context; the hint says where that file lives, because that is what "clear" costs.
   *
   * No delete button: removing a resource is the row's trash in the list beside this form.
   */
  import { Check, FolderOpen, Plug } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { jdbcFor, specFromJdbc, testTyped, testWithPicus, type PicusTest } from './tomcat-picus';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { tomcatSetSecret } from '$lib/ipc/bennu/tomcat';
  import type { PasswordMode, TomcatResource } from '$lib/stores/bennu/tomcat-config';
  import type { TomcatForm } from './tomcat-form.svelte';
  import { DRIVER_PRESETS } from './tomcat-drivers';

  let {
    resource,
    configId,
    root,
    form,
    onchange,
  }: {
    resource: TomcatResource;
    configId: string;
    root: string;
    form: TomcatForm;
    onchange: (patch: Partial<TomcatResource>) => void;
  } = $props();

  let pickingJar = $state(false);

  let typed = $state('');
  /** The last password typed here, kept in memory for the check only: the stored one is never read
   *  back, so without this a password typed a moment ago could not be tried. */
  let remembered = '';
  let saveError = $state('');
  const stored = $derived(form.hasSecret(resource.id));

  // ── Checking the connection ─────────────────────────────────────────────────
  // The check is of what this form says: host, database and user come out of the JDBC URL, the
  // password is the one typed. A data source taken from a Picus connection and not changed since is
  // checked as that connection, with the password Picus holds — there is nothing to retype.
  const linked = $derived(form.picus.find((c) => c.id === resource.picusConnection) ?? null);
  const linkedUnchanged = $derived.by(() => {
    const target = linked ? jdbcFor(linked) : null;
    const same = !!target && target.url === resource.url.trim() && linked?.user === resource.user.trim();
    return same ? linked : null;
  });
  const typedSpec = $derived(specFromJdbc(resource.url, resource.user.trim(), `bennu-tomcat-check-${resource.id}`));
  let testing = $state(false);
  let result = $state<PicusTest | null>(null);

  async function runTest() {
    if (testing) return;
    const password = typed || remembered || (resource.passwordMode === 'clear' ? resource.password : '');
    testing = true;
    result = null;
    try {
      if (linkedUnchanged) result = await testWithPicus(linkedUnchanged);
      else if (!typedSpec) result = { ok: false, message: 'Only PostgreSQL and Oracle URLs can be checked here.' };
      else if (!password) {
        result = { ok: false, message: stored ? 'The stored password cannot be read back — type it to check.' : 'Type the password to check.' };
      } else result = await testTyped(typedSpec, password);
    } finally {
      testing = false;
    }
  }

  async function saveSecret() {
    const value = typed;
    if (!value) return;
    saveError = '';
    try {
      await tomcatSetSecret(root, configId, resource.id, value);
      form.markSecret(resource.id, true);
      remembered = value;
      typed = '';
    } catch (e) {
      saveError = e instanceof Error ? e.message : String(e);
    }
  }

  async function forgetSecret() {
    try {
      await tomcatSetSecret(root, configId, resource.id, '');
      form.markSecret(resource.id, false);
      remembered = '';
    } catch (e) {
      saveError = e instanceof Error ? e.message : String(e);
    }
  }

  /** Picking a preset fills the driver and — only when empty — a URL skeleton to complete. */
  function applyPreset(driver: string) {
    const preset = DRIVER_PRESETS.find((p) => p.driver === driver);
    if (!preset) return;
    onchange({ driver: preset.driver, ...(resource.url.trim() ? {} : { url: preset.urlTemplate }) });
  }

  const modeHint = $derived(
    resource.passwordMode === 'secret'
      ? 'Kept outside the repository and handed to Tomcat as a system property. The generated file holds only a reference.'
      : 'Written as plain text into the generated context.xml, inside the project’s .arbor folder.',
  );
  const TYPES = ['java.lang.String', 'java.lang.Integer', 'java.lang.Long', 'java.lang.Boolean', 'java.lang.Double'];
</script>

{#if resource.kind === 'datasource'}
  <FormField label="JNDI name" hint="What the application looks up: java:comp/env/<name>.">
    <Input value={resource.name} placeholder="jdbc/Gestionale" oninput={(v) => onchange({ name: v })} />
  </FormField>

  <FormField label="Driver">
    <div class="pair">
      <Input value={resource.driver} placeholder="org.postgresql.Driver" oninput={(v) => onchange({ driver: v })} />
      <Select
        value=""
        options={[{ value: '', label: 'Presets…' }, ...DRIVER_PRESETS.map((p) => ({ value: p.driver, label: p.label }))]}
        onchange={applyPreset}
      />
    </div>
  </FormField>

  <FormField
    label="Driver jar"
    hint="Empty = found on the project's runtime classpath, or fetched from Maven Central for PostgreSQL, MySQL, MariaDB and SQLite. Choose one for any other driver: it is copied to the server's lib/ at launch."
  >
    <div class="pair">
      <Input value={resource.driverJar} placeholder="Found on the classpath" oninput={(v) => onchange({ driverJar: v })} />
      <Button variant="secondary" size="sm" onclick={() => (pickingJar = true)} tooltip={{ content: 'Choose a driver jar' }}>
        {#snippet iconStart()}<FolderOpen size={13} />{/snippet}
        Browse…
      </Button>
    </div>
  </FormField>

  <FormField label="JDBC URL">
    <Input value={resource.url} placeholder="jdbc:postgresql://localhost:5432/db" oninput={(v) => onchange({ url: v })} />
  </FormField>

  <div class="two">
    <FormField label="User">
      <Input value={resource.user} oninput={(v) => onchange({ user: v })} />
    </FormField>
    <FormField label="Password" error={saveError || null}>
      {#if resource.passwordMode === 'secret'}
        <Input
          type="password"
          value={typed}
          placeholder={stored ? '•••••••• saved — type to replace' : 'Not set'}
          oninput={(v) => (typed = v)}
          onblur={saveSecret}
          onkeydown={(e) => { if (e.key === 'Enter') void saveSecret(); }}
        >
          {#snippet iconEnd()}{#if stored}<span class="saved" title="A password is stored"><Check size={13} /></span>{/if}{/snippet}
        </Input>
      {:else}
        <Input type="password" value={resource.password} oninput={(v) => onchange({ password: v })} />
      {/if}
    </FormField>
  </div>

  <FormField label="Password storage" hint={modeHint}>
    <RadioGroup
      value={resource.passwordMode}
      size="sm"
      options={[
        { value: 'secret', label: 'Stored outside the repo' },
        { value: 'clear', label: 'In the context file' },
      ]}
      onchange={(v) => {
        onchange({ passwordMode: v as PasswordMode });
        if (v === 'clear' && stored) void forgetSecret();
      }}
    />
  </FormField>

  <div class="three">
    <FormField label="maxTotal">
      <NumberStepper value={resource.maxTotal} min={1} max={1000} step={1} narrow={false} ariaLabel="maxTotal"
        onchange={(v) => onchange({ maxTotal: Math.round(v) })} />
    </FormField>
    <FormField label="maxIdle">
      <NumberStepper value={resource.maxIdle} min={0} max={1000} step={1} narrow={false} ariaLabel="maxIdle"
        onchange={(v) => onchange({ maxIdle: Math.round(v) })} />
    </FormField>
    <FormField label="maxWait (ms)">
      <NumberStepper value={resource.maxWaitMs} min={0} max={600000} step={500} narrow={false} ariaLabel="maxWait"
        onchange={(v) => onchange({ maxWaitMs: Math.round(v) })} />
    </FormField>
  </div>

  <div class="two">
    <FormField label="validationQuery">
      <Input value={resource.validationQuery} placeholder="SELECT 1" oninput={(v) => onchange({ validationQuery: v })} />
    </FormField>
    <FormField label="testOnBorrow">
      <label class="check">
        <Toggle checked={resource.testOnBorrow} onchange={(v) => onchange({ testOnBorrow: v })} />
        <span>Check a connection before lending it</span>
      </label>
    </FormField>
  </div>

  <FormField
    label="Check the connection"
    hint="Picus opens a connection with the host, database, user and password above, then closes it. It proves the database answers — not that the driver jar is the right one."
  >
    <div class="test">
      <Button variant="secondary" size="sm" disabled={testing || !!form.picusError} loading={testing} onclick={runTest}>
        {#snippet iconStart()}<Plug size={13} />{/snippet}
        Test connection
      </Button>
      {#if form.picusError}<span class="result bad">Picus is not available</span>{/if}
    </div>
    {#if result}
      <p class="result" class:ok={result.ok} class:bad={!result.ok} role="status">{result.message}</p>
    {/if}
  </FormField>
{:else if resource.kind === 'env'}
  <FormField label="Name" hint="java:comp/env/<name>">
    <Input value={resource.name} placeholder="app/modalita" oninput={(v) => onchange({ name: v })} />
  </FormField>
  <div class="two">
    <FormField label="Type">
      <Select value={resource.valueType} options={TYPES.map((t) => ({ value: t, label: t.replace('java.lang.', '') }))} fill
        onchange={(v) => onchange({ valueType: v })} />
    </FormField>
    <FormField label="Value">
      <Input value={resource.value} oninput={(v) => onchange({ value: v })} />
    </FormField>
  </div>
  <FormField label="Override">
    <label class="check">
      <Toggle checked={resource.allowOverride} onchange={(v) => onchange({ allowOverride: v })} />
      <span>The application’s web.xml may replace this value</span>
    </label>
  </FormField>
{:else}
  <FormField label="Name">
    <Input value={resource.name} placeholder="parameter" oninput={(v) => onchange({ name: v })} />
  </FormField>
  <FormField label="Value">
    <Input value={resource.value} oninput={(v) => onchange({ value: v })} />
  </FormField>
{/if}

{#if pickingJar}
  <FileExplorerModal
    mode="file"
    title="Choose the JDBC driver jar"
    extensions={['jar']}
    initialPath={resource.driverJar || root}
    onConfirm={(path) => { onchange({ driverJar: path }); pickingJar = false; }}
    onCancel={() => (pickingJar = false)}
    onClose={() => (pickingJar = false)}
  />
{/if}

<style>
  .pair { display: grid; grid-template-columns: 1fr 140px; gap: 8px; }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  .three { display: grid; grid-template-columns: repeat(3, 1fr); gap: 14px; }
  .check { display: flex; align-items: center; gap: 8px; color: var(--text-secondary); }
  .saved { display: inline-flex; color: var(--success); }
  .test { display: flex; gap: 8px; align-items: center; }
  .result { margin: 8px 0 0; font-size: var(--font-size-xs); }
  .result.ok { color: var(--success); }
  .result.bad { color: var(--danger); }
</style>
