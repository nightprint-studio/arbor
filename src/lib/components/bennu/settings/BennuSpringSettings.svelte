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
  <p class="set-empty">
    Any match admits an artifact. The intended entries are your <strong>own</strong> shared modules
    and starters — their beans are plain <code>@Service</code> / <code>@Configuration</code> and
    simply true. Spring Boot's own starters can be added, but their beans are conditional and are
    shown as such.
  </p>
  {#each beanAxes as axis (axis.key)}
    <div class="axis">
      <div class="axis-head">{axis.label}</div>
      <TokenListInput
        values={libraryBeans[axis.key] ?? []}
        onchange={(next) => void commitAxis(axis.key, next)}
        placeholder={axis.hint}
        suggestions={suggestionsFor(axis.axis)}
        status={(v) => statusOf(v, axis.axis, axis.exact)}
        normalise={normalise}
        emptyMessage="None."
        ariaLabel={axis.label}
      />
    </div>
  {/each}
  {#if !dependenciesStore.report}
    <p class="set-empty">
      The project's dependencies have not been read yet, so nothing here can be checked against
      them — open the Dependencies panel, or wait for the index.
    </p>
  {/if}
  {#if allowlistEmpty}
    <p class="set-empty">Empty — no dependency jar is opened, and the Library beans view stays empty.</p>
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
    <div class="set-list">
      <div class="set-list-row">
        <span class="set-list-text" use:tooltip={pinnedFile}>{pinnedFile}</span>
        <button class="set-list-del" type="button" onclick={() => void unpinPropertyFile()} aria-label="Unpin the property file">
          <Trash2 size={13} />
        </button>
      </div>
    </div>
  {:else}
    <p class="set-empty">Nothing pinned — the profile-less files answer.</p>
  {/if}
</div>

<style>
  /* An axis is a block and not a row: its list grows downward, so a label beside it would drift
     away from what it names as soon as there were three entries. */
  .axis { padding: 6px 2px; }
  .axis-head {
    font-size: 11px; font-weight: 600; color: var(--text-secondary);
    margin-bottom: 4px; letter-spacing: 0.01em;
  }
</style>
