<script lang="ts">
  /**
   * BennuJpaGenerateModal — everything the JPA toolbar offers to write, built from the entity
   * model rather than typed.
   *
   * ## Shape
   *
   * **The kind is chosen before the dialog opens**, from the toolbar, so this is a focused form
   * with a title that names one job — not a mega-dialog with a tab strip where the first thing
   * you do is narrow it down.
   *
   * Seven bodies, one frame: the entity picker, the preview and the footer are shared, and the
   * two bodies that collect a `where` clause share the condition table too — a query and a bulk
   * update differ in what they *do* with the conditions, not in how you write them.
   *
   * ## The preview is a real editor
   *
   * `CodePreview`, read-only, with the same Java highlighting as the buffer behind the dialog.
   * It was a `<pre>` and that was wrong: generated code that is not highlighted does not read as
   * code, and the one thing a preview exists for — deciding whether what is about to be written
   * is what you meant — is exactly what flat grey text cannot support.
   *
   * Everything shown is the backend's: which entities exist, which properties each can address,
   * the keyword vocabulary, the lifecycle callbacks, and the generated text. This file collects
   * choices and renders — it knows no JPA rules, which is what keeps the two from drifting.
   *
   * **Nothing is written until the button.** A result that lands in an existing file goes through
   * the ordinary edit path, so it is undoable like any other edit.
   */
  import { Database, Plus, Minus, ChevronUp, ChevronDown, Search } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import CodePreview from '$lib/components/shared/ui/CodePreview.svelte';
  import { languageForPath } from './languages';
  import {
    jpaFormModel, jpaGenerate,
    type JpaCondition, type JpaFormModel, type JpaGenerated, type JpaGenerateRequest,
  } from '$lib/ipc/bennu/jpa';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { applyByteEdits } from './rename-apply';
  import { isSamePath } from '$lib/utils/paths';
  import { jpaActionSpec } from './jpa-actions';

  let { action, onClose }: { action: string; onClose: () => void } = $props();

  const spec = $derived(jpaActionSpec(action));
  const body = $derived(spec?.form ?? 'query');
  /** The two bodies that collect a `where` clause. */
  const hasConditions = $derived(body === 'query' || body === 'modify');
  /** Module singleton — a fresh descriptor per render would remount the preview editor. */
  const javaLang = languageForPath('Preview.java');

  let model = $state<JpaFormModel | null>(null);
  let entityFqcn = $state('');
  let repositoryFqcn = $state('');
  let busy = $state(false);

  let base = $state('JpaRepository');
  let projectionName = $state('');
  let projectionFields = $state<string[]>([]);
  let projectionNested = $state(false);
  let conditions = $state<JpaCondition[]>([]);
  let orderPath = $state('');
  let orderDesc = $state(false);
  /** Empty = the derived name. See the row that binds it. */
  let nameOverride = $state('');
  /** Write the JPQL out alongside a derived name. Forced on by a rename, which makes the name
   *  underivable — the toggle then shows on and disabled, because it is no longer a choice. */
  let withQuery = $state(false);
  /** Filter over the property picker — a real entity has forty fields. */
  let propertyFilter = $state('');

  // Attribute.
  let attrName = $state('');
  let attrType = $state('String');
  let attrRelation = $state('');
  let attrTarget = $state('');
  let attrMappedBy = $state('');
  let attrColumn = $state('');
  let attrNullable = $state(true);
  let attrUnique = $state(false);
  let attrLength = $state('');
  let attrLazy = $state(true);
  let attrAccessors = $state(true);

  // Named query / lifecycle.
  let queryName = $state('');
  let queryText = $state('');
  let lifecycleEvent = $state('');
  let callbackName = $state('');

  // Modify.
  let assignments = $state<string[]>([]);
  let returnsCount = $state(true);

  /**
   * The subject was decided by the button that opened this, not by a picker.
   *
   * Pressing *Add query method* on a repository's toolbar has already said which repository —
   * asking again is a question with one right answer, and one the user has to re-answer
   * correctly before the dialog is usable. So when the file the button came from resolves, the
   * entity and repository selects are gone and the header names the subject instead.
   *
   * They come back when it does not resolve — opening from the command palette with an
   * unrelated file in front, say — because then the dialog genuinely does not know.
   */
  let lockedTo = $state<'entity' | 'repository' | null>(null);

  const root = $derived(projectStore.project?.root ?? '');
  const entity = $derived(model?.entities.find((e) => e.fqcn === entityFqcn) ?? null);
  const properties = $derived(entity?.properties ?? []);
  const visibleProperties = $derived(
    propertyFilter.trim()
      ? properties.filter((p) => p.path.toLowerCase().includes(propertyFilter.trim().toLowerCase()))
      : properties,
  );
  /** A repository can only be generated for a real `@Entity`; an `@Embeddable` has no table.
   *  The backend sends every mapped type and says which is which — filtering is the form's job
   *  because the rule differs per action. */
  const selectableEntities = $derived(
    (model?.entities ?? []).filter((e) => body === 'attribute' || body === 'lifecycle' || e.kind === 'entity'),
  );
  /** The repositories this dialog may write into.
   *
   *  When the button came from a repository that IS the answer — matching by entity name again
   *  would let a repository whose entity the model resolved differently vanish from its own
   *  dialog, which is the failure that reads as "it says I have no repository". */
  const entityRepos = $derived.by(() => {
    const all = model?.repositories ?? [];
    if (lockedTo === 'repository') return all.filter((r) => r.fqcn === repositoryFqcn);
    return all.filter((r) => !entity || r.entity.endsWith(entity.simple));
  });

  $effect(() => {
    if (!root) return;
    void jpaFormModel(root)
      .then((m) => {
        model = m;
        if (!entityFqcn) seedFrom(m, bennuUiStore.jpaGenerateFile);
        if (!lifecycleEvent) lifecycleEvent = spec?.event || m.lifecycle[0]?.[0] || 'PrePersist';
      })
      .catch(() => {
        model = { entities: [], repositories: [], subjects: [], keywords: [], lifecycle: [], relations: [] };
      });
  });

  /**
   * Start from the file the button was pressed on — an entity, or the repository over it.
   *
   * The comparison goes through `isSamePath`, and that is not defensive tidiness: the backend
   * returns forward-slashed paths and the editor's own `activeFilePath` carries native ones, so
   * a plain `===` was false for the same file on Windows — every dialog fell through to
   * "whichever entity sorts first", which is how a repository's own button opened on an
   * unrelated view with no repository at all.
   */
  function seedFrom(m: JpaFormModel, file: string | null) {
    const repo = file ? m.repositories.find((r) => isSamePath(r.file, file)) : undefined;
    if (repo) {
      entityFqcn = m.entities.find((e) => repo.entity.endsWith(e.simple))?.fqcn
        ?? m.entities[0]?.fqcn ?? '';
      repositoryFqcn = repo.fqcn;
      lockedTo = 'repository';
      return;
    }
    const own = file ? m.entities.find((e) => isSamePath(e.file, file)) : undefined;
    if (own) {
      entityFqcn = own.fqcn;
      lockedTo = 'entity';
      return;
    }
    entityFqcn = m.entities[0]?.fqcn ?? '';
    lockedTo = null;
  }

  $effect(() => {
    const e = entity;
    if (!e) return;
    projectionName = `${e.simple}Summary`;
    projectionFields = [];
    conditions = [];
    assignments = [];
    orderPath = '';
    propertyFilter = '';
    // A name written for one entity means nothing on another.
    nameOverride = '';
    withQuery = false;
    queryName = '';
    queryText = '';
    if (!entityRepos.some((r) => r.fqcn === repositoryFqcn)) {
      repositoryFqcn = entityRepos[0]?.fqcn ?? '';
    }
  });

  // ── Condition rows ──────────────────────────────────────────────────────────
  let selected = $state(-1);

  function addCondition() {
    conditions = [
      ...conditions,
      { path: properties[0]?.path ?? '', keyword: '', ignore_case: false, or: false },
    ];
    selected = conditions.length - 1;
  }
  function removeSelected() {
    if (selected < 0) return;
    conditions = conditions.filter((_, i) => i !== selected);
    selected = Math.min(selected, conditions.length - 1);
  }
  function move(delta: number) {
    const to = selected + delta;
    if (selected < 0 || to < 0 || to >= conditions.length) return;
    const next = [...conditions];
    [next[selected], next[to]] = [next[to], next[selected]];
    conditions = next;
    selected = to;
  }
  function patch(i: number, change: Partial<JpaCondition>) {
    conditions = conditions.map((c, at) => (at === i ? { ...c, ...change } : c));
  }
  function toggle(list: string[], path: string, on: boolean): string[] {
    return on ? [...list, path] : list.filter((f) => f !== path);
  }

  // ── Live preview ────────────────────────────────────────────────────────────
  let result = $state<JpaGenerated | null>(null);
  let error = $state('');

  /** The buffer the insertion targets, when it happens to be open. Sent so the offset is
   *  computed against the text the user can see rather than a stale copy on disk. */
  const openBuffer = $derived.by((): [string, string] | undefined => {
    const path = projectStore.activeFilePath;
    return path ? [path, projectStore.sourceOf(path)] : undefined;
  });

  const request = $derived.by((): JpaGenerateRequest | null => {
    const s = spec;
    if (!s || !root || !entityFqcn) return null;
    const common = { root, entity: entityFqcn, open: openBuffer };

    if (s.form === 'repository') return { ...common, kind: 'repository', base };
    if (s.form === 'projection') {
      return {
        ...common,
        kind: 'projection',
        name: projectionName,
        fields: projectionFields,
        repository: projectionNested ? repositoryFqcn : undefined,
      };
    }
    if (s.form === 'attribute') {
      if (!attrName.trim()) return null;
      return {
        ...common,
        kind: 'attribute',
        attribute: {
          name: attrName.trim(),
          type_text: attrRelation ? attrTarget : attrType,
          column: attrColumn.trim(),
          optional: attrNullable,
          unique: attrUnique,
          length: attrLength.trim() ? Number(attrLength) : null,
          relation: attrRelation,
          mapped_by: attrMappedBy.trim(),
          lazy: attrLazy,
          accessors: attrAccessors,
        },
      };
    }
    if (s.form === 'named-query') {
      if (!queryName.trim()) return null;
      return { ...common, kind: 'named-query', name: queryName.trim(), text: queryText };
    }
    if (s.form === 'lifecycle') {
      return { ...common, kind: 'lifecycle', event: lifecycleEvent, name: callbackName.trim() };
    }
    if (!repositoryFqcn) return null;
    if (s.form === 'modify') {
      if (!s.delete && assignments.length === 0) return null;
      return {
        ...common,
        kind: 'modify-method',
        repository: repositoryFqcn,
        modify: {
          name: nameOverride.trim(),
          delete: s.delete,
          assignments,
          conditions,
          returns_count: returnsCount,
        },
      };
    }
    return {
      ...common,
      kind: 'query-method',
      repository: repositoryFqcn,
      query: {
        name: nameOverride.trim(),
        with_query: withQuery,
        subject: s.subject, distinct: s.distinct, limit: null, conditions,
        order_by: orderPath ? ([[orderPath, orderDesc ? 'desc' : 'asc']] as [string, string][]) : [],
        many: s.many, paged: s.paged, projection: '',
      },
    };
  });

  $effect(() => {
    const req = request;
    if (!req) { result = null; return; }
    let cancelled = false;
    const t = setTimeout(() => {
      void jpaGenerate(req)
        .then((r) => { if (!cancelled) { result = r; error = ''; } })
        .catch((e) => { if (!cancelled) { result = null; error = String(e); } });
    }, 120);
    return () => { cancelled = true; clearTimeout(t); };
  });

  const writesFile = $derived(!!result?.file && !(body === 'projection' && projectionNested));
  const destination = $derived(
    writesFile && result?.file
      ? `new file · ${result.file[0].split('/').slice(-3).join('/')}`
      : result?.insertion
        ? `into ${result.insertion.file.split(/[\\/]/).pop()}`
        : '',
  );

  async function generate() {
    const r = result;
    if (!r || busy) return;
    busy = true;
    try {
      if (writesFile && r.file) {
        const [path, content] = r.file;
        await projectStore.saveText(path, content);
        await projectStore.openFile(path);
        projectStore.refreshTree();
        bennuUiStore.revealActiveInTree();
        toastStore.show(`Created ${path.split('/').pop()}`, 'success');
      } else if (r.insertion) {
        await projectStore.openFile(r.insertion.file);
        const current = projectStore.sourceOf(r.insertion.file);
        const next = applyByteEdits(current, [
          { start: r.insertion.offset, end: r.insertion.offset, text: r.insertion.text },
        ]);
        await projectStore.saveText(r.insertion.file, next);
        await projectStore.openFile(r.insertion.file);
        toastStore.show('Added', 'success');
      } else {
        toastStore.show('Nothing to generate', 'info');
        return;
      }
      onClose();
    } catch {
      toastStore.show('Could not generate', 'error');
    } finally {
      busy = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); void generate(); }
    // The reference's own binding for adding a row, and the one that makes the table usable
    // without reaching for the mouse.
    if (e.altKey && e.key === 'Insert' && hasConditions) {
      e.preventDefault();
      addCondition();
    }
  }

  const entityOptions = $derived(selectableEntities.map((e) => ({ value: e.fqcn, label: e.simple })));
  const propertyOptions = $derived(properties.map((p) => ({ value: p.path, label: p.path })));
  const keywordOptions = $derived(
    (model?.keywords ?? []).map((k) => ({ value: k.keyword, label: k.label })),
  );

  /**
   * The parameter a condition binds — the answer to "not equal to *what*".
   *
   * The row used to say the attribute and the operator and stop there, which is half a sentence:
   * a comparison has a right-hand side, and it is the thing the caller will actually pass. The
   * backend sends the arity and whether the argument is a collection precisely so this can be
   * shown without the frontend inventing rules; the generator remains the authority on the
   * names, and this renders them.
   */
  function parameterOf(c: JpaCondition): string {
    const k = model?.keywords.find((x) => x.keyword === c.keyword);
    const leaf = c.path.split('.').pop() ?? c.path;
    if (!k || k.args === 0) return '—';
    if (k.args === 2) return `${leaf}From, ${leaf}To`;
    return k.collection ? `${leaf}s` : leaf;
  }
  const repoOptions = $derived(entityRepos.map((r) => ({ value: r.fqcn, label: r.simple })));
  const lifecycleOptions = $derived(
    (model?.lifecycle ?? []).map(([value, when]) => ({ value, label: `@${value} — ${when}` })),
  );
  const relationOptions = $derived([
    { value: '', label: 'Plain column' },
    ...(model?.relations ?? []).map((r) => ({ value: r, label: `@${r}` })),
  ]);
  /** Every mapped type is a legal relation target, `@Embeddable` included. */
  const targetOptions = $derived((model?.entities ?? []).map((e) => ({ value: e.simple, label: e.simple })));
  /** The method name the backend will emit — derived, so it is shown rather than typed. */
  const methodName = $derived(result?.preview.match(/\b(\w+)\s*\(/)?.[1] ?? '');
  /** Only the to-many sides take a `mappedBy`; on a `@ManyToOne` it is not a thing that exists. */
  const canMapBy = $derived(['OneToMany', 'ManyToMany', 'OneToOne'].includes(attrRelation));
  const title = $derived(spec?.title ?? 'Generate');
  /** What the header names, and the reason the pickers below can disappear. */
  const subject = $derived(
    lockedTo === 'repository'
      ? (entityRepos.find((r) => r.fqcn === repositoryFqcn)?.simple ?? entity?.simple ?? '')
      : (entity?.simple ?? ''),
  );
  /** Whether the entity is still a choice. It is not when the button that opened this already
   *  named one — or named a repository, which names one. */
  const asksForEntity = $derived(lockedTo === null);
  /** Whether the repository is still a choice: only when the dialog needs one and the file it
   *  came from did not supply it. */
  const asksForRepository = $derived(lockedTo !== 'repository');
  /** The picker's own label, so one block serves both the projection and the update. */
  const pickerTitle = $derived(body === 'modify' ? 'Set' : 'Expose');
  const pickerHint = $derived(
    body === 'modify' ? 'Each becomes one bound parameter' : 'Each property becomes one getter',
  );
  const picked = $derived(body === 'modify' ? assignments : projectionFields);
</script>

<Modal {onClose} width="920px" height="620px" padBody={false} ariaLabel={title}>
  {#snippet header()}
    <ModalHeader {onClose}>
      <Database size={14} />
      <span class="modal-title">{title}</span>
      {#if subject}
        <Badge variant="tone" tone="neutral" size="sm" label={subject} />
      {/if}
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="jg" onkeydown={onKeydown}>
    {#if !spec}
      <EmptyState
        title="Unknown action"
        description="This build of the UI does not know what to open for it."
      />
    {:else if model && selectableEntities.length === 0}
      <EmptyState
        title="No entities in this project"
        description="Nothing here can be generated without an @Entity class to generate it for."
      />
    {:else}
      <div class="jg-split">
      <div class="jg-form">
        {#if asksForEntity}
          <FormRow label="Entity" wideControl>
            <Select value={entityFqcn} options={entityOptions} onchange={(v) => (entityFqcn = v)} />
          </FormRow>
        {/if}

        {#if body === 'repository'}
          <FormRow label="Extends" wideControl>
            <Select
              value={base}
              options={[
                { value: 'JpaRepository', label: 'JpaRepository' },
                { value: 'CrudRepository', label: 'CrudRepository' },
                { value: 'PagingAndSortingRepository', label: 'PagingAndSortingRepository' },
              ]}
              onchange={(v) => (base = v)}
            />
          </FormRow>
          {#if entity?.has_repository}
            <Alert
              variant="info"
              compact
              text="A repository already manages this entity. A second one is legal — a read-only or projection-only repository beside the main one — but check that is what you meant."
            />
          {/if}

        {:else if body === 'projection'}
          <FormRow label="Name" wideControl>
            <Input value={projectionName} oninput={(v) => (projectionName = v)} />
          </FormRow>
          <FormRow label="Where" wideControl>
            <Select
              value={projectionNested ? 'nested' : 'file'}
              options={[
                { value: 'file', label: 'Its own file' },
                { value: 'nested', label: 'Nested in the repository' },
              ]}
              disabled={entityRepos.length === 0}
              onchange={(v) => (projectionNested = v === 'nested')}
            />
          </FormRow>
          {#if projectionNested}
            <FormRow label="Repository" wideControl>
              <Select value={repositoryFqcn} options={repoOptions} onchange={(v) => (repositoryFqcn = v)} />
            </FormRow>
          {/if}

        {:else if body === 'attribute'}
          <FormRow label="Name" wideControl>
            <Input value={attrName} placeholder="createdAt" oninput={(v) => (attrName = v)} />
          </FormRow>
          <FormRow label="Mapping" wideControl>
            <Select value={attrRelation} options={relationOptions} onchange={(v) => (attrRelation = v)} />
          </FormRow>
          {#if attrRelation}
            <FormRow label="Target" wideControl>
              <Select value={attrTarget} options={targetOptions} onchange={(v) => (attrTarget = v)} />
            </FormRow>
            {#if canMapBy}
              <FormRow
                label="Mapped by"
                wideControl
                description="Empty means this side owns the foreign key and gets the @JoinColumn. Filled in, it is the inverse side and owns no column at all."
              >
                <Input
                  value={attrMappedBy}
                  placeholder="field on the other side"
                  oninput={(v) => (attrMappedBy = v)}
                />
              </FormRow>
            {/if}
            <FormRow label="Fetch lazily">
              <Toggle bind:checked={attrLazy} size="sm" ariaLabel="Fetch lazily" />
            </FormRow>
          {:else}
            <FormRow label="Type" wideControl>
              <Input value={attrType} placeholder="String" oninput={(v) => (attrType = v)} />
            </FormRow>
            <FormRow label="Column" wideControl>
              <Input
                value={attrColumn}
                placeholder="(the provider's default naming)"
                oninput={(v) => (attrColumn = v)}
              />
            </FormRow>
            <FormRow label="Max length" wideControl>
              <Input value={attrLength} placeholder="(unset)" oninput={(v) => (attrLength = v)} />
            </FormRow>
            <FormRow label="Nullable">
              <Toggle bind:checked={attrNullable} size="sm" ariaLabel="Nullable" />
            </FormRow>
            <FormRow label="Unique">
              <Toggle bind:checked={attrUnique} size="sm" ariaLabel="Unique" />
            </FormRow>
          {/if}
          <FormRow label="Getter and setter">
            <Toggle bind:checked={attrAccessors} size="sm" ariaLabel="Write a getter and a setter" />
          </FormRow>

        {:else if body === 'named-query'}
          <FormRow
            label="Name"
            wideControl
            description={entity ? `Registered as ${entity.simple}.${queryName || '…'}` : undefined}
          >
            <Input value={queryName} placeholder="findOpen" oninput={(v) => (queryName = v)} />
          </FormRow>
          <div class="jg-area">
            <span class="jg-area-label">Query</span>
            <textarea
              class="jg-textarea"
              value={queryText}
              placeholder={entity ? `select e from ${entity.simple} e where …` : 'select …'}
              spellcheck="false"
              oninput={(e) => (queryText = e.currentTarget.value)}
            ></textarea>
          </div>
          <p class="jg-note">
            Left empty, a <code>select</code> over the whole entity is written as a starting point.
          </p>

        {:else if body === 'lifecycle'}
          <FormRow label="Event" wideControl>
            <Select value={lifecycleEvent} options={lifecycleOptions} onchange={(v) => (lifecycleEvent = v)} />
          </FormRow>
          <FormRow
            label="Method"
            wideControl
            description="The annotation is what wires it, so the default name says which event it serves."
          >
            <Input
              value={callbackName}
              placeholder={`on${lifecycleEvent}`}
              oninput={(v) => (callbackName = v)}
            />
          </FormRow>

        {:else if entityRepos.length === 0}
          <Alert
            variant="warning"
            compact
            text="This entity has no repository yet — generate one first, and the method has somewhere to live."
          />
        {:else}
          {#if asksForRepository}
            <FormRow label="Repository" wideControl>
              <Select value={repositoryFqcn} options={repoOptions} onchange={(v) => (repositoryFqcn = v)} />
            </FormRow>
          {/if}
          <FormRow
            label="Method name"
            wideControl
            description={nameOverride.trim() && body === 'query'
              ? 'A name Spring Data cannot parse is no longer a derived query, so the method arrives with its @Query written out.'
              : undefined}
          >
            <Input
              value={nameOverride}
              placeholder={methodName || 'Derived from the conditions below'}
              oninput={(v) => (nameOverride = v)}
            />
          </FormRow>
          {#if body === 'query'}
            <FormRow
              label="Write the @Query too"
              description="A derived name is re-checked against the entity every time the project opens; an explicit query is a string nobody verifies until it runs. Ask for it when the generated JPQL is a starting point you mean to edit — a join, a fetch, a projection the name cannot express."
            >
              <Toggle
                checked={withQuery || !!nameOverride.trim()}
                disabled={!!nameOverride.trim()}
                size="sm"
                ariaLabel="Write the @Query too"
                onchange={(v) => (withQuery = v)}
              />
            </FormRow>
          {/if}
        {/if}

        <!-- The property picker, shared by the projection and the bulk update: both are
             "choose some of this entity's properties", and both are unusable without a filter
             on an entity with forty fields. -->
        {#if body === 'projection' || (body === 'modify' && spec && !spec.delete)}
          <section class="jg-block">
            <header class="jg-block-head">
              <span class="jg-block-title">{pickerTitle}</span>
              {#if picked.length > 0}
                <Badge variant="count" label={String(picked.length)} />
              {/if}
              <span class="jg-block-hint">{pickerHint}</span>
              <div class="jg-filter">
                <Search size={11} />
                <input
                  type="text"
                  placeholder="Filter"
                  value={propertyFilter}
                  aria-label="Filter properties"
                  oninput={(e) => (propertyFilter = e.currentTarget.value)}
                />
              </div>
            </header>
            <div class="jg-rows">
              {#each visibleProperties as p (p.path)}
                <label class="jg-check">
                  <input
                    type="checkbox"
                    checked={picked.includes(p.path)}
                    onchange={(e) => {
                      const on = e.currentTarget.checked;
                      if (body === 'modify') assignments = toggle(assignments, p.path, on);
                      else projectionFields = toggle(projectionFields, p.path, on);
                    }}
                  />
                  <span class="jg-path">{p.path}</span>
                  <span class="jg-type">{p.type_text}</span>
                </label>
              {:else}
                <p class="jg-note jg-pad">No property matches <code>{propertyFilter}</code>.</p>
              {/each}
            </div>
          </section>
        {/if}

        {#if hasConditions && entityRepos.length > 0}
          <section class="jg-block">
            <header class="jg-block-head">
              <span class="jg-block-title">
                {body === 'modify' ? 'Affects rows where' : 'Conditions'}
              </span>
              {#if conditions.length > 0}
                <Badge variant="count" label={String(conditions.length)} />
              {/if}
              <div class="jg-tools">
                <IconButton tooltip="Add condition" shortcut="Alt+Ins" size={22} variant="accent" onclick={addCondition}>
                  <Plus size={13} />
                </IconButton>
                <IconButton tooltip="Remove" size={22} disabled={selected < 0} onclick={removeSelected}>
                  <Minus size={13} />
                </IconButton>
                <IconButton tooltip="Move up" size={22} disabled={selected <= 0} onclick={() => move(-1)}>
                  <ChevronUp size={13} />
                </IconButton>
                <IconButton
                  tooltip="Move down"
                  size={22}
                  disabled={selected < 0 || selected >= conditions.length - 1}
                  onclick={() => move(1)}
                >
                  <ChevronDown size={13} />
                </IconButton>
              </div>
            </header>
            {#if conditions.length > 0}
              <div class="jg-cols">
                <span>Join</span><span>Attribute</span><span>Condition</span>
                <span>Parameter</span><span>Aa</span>
              </div>
            {/if}
            <div class="jg-rows">
              {#each conditions as c, i (i)}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <div
                  class="jg-cond"
                  class:sel={selected === i}
                  role="row"
                  tabindex="0"
                  onfocus={() => (selected = i)}
                  onclick={() => (selected = i)}
                >
                  {#if i === 0}
                    <span class="jg-fixed">where</span>
                  {:else}
                    <Select
                      value={c.or ? 'or' : 'and'}
                      options={[{ value: 'and', label: 'and' }, { value: 'or', label: 'or' }]}
                      narrow
                      onchange={(v) => patch(i, { or: v === 'or' })}
                    />
                  {/if}
                  <Select value={c.path} options={propertyOptions} onchange={(v) => patch(i, { path: v })} />
                  <Select value={c.keyword} options={keywordOptions} onchange={(v) => patch(i, { keyword: v })} />
                  <span class="jg-param" class:jg-none={parameterOf(c) === '—'}>{parameterOf(c)}</span>
                  <Toggle
                    checked={c.ignore_case}
                    size="sm"
                    ariaLabel="Ignore case"
                    onchange={(v) => patch(i, { ignore_case: v })}
                  />
                </div>
              {:else}
                <button type="button" class="jg-empty" onclick={addCondition}>
                  <Plus size={12} /> Add a condition <span class="jg-kbd">Alt+Ins</span>
                </button>
              {/each}
            </div>
          </section>

          {#if body === 'modify'}
            <FormRow label="Return the number of rows affected">
              <Toggle bind:checked={returnsCount} size="sm" ariaLabel="Return the number of rows affected" />
            </FormRow>
            {#if conditions.length === 0 && spec}
              <Alert
                variant="warning"
                compact
                text={`With no conditions this ${spec.delete ? 'deletes' : 'updates'} every row of the table.`}
              />
            {/if}
            <Alert variant="info" compact>
              A bulk write goes straight to the database: it does not load the rows, so
              <code>@PreUpdate</code> / <code>@PreRemove</code> do not fire and the persistence
              context does not see it. Callers need a transaction of their own.
            </Alert>
          {:else}
            <FormRow label="Ordered by" wideControl>
              <div class="jg-order">
                <Select
                  value={orderPath}
                  options={[{ value: '', label: '(unordered)' }, ...propertyOptions]}
                  onchange={(v) => (orderPath = v)}
                />
                {#if orderPath}
                  <Select
                    value={orderDesc ? 'desc' : 'asc'}
                    options={[{ value: 'asc', label: 'ascending' }, { value: 'desc', label: 'descending' }]}
                    narrow
                    onchange={(v) => (orderDesc = v === 'desc')}
                  />
                {/if}
              </div>
            </FormRow>
          {/if}
        {/if}
      </div>

      <!-- The preview beside the form, not under it — the same arrangement as Find in files,
           for the same reason: what you are building and what it produces are read together,
           and stacking them means the answer is always the part scrolled off. -->
      <div class="jg-preview">
        <CodePreview
          code={result?.preview ?? ''}
          language={javaLang}
          title="Preview"
          detail={destination}
          error={error || null}
          fill
          empty="Fill the form and the generated code appears here."
        />
      </div>
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter>
      <Button variant="ghost" onclick={onClose}>Cancel</Button>
      <Button
        variant="primary"
        disabled={!result || busy}
        shortcut="Ctrl+Enter"
        onclick={() => void generate()}
      >
        {writesFile ? 'Create file' : 'Add'}
      </Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .jg { display: flex; flex-direction: column; height: 100%; min-height: 0; }

  /* Two columns: what you are building, and what it produces. */
  .jg-split {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 5fr) minmax(0, 6fr);
    gap: 12px;
    padding: 10px 14px 12px;
  }

  .jg-form {
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-right: 2px;
    overflow-y: auto;
  }
  /* FormRow's control column is sized for a settings page; in a dense dialog it should take
     the width it is given rather than hugging a chevron. */
  .jg-form :global(.fr-row) { padding: 4px 0; }
  .jg-form :global(.fr-control .select),
  .jg-form :global(.fr-control .input) { width: 100%; }

  .jg-preview {
    min-height: 0;
    display: flex;
  }
  .jg-preview > :global(*) { flex: 1; min-width: 0; }

  /* ── A titled block: the property picker and the condition table ───────────── */
  .jg-block {
    display: flex;
    flex-direction: column;
    min-height: 0;
    margin: 6px 0;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
    overflow: hidden;
  }
  .jg-block-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 6px 5px 10px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
  }
  .jg-block-title {
    font-size: var(--font-size-xs);
    font-weight: 600;
    color: var(--text-secondary);
  }
  .jg-block-hint {
    font-size: 11px;
    color: var(--text-disabled);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .jg-tools { margin-left: auto; display: flex; gap: 1px; }

  .jg-filter {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 2px 7px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
    color: var(--text-disabled);
  }
  .jg-filter input {
    width: 96px;
    border: none;
    background: none;
    color: var(--text-primary);
    font-size: 11px;
    outline: none;
  }

  /* Tall enough to show a real entity's fields without scrolling twice — the column scrolls,
     so a list that ends early only wastes the space above the fold. */
  .jg-rows { max-height: 260px; overflow-y: auto; }

  .jg-cols, .jg-cond {
    display: grid;
    grid-template-columns: 66px minmax(0, 1fr) minmax(0, 1fr) minmax(0, 0.8fr) 34px;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
  }
  /* What the caller will pass — the right-hand side of the comparison, which is the half the
     row used to leave out. */
  .jg-param {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--syntax-parameter, var(--text-secondary));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .jg-none { color: var(--text-disabled); font-family: inherit; }
  .jg-cols {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-disabled);
    border-bottom: 1px solid var(--border-subtle);
  }
  .jg-cond { cursor: default; }
  .jg-cond:hover { background: var(--bg-hover); }
  /* A left bar rather than a full tint: the row holds three controls, and washing all of them
     in accent makes the selected row the hardest one to read. */
  .jg-cond.sel {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .jg-fixed { font-size: 11px; color: var(--text-muted); padding-left: 4px; white-space: nowrap; }

  .jg-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    width: 100%;
    padding: 16px;
    background: none;
    border: none;
    font-size: 12px;
    color: var(--accent);
    cursor: pointer;
  }
  .jg-empty:hover { background: var(--bg-hover); }
  .jg-kbd { color: var(--text-disabled); font-family: var(--font-code); font-size: 11px; }

  .jg-check {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 10px;
    cursor: pointer;
  }
  .jg-check:hover { background: var(--bg-hover); }
  .jg-path { font-family: var(--font-code); font-size: 12px; }
  .jg-type { margin-left: auto; font-size: 11px; color: var(--text-muted); }

  .jg-order { display: flex; gap: 6px; width: 100%; }
  .jg-order :global(.select:first-child) { flex: 1; min-width: 0; }

  .jg-area { display: flex; gap: 10px; padding: 6px 0; }
  .jg-area-label {
    flex: 0 0 auto;
    padding-top: 6px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
  .jg-textarea {
    flex: 1;
    min-width: 0;
    min-height: 84px;
    resize: vertical;
    padding: 6px 8px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 12px;
    line-height: 1.5;
  }
  .jg-textarea:focus { outline: none; border-color: var(--accent); }

  .jg-note {
    margin: 0;
    padding: 2px 0 6px;
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-muted);
  }
  .jg-pad { padding: 10px; }
</style>
