<script lang="ts">
  /**
   * How the application reaches Tomcat.
   *
   * **Live** is the default and the point of the whole feature: Tomcat serves `src/main/webapp`
   * itself, with the module's compiled classes and its dependencies mounted underneath, so there is
   * no WAR and no copy — a JSP saved in the editor is the JSP the next request runs. **External**
   * serves something built elsewhere (a WAR, an exploded folder) for the cases where the packaging
   * is part of what is being tested.
   */
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import type { RunConfig } from '$lib/stores/bennu/run-config.svelte';
  import type { TomcatSettings } from '$lib/stores/bennu/tomcat-config';
  import type { TomcatForm } from './tomcat-form.svelte';
  import BennuTomcatLibsField from './BennuTomcatLibsField.svelte';

  let {
    config,
    patch,
    patchT,
    form,
    root,
    moduleOptions,
    isMultiModule,
  }: {
    config: RunConfig;
    patch: (p: Partial<Omit<RunConfig, 'id'>>) => void;
    patchT: (p: Partial<TomcatSettings>) => void;
    form: TomcatForm;
    root: string;
    moduleOptions: { value: string; label: string }[];
    isMultiModule: boolean;
  } = $props();

  const t = $derived(config.tomcat);
  const err = (field: string) => form.issues.find((i) => i.tab === 'deploy' && i.field === field)?.message ?? null;
  const general = $derived(form.issues.find((i) => i.tab === 'deploy' && !i.field)?.message ?? null);
</script>

<FormField
  label="Web module"
  hint={isMultiModule ? 'The module with src/main/webapp. Its classes and dependencies are what Tomcat runs.' : 'This project has one module.'}
  error={general}
>
  <Select
    value={config.module}
    options={moduleOptions}
    fill
    disabled={!isMultiModule}
    onchange={(v) => patch({ module: v })}
  />
</FormField>

<FormField label="Deployment">
  <RadioGroup
    value={t.deploy}
    size="sm"
    block
    options={[
      { value: 'live', label: 'Live — serve src/main/webapp' },
      { value: 'external', label: 'External — a WAR or folder' },
    ]}
    onchange={(v) => patchT({ deploy: v === 'external' ? 'external' : 'live' })}
  />
</FormField>

{#if t.deploy === 'external'}
  <FormField label="WAR or exploded folder" hint="Absolute path. Nothing is built for you: build it, then run." error={err('docBase')}>
    <Input value={t.docBase} placeholder="C:/work/app/target/app.war" oninput={(v) => patchT({ docBase: v })} />
  </FormField>
{:else}
  <p class="live-note">
    Compiled classes and runtime dependencies are mounted under <code>WEB-INF</code> when Tomcat starts — nothing is packaged,
    and a saved JSP is served as it is.
  </p>
{/if}

<FormField
  label="Context path"
  hint="Where the application is served: “/” for the root, or “/gestionale”."
  error={err('contextPath')}
>
  <Input value={t.contextPath} placeholder="/" oninput={(v) => patchT({ contextPath: v })} />
</FormField>

<FormField label="Reload on class change" hint="Tomcat's reloadable: restarts the context when WEB-INF/classes changes.">
  <label class="check">
    <Toggle checked={t.reloadable} onchange={(v) => patchT({ reloadable: v })} />
    <span>Reload the context when classes change</span>
  </label>
</FormField>

<FormField label="Server libraries">
  <BennuTomcatLibsField paths={t.libPaths} {root} onchange={(libPaths) => patchT({ libPaths })} />
</FormField>

<style>
  .live-note { margin: 0 0 14px; color: var(--text-muted); font-size: var(--font-size-xs); line-height: 1.5; }
  .check { display: flex; align-items: center; gap: 8px; color: var(--text-secondary); }
  code { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); }
</style>
