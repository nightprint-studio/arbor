<script lang="ts">
  import { tooltip } from '$lib/actions/tooltip';
  /**
   * Generate an entity from a table — the reverse of the Entities tab. Pick a table, see the Java,
   * copy it or create the file. The package and the source root default to where the project's
   * existing entities already live, so the common case is: pick, Ctrl+Enter.
   */
  import { FilePlus, Wand2 } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CodePreview from '$lib/components/shared/ui/CodePreview.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { readFile, writeFile } from '$lib/ipc/bennu';
  import { tableDetail } from '$lib/ipc/picus/db';
  import { bennuPicusStore } from '$lib/stores/bennu/picus.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import type { TableInfo } from '$lib/types/picus';
  import { languageForPath } from '../languages';
  import { nameKey, tableNameOf } from './entity-check';
  import {
    foreignKeyAssociations,
    generateEntity,
    type JoinTweak,
    type PersistenceApi,
  } from './entity-generate';
  import { openAt } from './open-at';
  import PicusJoinOptions from './PicusJoinOptions.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const OTHER_PACKAGE = '\u0000other';

  const allTables = $derived(bennuPicusStore.tables.filter((t) => t.kind === 'table'));
  const entities = $derived(bennuPicusStore.model?.entities ?? []);
  /** The project's entity for a table, so an association points at a class that exists. */
  const entityByTable = $derived(
    new Map(entities.filter((e) => e.kind === 'entity').map((e) => [nameKey(tableNameOf(e)), e.simple])),
  );

  let onlyUnmapped = $state(true);
  const tables = $derived(
    allTables.filter((t) => !onlyUnmapped || !entityByTable.has(nameKey(t.name)) || t.name === tableName),
  );

  /** Packages the project's entities already live in, most used first — where a new one most plausibly belongs. */
  const packages = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const e of entities) {
      const pkg = e.fqcn.includes('.') ? e.fqcn.slice(0, e.fqcn.lastIndexOf('.')) : '';
      counts.set(pkg, (counts.get(pkg) ?? 0) + 1);
    }
    return [...counts.entries()].sort((a, b) => b[1] - a[1]).map(([pkg, n]) => ({ pkg, n }));
  });

  /** `jakarta.persistence` from Java 17 (Spring Boot 3 / Jakarta EE 9), `javax` before. */
  const defaultApi = $derived<PersistenceApi>(
    (() => {
      const v = projectStore.project?.jdk?.version ?? '';
      const major = Number(v.startsWith('1.') ? v.slice(2) : v);
      return major >= 17 ? 'jakarta' : 'javax';
    })(),
  );

  let tableName = $state(bennuPicusStore.generating?.table ?? '');
  let packageChoice = $state<string | null>(null);
  let customPackage = $state('');
  let apiOverride = $state<PersistenceApi | null>(null);
  let busy = $state(false);
  let associations = $state(true);
  let generatedIds = $state(true);
  let immutable = $state(false);
  let joins = $state<Record<string, Partial<JoinTweak>>>({});

  const packageName = $derived(
    packageChoice === OTHER_PACKAGE ? customPackage.trim() : (packageChoice ?? packages[0]?.pkg ?? ''),
  );
  const api = $derived(apiOverride ?? defaultApi);

  // The snapshot carries columns but no constraints; the detail adds the foreign keys the associations
  // are made from, and the column defaults the generated ids are read off.
  let detail = $state<TableInfo | null>(null);
  $effect(() => {
    const name = tableName;
    const id = bennuPicusStore.selectedId;
    detail = null;
    joins = {};
    if (!name || !id) return;
    void tableDetail(id, name).then((d) => { if (name === tableName) detail = d; }).catch(() => {});
  });
  const table = $derived(detail ?? allTables.find((t) => t.name === tableName) ?? null);
  const entityOf = (t: string) => entityByTable.get(nameKey(t)) ?? null;
  const foreignKeys = $derived(table && associations ? foreignKeyAssociations(table, entityOf) : []);

  // Lombok and fluent start from — and write back to — the Java style settings, so the last choice
  // is the next default.
  const lombok = $derived(bennuSettingsStore.entityLombok);
  const fluent = $derived(bennuSettingsStore.entityFluent);
  const generated = $derived(
    table
      ? generateEntity(table, {
          packageName, api, lombok, fluent, associations, generatedIds, immutable, joins,
          entityOfTable: entityOf,
        })
      : null,
  );
  const javaLanguage = languageForPath('Entity.java');

  /** `<source root>/<package path>/<Class>.java`. The source root is read off an existing entity,
   *  which is the only reliable way to know it (`src/main/java` is a convention, not a rule). */
  const targetPath = $derived.by(() => {
    if (!generated) return '';
    const root = projectStore.project?.root.replace(/\\/g, '/') ?? '';
    const pkgPath = packageName.replace(/\./g, '/');
    const sample = entities[0];
    let sourceRoot = `${root}/src/main/java`;
    if (sample) {
      const rel = `${sample.fqcn.replace(/\./g, '/')}.java`;
      const file = sample.file.replace(/\\/g, '/');
      if (file.endsWith(rel)) sourceRoot = file.slice(0, file.length - rel.length - 1);
    }
    return `${sourceRoot}/${pkgPath ? `${pkgPath}/` : ''}${generated.className}.java`;
  });

  async function exists(root: string, file: string): Promise<boolean> {
    try {
      await readFile(root, file);
      return true;
    } catch {
      return false;
    }
  }

  async function create() {
    const root = projectStore.project?.root;
    if (!generated || !root || busy) return;
    busy = true;
    try {
      // `writeFile` without a stamp overwrites; a generator must never do that to a class that exists.
      if (await exists(root, targetPath)) {
        toastStore.show(`${generated.className}.java already exists — not overwritten`, 'warning');
        return;
      }
      await writeFile(root, targetPath, generated.source);
      await projectStore.refreshTree();
      onClose();
      openAt(targetPath, 0);
    } catch (e) {
      toastStore.show(`Could not create the entity: ${e}`, 'error');
    } finally {
      busy = false;
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      void create();
    }
  }

  function tweakJoin(column: string, tweak: Partial<JoinTweak>) {
    joins = { ...joins, [column]: { ...joins[column], ...tweak } };
  }
</script>

<Modal {onClose} width="960px" height="640px" padBody={false} ariaLabel="Generate an entity from a table">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Wand2 size={14} />
      <span class="modal-title">Generate entity</span>
      {#if bennuPicusStore.selected}<span class="from">from <code>{bennuPicusStore.selected.name}</code></span>{/if}
    </ModalHeader>
  {/snippet}

  {#if allTables.length === 0}
    <EmptyState message="No tables loaded." description="Connect and refresh to list the tables you can generate from." />
  {:else}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="gen" onkeydown={onKeyDown}>
      <div class="left">
        <FormField label="Table">
          <Select fill searchable size="sm" ariaLabel="Table" placeholder="Choose a table…"
            value={tableName} onchange={(v) => (tableName = v)}
            options={tables.map((t) => ({ value: t.name, label: t.name }))} />
        </FormField>
        <Toggle size="sm" label="Only tables without an entity"
          description="{allTables.length - tables.length} already mapped are hidden"
          checked={onlyUnmapped} onchange={(v) => (onlyUnmapped = v)} />

        <FormField label="Package">
          <Select fill searchable size="sm" ariaLabel="Package"
            value={packageChoice ?? packages[0]?.pkg ?? OTHER_PACKAGE}
            onchange={(v) => (packageChoice = v)}
            options={[
              ...packages.map((p) => ({ value: p.pkg, label: `${p.pkg || '(default package)'} · ${p.n}` })),
              { value: OTHER_PACKAGE, label: 'Another package…' },
            ]} />
        </FormField>
        {#if packageChoice === OTHER_PACKAGE || packages.length === 0}
          <Input size="sm" value={customPackage} placeholder="com.example.model" oninput={(v) => { packageChoice = OTHER_PACKAGE; customPackage = v; }} />
        {/if}

        <FormField label="Persistence API">
          <Select fill size="sm" ariaLabel="Persistence API" value={api}
            onchange={(v) => (apiOverride = v as PersistenceApi)}
            options={[{ value: 'jakarta', label: 'jakarta.persistence' }, { value: 'javax', label: 'javax.persistence' }]} />
        </FormField>

        <div class="toggles">
          <Toggle size="sm" label="Lombok accessors" description="@Getter / @Setter instead of written methods"
            checked={lombok} onchange={(v) => bennuSettingsStore.setEntityLombok(v)} />
          <Toggle size="sm" label="Fluent accessors" description="name() / name(v) returning this"
            checked={fluent} onchange={(v) => bennuSettingsStore.setEntityFluent(v)} />
          <Toggle size="sm" label="Immutable" description="@Immutable, and no setters"
            checked={immutable} onchange={(v) => (immutable = v)} />
          <Toggle size="sm" label="Associations from foreign keys" description="@ManyToOne instead of a plain column"
            checked={associations} onchange={(v) => (associations = v)} />
          <Toggle size="sm" label="Generated ids" description="@GeneratedValue for a serial or sequence key"
            checked={generatedIds} onchange={(v) => (generatedIds = v)} />
        </div>

        <PicusJoinOptions associations={foreignKeys} {joins} onchange={tweakJoin} />
      </div>

      <div class="right">
        {#if generated}
          <div class="src"><CodePreview text={generated.source} language={javaLanguage} /></div>
          <p class="path" use:tooltip={targetPath}>{targetPath}</p>
        {:else}
          <EmptyState message="Pick a table." description="The entity is previewed here before anything is written." />
        {/if}
      </div>
    </div>
  {/if}

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="hint"><Kbd keys={['Ctrl', 'Enter']} size="sm" /> creates the file; an existing class is never overwritten.</span>
      <div class="actions">
        {#if generated}<CopyButton value={generated.source} variant="inline" label="Copy" />{/if}
        <Button variant="ghost" size="sm" onclick={onClose}>Cancel</Button>
        <Button size="sm" variant="primary" disabled={busy || !generated} onclick={() => void create()}>
          {#snippet iconStart()}<FilePlus size={13} />{/snippet}
          Create{generated ? ` ${generated.className}.java` : ''}
        </Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .from { font-size: var(--font-size-xs); color: var(--text-muted); }
  .gen { display: grid; grid-template-columns: 340px minmax(0, 1fr); height: 100%; min-height: 0; }
  .left {
    display: flex; flex-direction: column; gap: 10px; padding: 12px 14px; overflow-y: auto; min-width: 0;
    border-right: 1px solid var(--border-subtle);
  }
  .toggles { display: flex; flex-direction: column; gap: 6px; }
  .right { display: flex; flex-direction: column; gap: 6px; padding: 12px 14px; min-width: 0; min-height: 0; }
  .src { flex: 1; min-height: 0; overflow: hidden; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); }
  .path { margin: 0; font-size: 10.5px; color: var(--text-muted); font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; }
  .hint { font-size: var(--font-size-2xs); color: var(--text-muted); }
  .actions { display: flex; align-items: center; gap: 8px; }
</style>
