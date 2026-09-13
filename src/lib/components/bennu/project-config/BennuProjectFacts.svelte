<script lang="ts">
  /**
   * Project Configuration › Project — what this project is, and the two things you can say
   * differently about it.
   *
   * ## What changed, and why it is shorter than it was
   *
   * This screen used to offer four overrides, three of which went nowhere: a source root, an output
   * root and an excluded-directory list held in memory for the length of a session and written to
   * no file at all. They are gone rather than wired up, for two reasons:
   *
   * - the **source and output roots** are Maven's answer, read from the pom. An editor that lets you
   *   type a different one is offering to disagree with the build, which is a way to make go-to stop
   *   working with no error anywhere;
   * - **excluded directories** already exist, once, in Settings › Java. A second copy per project was
   *   two places to set one thing, and only one of them did anything.
   *
   * What is left is the two that have a real home — `jdk_overrides` and `encoding_overrides` in the
   * profile config, both keyed by project root, both consulted by the backend — and they now
   * actually write there.
   */
  import { Coffee, FileType, Boxes, TriangleAlert } from 'lucide-svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuConfigStore } from '$lib/stores/bennu/config.svelte';
  import { bennuDiagnosticsStore } from '$lib/stores/bennu/diagnostics.svelte';

  /** "No override — use what the manifest says." Empty would be indistinguishable from unset. */
  const AUTO = '__auto__';

  const project = $derived(projectStore.project);
  const root = $derived(project?.root ?? null);
  const cfg = $derived(bennuConfigStore.cfg);

  const resolvedJdk = $derived(project?.jdk?.version ?? null);
  const resolvedJdkSource = $derived(project?.jdk?.source ?? null);
  const declaredEncoding = $derived(project?.source_encoding ?? 'UTF-8');
  const modules = $derived(project?.modules ?? []);
  const isCargo = $derived(projectStore.isCargo);

  /** What the JDK search actually found — the warning that explains a completion with no standard
   *  library in it. Where JDKs are *looked for* is Settings › Java › JDK locations: that is the
   *  machine's answer, this is the project's. */
  const jdkReport = $derived(bennuDiagnosticsStore.jdk);

  const jdkOverride = $derived((root && cfg?.jdk_overrides?.[root]) || AUTO);
  const encodingOverride = $derived((root && cfg?.encoding_overrides?.[root]) || AUTO);

  const jdkOptions = $derived([
    { value: AUTO, label: resolvedJdk ? `Auto — from the manifest (${resolvedJdk})` : 'Auto — from the manifest' },
    { value: '1.8', label: 'Java 8 (1.8)' },
    { value: '11', label: 'Java 11' },
    { value: '17', label: 'Java 17' },
    { value: '21', label: 'Java 21' },
    { value: '25', label: 'Java 25' },
  ]);

  const encodingOptions = $derived([
    { value: AUTO, label: `Auto — as declared (${declaredEncoding})` },
    { value: 'UTF-8', label: 'UTF-8' },
    { value: 'Cp1252', label: 'Cp1252 (Windows-1252)' },
    { value: 'ISO-8859-1', label: 'ISO-8859-1 (Latin-1)' },
    { value: 'US-ASCII', label: 'US-ASCII' },
  ]);

  /**
   * Write one entry of a per-project map, or remove it when the choice is Auto.
   *
   * Removing rather than storing a sentinel is what makes "Auto" keep tracking the manifest: a
   * stored `__auto__` would be an override of its own the day the backend stopped recognising it.
   */
  async function setOverride(map: 'jdk_overrides' | 'encoding_overrides', value: string) {
    const current = cfg;
    if (!root || !current) return;
    const next = { ...(current[map] ?? {}) };
    if (value === AUTO) delete next[root];
    else next[root] = value;
    // Spelled out rather than built from the key: a computed key widens to `string` and the patch
    // stops being checked against the config's own shape, which is the check worth keeping here.
    await bennuConfigStore.patch(
      map === 'jdk_overrides' ? { jdk_overrides: next } : { encoding_overrides: next },
    );
    // The JDK decides the classpath the index resolves against, so the model on screen is stale
    // the moment it changes. Silent on failure — see `refreshProjectInfo`.
    void projectStore.refreshProjectInfo(root);
  }
</script>

<div class="section-header">
  <h2>Project</h2>
  <p>
    What this project is, as Bennu resolved it — and the two things you can say differently. Both are
    remembered per project, in your profile rather than in the repository.
  </p>
</div>

{#if !project}
  <EmptyState message="Open a project to configure it." />
{:else}
  <div class="card">
    <div class="card-section-title"><Coffee size={12} /> Language level</div>
    {#if isCargo}
      <p class="set-hint">
        A Cargo workspace has no JDK — the toolchain is <code>rustup</code>’s, and the edition is
        the manifest’s.
      </p>
    {:else}
      <FormRow
        label="JDK"
        description={resolvedJdkSource
          ? `Resolved from ${resolvedJdkSource}. Override only to pin a different level.`
          : 'The Java language level the classpath is resolved against.'}
      >
        <Select
          value={jdkOverride}
          options={jdkOptions}
          ariaLabel="JDK language level"
          onchange={(v) => void setOverride('jdk_overrides', v)}
        />
      </FormRow>
      {#if jdkReport}
        {#if !jdkReport.any_installed}
          <div class="set-warn set-warn-error">
            <TriangleAlert size={13} />
            No JDK found — completion and navigation can’t resolve the standard library. Add a
            directory under <strong>Settings › Java › JDK locations</strong>.
          </div>
        {:else if !jdkReport.exact}
          <div class="set-warn">
            <TriangleAlert size={13} />
            No JDK for the exact level installed — using Java {jdkReport.resolved_major} as a fallback.
          </div>
        {/if}
        {#if jdkReport.resolved_home}
          <div class="set-kv">
            <span class="set-k">Using</span>
            <code>{jdkReport.resolved_home}</code>
          </div>
        {/if}
      {/if}
    {/if}
  </div>

  <div class="card">
    <div class="card-section-title"><FileType size={12} /> Encoding</div>
    <FormRow
      label="Source encoding"
      description="How this project's files are decoded. Legacy Java projects often declare Cp1252 in the pom; Rust source is UTF-8 by definition."
    >
      <Select
        value={encodingOverride}
        options={encodingOptions}
        ariaLabel="Source encoding"
        onchange={(v) => void setOverride('encoding_overrides', v)}
      />
    </FormRow>
    {#if projectStore.activeFilePath}
      <div class="set-kv">
        <span class="set-k">{projectStore.activeFilePath.split(/[\\/]/).pop()}</span>
        <span class="set-v">decoded as {projectStore.activeEncoding}</span>
      </div>
    {/if}
  </div>

  <div class="card">
    <div class="card-section-title"><Boxes size={12} /> Modules</div>
    {#if modules.length === 0}
      <p class="set-empty">
        {isCargo ? 'Single crate — no workspace members declared.' : 'Single module — no child modules declared.'}
      </p>
    {:else}
      <ul class="mods">
        {#each modules as m (m)}
          <li><Boxes size={12} /><span>{m}</span></li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}

<style>
  .mods {
    list-style: none; margin: 0; padding: 6px 14px 10px;
    display: flex; flex-direction: column;
  }
  .mods li {
    display: flex; align-items: center; gap: 8px;
    padding: 5px 0; font-size: var(--font-size-sm); color: var(--text-primary);
  }
  .mods li :global(svg) { color: var(--accent); opacity: 0.8; flex-shrink: 0; }
  .mods li span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
