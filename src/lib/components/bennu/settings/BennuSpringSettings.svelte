<script lang="ts">
  /**
   * Settings › Spring — which dependencies contribute their beans, and the property file this
   * project resolves `${…}` against.
   *
   * The allowlist is four real lists rather than four comma-separated strings: a list pretending to
   * be a string cannot say that its third entry matches no dependency in the project, which is the
   * commonest mistake there is and otherwise shows up only as a panel that stays empty.
   */
  import { Boxes, FileCog, Trash2 } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import TokenListInput, {
    type TokenStatus, type TokenSuggestion,
  } from '$lib/components/shared/ui/TokenListInput.svelte';
  import { bennuConfigStore } from '$lib/stores/bennu/config.svelte';
  import { dependenciesStore } from '$lib/stores/bennu/dependencies.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';

  const cfg = $derived(bennuConfigStore.cfg);
  const root = $derived(projectStore.project?.root ?? null);

  // The page checks every entry against the project's real dependencies and offers them in the add
  // field, so it needs the report — and only it does, which is why this asks here rather than when
  // the dialog opens. The store answers from cache when something else has already read it.
  $effect(() => {
    if (root) void dependenciesStore.load(root);
  });

  const beanAxes = [
    { key: 'group_id', label: 'Group ids', hint: 'com.acme.platform', axis: 'group' as const, exact: true },
    { key: 'group_id_prefix', label: 'Group id prefixes', hint: 'com.acme.  — the trailing dot matters', axis: 'group' as const, exact: false },
    { key: 'artifact_id', label: 'Artifact ids', hint: 'shared-security', axis: 'artifact' as const, exact: true },
    { key: 'artifact_id_prefix', label: 'Artifact id prefixes', hint: 'acme-starter-', axis: 'artifact' as const, exact: false },
  ] as const;

  const libraryBeans = $derived(
    cfg?.library_beans ?? { group_id: [], group_id_prefix: [], artifact_id: [], artifact_id_prefix: [] },
  );
  const allowlistEmpty = $derived(beanAxes.every((a) => (libraryBeans[a.key] ?? []).length === 0));

  /** How many of the project's artifacts the allowlist admits — the answer the four lists add up to,
   *  and the one thing four lists of strings cannot say on their own. */
  const matched = $derived.by(() => {
    if (allowlistEmpty) return 0;
    return projectCoords.filter((c) =>
      beanAxes.some((axis) => {
        const value = axis.axis === 'group' ? c.group : c.artifact;
        return (libraryBeans[axis.key] ?? []).some((entry) =>
          axis.exact ? value === entry : value.startsWith(entry),
        );
      }),
    ).length;
  });

  /**
   * Every `group:artifact` the project actually has — declared and transitive, deduplicated.
   *
   * The transitive ones matter as much as the declared: a starter you want the beans of is usually
   * dragged in by something else, and offering only what a pom names would leave the commonest
   * entry untypeable from the list.
   */
  const projectCoords = $derived.by(() => {
    const report = dependenciesStore.report;
    if (!report) return [] as { group: string; artifact: string }[];
    const seen = new Set<string>();
    const out: { group: string; artifact: string }[] = [];
    const push = (group: string, artifact: string) => {
      if (!artifact) return;
      const key = `${group}:${artifact}`;
      if (seen.has(key)) return;
      seen.add(key);
      out.push({ group, artifact });
    };
    for (const m of report.modules) for (const d of m.dependencies) push(d.group, d.name);
    for (const t of report.transitive) push(t.group, t.name);
    return out;
  });

  /** What the add field offers for one axis: the distinct values the project has, each saying how
   *  many artifacts carry it — so a group that covers twelve jars is visibly the useful entry. */
  function suggestionsFor(axis: 'group' | 'artifact'): TokenSuggestion[] {
    const counts = new Map<string, number>();
    for (const c of projectCoords) {
      const v = axis === 'group' ? c.group : c.artifact;
      if (v) counts.set(v, (counts.get(v) ?? 0) + 1);
    }
    return [...counts.entries()]
      .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
      .map(([value, n]) => ({ value, detail: axis === 'group' ? `${n} artifact${n === 1 ? '' : 's'}` : '' }));
  }

  /**
   * What one entry turned out to mean — the thing a text field could never say.
   *
   * Silent while the report is still loading: "matches nothing" about a project whose dependencies
   * have not been read yet would be a lie with a warning colour on it.
   */
  function statusOf(entry: string, axis: 'group' | 'artifact', exact: boolean): TokenStatus | null {
    if (!dependenciesStore.report) return null;
    const n = projectCoords.filter((c) => {
      const v = axis === 'group' ? c.group : c.artifact;
      return exact ? v === entry : v.startsWith(entry);
    }).length;
    if (n === 0) {
      return {
        label: 'matches nothing',
        tone: 'warning',
        tooltip: `No dependency of this project has ${axis === 'group' ? 'a group id' : 'an artifact id'} ${exact ? 'equal to' : 'starting with'} “${entry}”.`,
      };
    }
    return {
      label: `${n} artifact${n === 1 ? '' : 's'}`,
      tone: 'success',
      tooltip: 'Matched against the project\'s declared and transitive dependencies.',
    };
  }

  /** Refuse the entry that would mean "everything": an empty prefix admits every artifact in the
   *  repository, which is never what a stray keystroke meant. The backend refuses it too. */
  function normalise(raw: string): string | null {
    const t = raw.trim();
    return t.length > 0 ? t : null;
  }

  /**
   * Save one axis. That is the whole of it: the backend's scan cache is keyed **by the allowlist
   * itself**, so the next read re-scans because the key no longer matches.
   */
  async function commitAxis(key: (typeof beanAxes)[number]['key'], next: string[]) {
    await bennuConfigStore.patch({ library_beans: { ...libraryBeans, [key]: next } });
  }

  // ── The property file this project resolves placeholders against ─────────────
  const propertyFiles = $derived(cfg?.spring_property_files ?? {});
  const pinnedKey = $derived(root ? root.replace(/\\/g, '/') : null);
  const pinnedFile = $derived(pinnedKey ? propertyFiles[pinnedKey] ?? null : null);

  async function unpinPropertyFile() {
    if (!pinnedKey) return;
    const next = { ...propertyFiles };
    delete next[pinnedKey];
    await bennuConfigStore.patch({ spring_property_files: next });
  }
</script>

<div class="section-header">
  <h2>Spring</h2>
  <p>Which dependencies contribute their beans, and which property file this project resolves against.</p>
</div>

<div class="card">
  <div class="card-section-title"><Boxes size={12} /> Read beans from these dependencies</div>
  <!-- Where the page stands, in one line: what is admitted, and whether the project can be asked. -->
  <div class="summary" class:summary-empty={allowlistEmpty}>
    {#if allowlistEmpty}
      <span class="summary-dot dot-idle"></span>
      <span>No jar is opened — the <strong>Library beans</strong> view stays empty until something is named here.</span>
    {:else}
      <span class="summary-dot dot-on"></span>
      <span>
        <strong>{matched}</strong> of this project's {projectCoords.length} artifacts
        {matched === 1 ? 'has' : 'have'} its beans read.
      </span>
    {/if}
  </div>
  <p class="set-hint">
    Any match admits an artifact. The entries this is for are your <strong>own</strong> shared modules
    and starters — their beans are plain <code>@Service</code> / <code>@Configuration</code> and simply
    true. Spring Boot's own starters can be added, but their beans are conditional and are shown as such.
  </p>
  <div class="axes">
    {#each beanAxes as axis (axis.key)}
      {@const values = libraryBeans[axis.key] ?? []}
      <div class="axis">
        <div class="axis-head">
          <span class="axis-label">{axis.label}</span>
          {#if values.length}<span class="axis-count">{values.length}</span>{/if}
        </div>
        <TokenListInput
          values={values}
          onchange={(next) => void commitAxis(axis.key, next)}
          placeholder={axis.hint}
          suggestions={suggestionsFor(axis.axis)}
          status={(v) => statusOf(v, axis.axis, axis.exact)}
          normalise={normalise}
          emptyMessage="None yet."
          ariaLabel={axis.label}
        />
      </div>
    {/each}
  </div>
  {#if !dependenciesStore.report}
    <p class="set-hint">
      The project's dependencies have not been read yet, so nothing here can be checked against
      them — open the Dependencies panel, or wait for the index.
    </p>
  {/if}
</div>

<div class="card">
  <div class="card-section-title"><FileCog size={12} /> Property file</div>
  <p class="set-hint">
    Which file a <code>&#36;&#123;placeholder&#125;</code> resolves against. A real project has
    several — <code>application.yml</code>, <code>application-dev.yml</code>, one per module — and
    which one is <em>running</em> is a launch argument rather than something the sources reveal, so
    nothing is guessed: with nothing pinned, the profile-less files answer, which is what Spring
    always loads. The pin itself is made in the Spring panel, on the file.
  </p>
  {#if !root}
    <p class="set-empty">No project open.</p>
  {:else if pinnedFile}
    <div class="pinned">
      <span class="summary-dot dot-on"></span>
      <span class="pinned-file" use:tooltip={pinnedFile}>{pinnedFile.split('/').pop()}</span>
      <span class="pinned-path">{pinnedFile}</span>
      <button class="set-list-del" type="button" onclick={() => void unpinPropertyFile()} aria-label="Unpin the property file">
        <Trash2 size={13} />
      </button>
    </div>
  {:else}
    <div class="pinned pinned-none">
      <span class="summary-dot dot-idle"></span>
      <span>Nothing pinned — the profile-less files answer, which is what Spring always loads.</span>
    </div>
  {/if}
</div>

<style>
  /* Two columns: four full-width lists made a page you scroll to see what is otherwise four short
     lists. An axis is a block and not a row — its list grows downward, so a label beside it would
     drift away from what it names as soon as there were three entries. */
  .axes { display: grid; grid-template-columns: repeat(auto-fit, minmax(240px, 1fr)); gap: 4px 14px; padding: 2px; }
  .axis { padding: 6px 2px; }
  .axis-head { display: flex; align-items: center; gap: 6px; margin-bottom: 4px; }
  .axis-label { font-size: 11px; font-weight: 600; color: var(--text-secondary); letter-spacing: 0.01em; }
  .axis-count {
    padding: 0 5px; border-radius: var(--radius-sm);
    font-size: var(--font-size-2xs); font-weight: 600;
    color: var(--accent); background: var(--accent-subtle);
  }

  /* Where the page stands, before the fields that decide it. */
  .summary {
    display: flex; align-items: center; gap: 8px; margin: 8px 2px 2px; padding: 8px 11px;
    font-size: var(--font-size-xs); line-height: 1.45; color: var(--text-secondary);
    background: color-mix(in srgb, var(--success) 9%, transparent);
    border: 1px solid color-mix(in srgb, var(--success) 26%, transparent);
    border-radius: var(--radius-md);
  }
  .summary-empty {
    background: var(--bg-overlay); border-color: var(--border-subtle); color: var(--text-muted);
  }
  .summary strong { color: var(--text-primary); }
  .summary-dot { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; }
  .dot-on { background: var(--success); }
  .dot-idle { background: var(--text-disabled); }

  .pinned {
    display: flex; align-items: center; gap: 8px; margin: 4px 2px 2px; padding: 7px 10px;
    font-size: var(--font-size-xs); color: var(--text-secondary);
    background: var(--bg-base); border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
  }
  .pinned-none { color: var(--text-muted); }
  .pinned-file { font-family: var(--font-code); color: var(--text-primary); flex-shrink: 0; }
  .pinned-path {
    flex: 1; min-width: 0; font-family: var(--font-code); font-size: var(--font-size-2xs);
    color: var(--text-disabled); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl;
  }
</style>
