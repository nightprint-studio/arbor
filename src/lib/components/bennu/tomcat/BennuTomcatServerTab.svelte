<script lang="ts">
  /**
   * The server half of a Tomcat run configuration: which Tomcat, and on which ports.
   *
   * The installation is the one decision that cannot be defaulted, so it comes first and says what
   * follows from it — the servlet package the container speaks and the Java it needs — because those
   * two facts are what a "Tomcat does not start" turns out to be nine times in ten.
   *
   * Ports are checked as they are typed (the form asks the backend whether something is listening),
   * so a taken 8080 is a red line under the field and not a stack trace in a console.
   */
  import { Download, FolderOpen } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import type { RunConfig } from '$lib/stores/bennu/run-config.svelte';
  import type { TomcatSettings } from '$lib/stores/bennu/tomcat-config';
  import type { TomcatForm } from './tomcat-form.svelte';

  let {
    config,
    patchT,
    form,
    root,
    onDownload,
  }: {
    config: RunConfig;
    patchT: (p: Partial<TomcatSettings>) => void;
    form: TomcatForm;
    root: string;
    onDownload: () => void;
  } = $props();

  const t = $derived(config.tomcat);
  const same = (a: string, b: string) => a.replace(/\\/g, '/').toLowerCase() === b.replace(/\\/g, '/').toLowerCase();

  /** The downloaded installations, plus the chosen folder when it is one Bennu did not download. */
  const options = $derived.by(() => {
    const installs = form.env?.installs ?? [];
    const opts = installs.map((i) => ({ value: i.home, label: `Apache Tomcat ${i.version} · ${i.namespace}.servlet` }));
    if (t.home && !installs.some((i) => same(i.home, t.home))) {
      opts.unshift({ value: t.home, label: form.installation ? `Tomcat ${form.installation.version} — ${t.home}` : t.home });
    }
    return [{ value: '', label: installs.length ? 'Choose an installation…' : 'No Tomcat installed yet' }, ...opts];
  });

  const err = (field: string) => form.issues.find((i) => i.tab === 'server' && i.field === field)?.message ?? null;

  let browsing = $state(false);
  const projectNs = $derived(form.scan?.namespace ?? null);
</script>

<FormField label="Tomcat" hint="Downloaded here, or any folder holding a Tomcat 8.5, 9, 10.1 or 11." error={err('home')}>
  <div class="row">
    <Select value={t.home} {options} fill onchange={(v) => patchT({ home: v })} />
    <Button variant="secondary" size="sm" onclick={onDownload} tooltip={{ content: 'Download a Tomcat' }}>
      {#snippet iconStart()}<Download size={13} />{/snippet}
      Download…
    </Button>
    <Button variant="secondary" size="sm" onclick={() => (browsing = true)} tooltip={{ content: 'Use a Tomcat already on this machine' }}>
      {#snippet iconStart()}<FolderOpen size={13} />{/snippet}
      Browse…
    </Button>
  </div>
  {#if form.installation}
    {@const inst = form.installation}
    <div class="chips">
      <Badge variant="tone" tone="neutral" size="sm" label={`Servlet ${inst.servlet}`} />
      <Badge variant="tone" tone={inst.namespace === 'jakarta' ? 'accent' : 'warning'} size="sm" label={`${inst.namespace}.servlet`} />
      <Badge variant="tone" tone="neutral" size="sm" label={`Java ${inst.min_java}+`} />
      {#if projectNs}
        <Badge
          variant="tone"
          tone={projectNs === inst.namespace ? 'success' : 'error'}
          size="sm"
          label={projectNs === inst.namespace ? `✓ matches the project (${projectNs})` : `✗ the project uses ${projectNs}`}
        />
      {/if}
    </div>
  {/if}
</FormField>

<div class="two">
  <FormField label="HTTP port" error={err('httpPort')}>
    <NumberStepper
      value={t.httpPort}
      min={1}
      max={65535}
      step={1}
      narrow={false}
      ariaLabel="HTTP port"
      onchange={(v) => patchT({ httpPort: Math.round(v) })}
    />
  </FormField>
  <FormField label="Shutdown port" hint="Tomcat's own control port." error={err('shutdownPort')}>
    <NumberStepper
      value={t.shutdownPort}
      min={1}
      max={65535}
      step={1}
      narrow={false}
      ariaLabel="Shutdown port"
      onchange={(v) => patchT({ shutdownPort: Math.round(v) })}
    />
  </FormField>
</div>

<FormField label="After it starts">
  <label class="check">
    <Toggle checked={t.openBrowser} onchange={(v) => patchT({ openBrowser: v })} />
    <span>Open <code>http://localhost:{t.httpPort}{t.contextPath === '/' ? '/' : t.contextPath}</code> in the browser</span>
  </label>
</FormField>

<FormSection label="Folders" collapsible open={!!(t.installDir || t.baseDir)} summary={t.installDir || t.baseDir ? 'custom' : 'defaults'}>
  <FormField label="Downloaded Tomcats" hint="Where “Download…” unpacks. Empty = one folder in your Bennu profile, shared by every project.">
    <Input value={t.installDir} placeholder={form.env?.install_dir ?? ''} oninput={(v) => patchT({ installDir: v })} />
  </FormField>
  <FormField label="Generated servers" hint="One folder per configuration (server.xml, context, logs, work). Empty = .arbor/bennu/tomcat in the project.">
    <Input value={t.baseDir} placeholder={form.env?.base_dir ?? ''} oninput={(v) => patchT({ baseDir: v })} />
  </FormField>
</FormSection>

{#if browsing}
  <FileExplorerModal
    mode="folder"
    title="Pick a Tomcat folder"
    initialPath={t.home || root}
    onConfirm={(dir) => { patchT({ home: dir }); browsing = false; }}
    onCancel={() => (browsing = false)}
    onClose={() => (browsing = false)}
  />
{/if}

<style>
  .row { display: flex; gap: 8px; align-items: center; }
  .row :global(.select), .row > :first-child { flex: 1; min-width: 0; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 8px; }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  .check { display: flex; align-items: center; gap: 8px; color: var(--text-secondary); }
  code { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); }
</style>
