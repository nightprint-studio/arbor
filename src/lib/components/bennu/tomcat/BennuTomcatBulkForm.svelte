<script lang="ts">
  /**
   * Edit several resources at once.
   *
   * What a legacy context needs most is the same change on every data source: the pool sizes
   * everywhere, the host swapped from the test server to localhost, every password kept out of the
   * file. Each section here applies to **all selected data sources** in one press and says how many
   * it touched; nothing is applied as it is typed, because a field half-typed over twelve
   * resources is twelve mistakes.
   *
   * Environment entries and parameters in the selection are left alone by the sections that mean
   * nothing to them, and the count says so.
   */
  import { Trash2 } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { tomcatSetSecret } from '$lib/ipc/bennu/tomcat';
  import type { PasswordMode, TomcatResource } from '$lib/stores/bennu/tomcat-config';
  import type { TomcatForm } from './tomcat-form.svelte';

  let {
    resources,
    configId,
    root,
    form,
    onapply,
    ondelete,
  }: {
    /** The selected resources. */
    resources: TomcatResource[];
    configId: string;
    root: string;
    form: TomcatForm;
    /** Apply `patch` to every resource whose id is in `ids`. */
    onapply: (ids: string[], patch: (r: TomcatResource) => Partial<TomcatResource>) => void;
    ondelete: (ids: string[]) => void;
  } = $props();

  const sources = $derived(resources.filter((r) => r.kind === 'datasource'));
  const ids = $derived(sources.map((r) => r.id));

  // Seeded from the first data source, so the common case — they are all the same — shows the
  // values they have and "apply" repeats them.
  let maxTotal = $state(20);
  let maxIdle = $state(5);
  let maxWaitMs = $state(10000);
  let validationQuery = $state('');
  let testOnBorrow = $state(true);
  let from = $state('');
  let to = $state('');
  let mode = $state<PasswordMode>('secret');
  let driver = $state('');
  let done = $state('');

  $effect(() => {
    const first = sources[0];
    if (!first) return;
    maxTotal = first.maxTotal;
    maxIdle = first.maxIdle;
    maxWaitMs = first.maxWaitMs;
    validationQuery = first.validationQuery;
    testOnBorrow = first.testOnBorrow;
    mode = first.passwordMode;
    driver = first.driver;
  });

  const said = (what: string) => (done = `${what} on ${ids.length} data source${ids.length === 1 ? '' : 's'}.`);

  function applyPool() {
    onapply(ids, () => ({ maxTotal, maxIdle, maxWaitMs, validationQuery, testOnBorrow }));
    said('Pool settings set');
  }

  function replaceInUrls() {
    if (!from) return;
    let touched = 0;
    onapply(ids, (r) => {
      if (!r.url.includes(from)) return {};
      touched += 1;
      return { url: r.url.split(from).join(to) };
    });
    done = touched ? `“${from}” replaced in ${touched} URL${touched === 1 ? '' : 's'}.` : `No URL contains “${from}”.`;
  }

  async function applyMode() {
    onapply(ids, () => ({ passwordMode: mode, ...(mode === 'secret' ? { password: '' } : {}) }));
    // Leaving secret mode drops the stored copy: it would otherwise sit unused where nobody looks.
    if (mode === 'clear') {
      await Promise.all(
        ids.filter((id) => form.hasSecret(id)).map(async (id) => {
          await tomcatSetSecret(root, configId, id, '');
          form.markSecret(id, false);
        }),
      ).catch(() => {});
    }
    said(mode === 'secret' ? 'Passwords moved out of the context file' : 'Passwords written into the context file');
  }

  function applyDriver() {
    onapply(ids, () => ({ driver }));
    said('Driver set');
  }
</script>

<div class="head">
  <strong>{resources.length} resources selected</strong>
  <span class="sub">{sources.length} data source{sources.length === 1 ? '' : 's'}{resources.length > sources.length ? ' · the rest are not touched by these' : ''}</span>
  <span class="sp"></span>
  <Button variant="danger" size="sm" onclick={() => ondelete(resources.map((r) => r.id))}>
    {#snippet iconStart()}<Trash2 size={13} />{/snippet}
    Delete {resources.length}
  </Button>
</div>

{#if sources.length === 0}
  <p class="none">None of the selected resources is a data source, so there is nothing to edit together.</p>
{:else}
  <FormSection label="Pool" collapsible open summary="maxTotal {maxTotal} · maxIdle {maxIdle}">
    <div class="three">
      <FormField label="maxTotal" hint="-1 = no limit">
        <NumberStepper value={maxTotal} min={-1} max={1000} step={1} narrow={false} ariaLabel="maxTotal" onchange={(v) => (maxTotal = Math.round(v))} />
      </FormField>
      <FormField label="maxIdle" hint="-1 = no limit">
        <NumberStepper value={maxIdle} min={-1} max={1000} step={1} narrow={false} ariaLabel="maxIdle" onchange={(v) => (maxIdle = Math.round(v))} />
      </FormField>
      <FormField label="maxWait (ms)" hint="-1 = wait forever">
        <NumberStepper value={maxWaitMs} min={-1} max={600000} step={500} narrow={false} ariaLabel="maxWait" onchange={(v) => (maxWaitMs = Math.round(v))} />
      </FormField>
    </div>
    <div class="two">
      <FormField label="validationQuery"><Input value={validationQuery} oninput={(v) => (validationQuery = v)} /></FormField>
      <FormField label="testOnBorrow">
        <label class="check"><Toggle checked={testOnBorrow} onchange={(v) => (testOnBorrow = v)} /><span>Check before lending</span></label>
      </FormField>
    </div>
    <Button variant="secondary" size="sm" onclick={applyPool}>Apply to {sources.length}</Button>
  </FormSection>

  <FormSection label="Connection" collapsible open summary="driver · find & replace in URLs">
    <FormField label="Driver">
      <div class="pair">
        <Input value={driver} placeholder="org.postgresql.Driver" oninput={(v) => (driver = v)} />
        <Button variant="secondary" size="sm" onclick={applyDriver} disabled={!driver.trim()}>Apply</Button>
      </div>
    </FormField>
    <FormField label="Replace in URLs" hint="Swap a host or a database name in every selected URL — e.g. test-db → localhost.">
      <div class="pair3">
        <Input value={from} placeholder="find" oninput={(v) => (from = v)} />
        <Input value={to} placeholder="replace with" oninput={(v) => (to = v)} />
        <Button variant="secondary" size="sm" onclick={replaceInUrls} disabled={!from}>Replace</Button>
      </div>
    </FormField>
  </FormSection>

  <FormSection label="Passwords" collapsible open summary={mode === 'secret' ? 'outside the repo' : 'in the context file'}>
    <FormField label="Storage" hint="Applies to every selected data source. Moving to the file forgets the stored copies.">
      <div class="pair">
        <RadioGroup
          value={mode}
          size="sm"
          options={[{ value: 'secret', label: 'Stored outside the repo' }, { value: 'clear', label: 'In the context file' }]}
          onchange={(v) => (mode = v as PasswordMode)}
        />
        <Button variant="secondary" size="sm" onclick={applyMode}>Apply</Button>
      </div>
    </FormField>
  </FormSection>
{/if}

{#if done}<p class="done" role="status">{done}</p>{/if}

<style>
  .head { display: flex; align-items: center; gap: 10px; margin-bottom: 14px; }
  .sub { color: var(--text-muted); font-size: var(--font-size-xs); }
  .sp { flex: 1; }
  .none { color: var(--text-muted); }
  .pair { display: flex; gap: 8px; align-items: center; }
  .pair > :first-child { flex: 1; min-width: 0; }
  .pair3 { display: grid; grid-template-columns: 1fr 1fr auto; gap: 8px; align-items: center; }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  .three { display: grid; grid-template-columns: repeat(3, 1fr); gap: 14px; }
  .check { display: flex; align-items: center; gap: 8px; color: var(--text-secondary); }
  .done { margin: 12px 0 0; color: var(--success); font-size: var(--font-size-xs); }
</style>
