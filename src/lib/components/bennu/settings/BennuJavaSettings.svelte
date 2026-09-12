<script lang="ts">
  /**
   * Settings › Java — how Java sources are decoded, indexed and searched, and what the DTO Lab's
   * JVM is allowed to cost.
   */
  import { Beaker, Braces, Package } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { bennuConfigStore } from '$lib/stores/bennu/config.svelte';
  import {
    bennuSettingsStore, SOURCE_ENCODINGS, type SourceEncoding,
  } from '$lib/stores/bennu/settings.svelte';
  import type { MavenConfigDto } from '$lib/ipc/bennu/config';

  let { onGoTo }: { onGoTo?: (page: string) => void } = $props();

  const s = bennuSettingsStore;
  const cfg = $derived(bennuConfigStore.cfg);

  const encodingOptions = SOURCE_ENCODINGS.map((e) => ({ value: e, label: e }));

  const validateOnOpen = $derived(cfg?.validate_on_open ?? true);
  const validationThreads = $derived(cfg?.validation_threads ?? 0);
  const indexThreads = $derived(cfg?.index_threads ?? 1);

  /** The `[maven]` section, with a complete default so a config written before it existed reads as
   *  "on, a day" rather than as a half-populated object. */
  const mavenCfg = $derived<MavenConfigDto>(cfg?.maven ?? { central: true, metadata_ttl_hours: 24 });

  async function commitThreads(field: 'validation_threads' | 'index_threads', raw: string) {
    const n = Math.max(0, Math.floor(Number(raw) || 0));
    await bennuConfigStore.patch({ [field]: n });
  }
</script>

<div class="section-header">
  <h2>Java</h2>
  <p>How Java sources are decoded and indexed.</p>
</div>
<div class="card">
  <div class="card-section-title"><Braces size={12} /> Sources</div>
  <FormRow label="Default source encoding" description="Fallback when the pom doesn't declare project.build.sourceEncoding.">
    <Select value={s.defaultEncoding} options={encodingOptions}
            onchange={(v) => s.setDefaultEncoding(v as SourceEncoding)} />
  </FormRow>
  <FormRow
    label="Download missing dependencies"
    description="Let the dependency resolve reach the network when a jar is not in ~/.m2 yet. On by default: types from a jar that was never downloaded read as unresolved, and the only other way out is to leave the editor and run a build. Off resolves from ~/.m2 alone — worth it on a metered connection, or behind a VPN that makes the corporate repository slow."
  >
    <Toggle checked={s.mavenAutoDownload} onchange={(v) => s.setMavenAutoDownload(v)} ariaLabel="Download missing dependencies" />
  </FormRow>
  <!-- The other thing on this side that reaches the network, and the only one that is about
       versions rather than about jars. Beside the download toggle because both answer "may Bennu
       talk to a repository", and because this is where someone comes looking. -->
  <FormRow
    label="Check Maven Central for newer versions"
    description="Reads each dependency's maven-metadata.xml to mark a version in a pom that is behind, and offers the newer one in a click. Only versions the pom itself pins — one inherited from a parent or written as a property reference is left alone. Answers come from a cache on disk, refreshed at most once a day per artifact. Off keeps the Java side entirely local; nothing else changes."
  >
    <Toggle checked={mavenCfg.central} onchange={(on) => void bennuConfigStore.patch({ maven: { ...mavenCfg, central: on } })}
            ariaLabel="Check Maven Central for newer versions" />
  </FormRow>
  <FormRow label="Rebuild index on open" description="Re-scan symbols each time a project opens (slower open, fresher completion).">
    <Toggle checked={s.rebuildIndexOnOpen} onchange={(v) => s.setRebuildIndexOnOpen(v)} ariaLabel="Rebuild index on open" />
  </FormRow>
  <FormRow label="Validate project on open" description="After indexing, validate the whole project in the background so the first ‘Validate (no compile)’ is instant. Uses a little CPU on open.">
    <Toggle checked={validateOnOpen} onchange={(v) => void bennuConfigStore.patch({ validate_on_open: v })} ariaLabel="Validate project on open" />
  </FormRow>
  <FormRow label="Validation CPU threads" description="Max worker threads the whole-project validation may use. 0 = auto (leaves about half the cores free for the UI). Set 1 for single-threaded so a big project can’t peg every core and freeze the editor.">
    <Input value={String(validationThreads)} placeholder="0"
           onchange={(v) => void commitThreads('validation_threads', v)} ariaLabel="Validation CPU threads" />
  </FormRow>
  <FormRow label="Indexing CPU threads" description="Max worker threads the index build, the find-usages reference walk and the encoding scan may use. 1 = serial, the default — indexing is a background job and one that makes the machine unusable has not earned its speed. Raise it when indexing feels slow and there are cores to spare; 0 = auto.">
    <Input value={String(indexThreads)} placeholder="1"
           onchange={(v) => void commitThreads('index_threads', v)} ariaLabel="Indexing CPU threads" />
  </FormRow>
  <FormRow label="Excluded directories" description="Comma-separated folder names skipped by the indexer.">
    <Input value={s.excludedDirs} placeholder="target, .git"
           onchange={(v) => s.setExcludedDirs(v)} ariaLabel="Excluded directories" />
  </FormRow>
</div>
<div class="card">
  <div class="card-section-title"><Package size={12} /> Navigation</div>
  <FormRow
    label="Search the dependencies too"
    description="Open Go to (Ctrl+N / Ctrl+Shift+N) and Find in project on Project & dependencies rather than on Project alone, so a framework annotation, a struts-default.xml or a schema is found without asking for it. Either way the Source picker still decides the search in front of you. The jars are searched as you type rather than listed, so they cost nothing until used; the first search after opening a project spends a moment reading them."
  >
    <Toggle checked={s.searchDependencies} onchange={(v) => void s.setSearchDependencies(v)}
            ariaLabel="Search the dependencies too" />
  </FormRow>
</div>
<div class="card">
  <div class="card-section-title"><Beaker size={12} /> DTO Lab</div>
  <FormRow
    label="Stop the JVM after"
    description="The DTO Lab answers from a JVM running the project's own classes. It starts the first time the lab needs it — a second or two — and holds those classes in memory while it runs. After this long with no questions it is stopped, and the next question starts it again."
  >
    <NumberStepper value={s.dtoLabIdleMinutes} min={1} max={240} narrow suffix="min"
                   onchange={(v) => s.setDtoLabIdleMinutes(v)} ariaLabel="Minutes before the DTO Lab JVM is stopped" />
  </FormRow>
  <FormRow
    label="Test values"
    description="What a field is filled with in generated tests and in the payload sketch, for its name or a constraint it carries — email, tax code, IBAN and the like, and your own."
  >
    <Button variant="ghost" size="sm" onclick={() => onGoTo?.('test-values')}>Edit test values…</Button>
  </FormRow>
</div>
{#if s.excludedDirList.length}
  <div class="card">
    <div class="card-section-title"><Braces size={12} /> Parsed exclusions ({s.excludedDirList.length})</div>
    <div class="set-chips">
      {#each s.excludedDirList as d (d)}
        <Badge variant="tone" tone="neutral" label={d} />
      {/each}
    </div>
  </div>
{/if}
