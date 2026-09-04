<script lang="ts">
  /**
   * BennuJpaGenerateModal — everything the JPA toolbar offers to write, built from the entity
   * model rather than typed.
   *
   * ## Shape
   *
   * **The kind is chosen before the dialog opens**, from the toolbar, so this is a focused form
   * with a title that names one job — not a mega-dialog whose first step is narrowing it down.
   * Seven bodies, one frame: the header, the preview column and the footer are shared, and the two
   * bodies that collect a `where` clause share the condition rows too.
   *
   * What the button decided is a **starting point, not a cage**: pressing *Create paged query*
   * opens the form on `Page`, and the return-type row is right there if it turns out you wanted a
   * `Slice`. The alternative — one form per shape, no way across — is how `Slice` and `Stream`
   * ended up unreachable in the first place.
   *
   * ## Read left to right
   *
   * The form is a column of labelled bands, and the preview is a column beside it. Same
   * arrangement as Find in files, for the same reason: what you are building and what it produces
   * are read together, and stacking them means the answer is always the part scrolled off.
   *
   * The preview is a real read-only editor with the buffer's own highlighting — generated code
   * that is not highlighted does not read as code, and deciding "is this what I meant" is the only
   * thing a preview is for. Where a change means more than Java, the pane carries tabs: an
   * attribute shows the column it implies next to the field it writes.
   *
   * Everything shown is the backend's: which entities exist, which properties each can address,
   * the keyword vocabulary, the callbacks, the generated text and the DDL. This file collects
   * choices and renders — it knows no JPA rules, which is what keeps the two from drifting.
   *
   * **Nothing is written until the button.** A result that lands in an existing file goes through
   * the ordinary edit path, so it is undoable like any other edit.
   */
  import { tooltip } from '$lib/actions/tooltip';
  import {
    Database, Plus, Trash2, Search, Type, ListChecks, Box, FileText, ChevronUp, ChevronDown,
    ArrowRight, ArrowLeft, ArrowLeftRight, Link2,
  } from 'lucide-svelte';
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
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import ChipBar from '$lib/components/shared/ui/ChipBar.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import CodePreview from '$lib/components/shared/ui/CodePreview.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import { languageForPath } from './languages';
  import {
    jpaFormModel, jpaGenerate, JPA_CASCADE_TYPES, JPA_RETURN_SHAPES, JPA_VALIDATIONS,
    type JpaAttributeKind, type JpaCondition, type JpaFormModel, type JpaGenerated,
    type JpaGenerateRequest, type JpaReturnShape,
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
  /** Module singletons — a fresh descriptor per render would remount the preview editor. */
  const javaLang = languageForPath('Preview.java');
  const sqlLang = languageForPath('preview.sql');

  let model = $state<JpaFormModel | null>(null);
  let entityFqcn = $state('');
  let repositoryFqcn = $state('');
  let busy = $state(false);

  let base = $state('JpaRepository');
  let projectionName = $state('');
  let projectionFields = $state<string[]>([]);
  let projectionNested = $state(false);
  let conditions = $state<JpaCondition[]>([]);
  let orders = $state<{ path: string; desc: boolean }[]>([]);
  /** Empty = the derived name. See the row that binds it. */
  let nameOverride = $state('');
  /** Write the JPQL out alongside a derived name. Forced on by a rename, which makes the name
   *  underivable — the toggle then shows on and disabled, because it is no longer a choice. */
  let withQuery = $state(false);
  /** Filter over the property picker — a real entity has forty fields. */
  let propertyFilter = $state('');

  // Query shape. Seeded from the action that opened the dialog and changeable from here: the
  // button says where to start, it does not decide for you.
  let subject = $state('find');
  let returns = $state<JpaReturnShape>('optional');
  let sorted = $state(false);
  let distinct = $state(false);
  let limit = $state('');
  let projectionType = $state('');

  // Attribute.
  let attrName = $state('');
  let attrType = $state('String');
  let attrKind = $state<JpaAttributeKind>('base');
  let attrRelation = $state('');
  let attrTarget = $state('');
  let attrCollection = $state('Set');
  let attrMappedBy = $state('');
  let attrColumn = $state('');
  let attrNullable = $state(true);
  let attrUnique = $state(false);
  let attrLength = $state('');
  let attrDefault = $state('');
  let attrValidation = $state<string[]>([]);
  let attrCascade = $state<string[]>([]);
  let attrOrphan = $state(false);
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
   * asking again is a question with one right answer, and one the user has to re-answer correctly
   * before the dialog is usable. So when the file the button came from resolves, the entity and
   * repository selects are gone and the header names the subject instead.
   *
   * They come back when it does not resolve — opening from the command palette with an unrelated
   * file in front, say — because then the dialog genuinely does not know.
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
  /** A repository can only be generated for a real `@Entity`; an `@Embeddable` has no table. The
   *  backend sends every mapped type and says which is which — filtering is the form's job
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

  // The shape the action opened on. Untracked afterwards — this seeds the form once and then gets
  // out of the way, so changing the return type does not fight the button that opened the dialog.
  $effect(() => {
    const s = spec;
    if (!s) return;
    subject = s.subject;
    returns = s.returns;
    distinct = s.distinct;
  });

  /**
   * Start from the file the button was pressed on — an entity, or the repository over it.
   *
   * The comparison goes through `isSamePath`, and that is not defensive tidiness: the backend
   * returns forward-slashed paths and the editor's own `activeFilePath` carries native ones, so a
   * plain `===` was false for the same file on Windows — every dialog fell through to "whichever
   * entity sorts first", which is how a repository's own button opened on an unrelated view.
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
    resetSubjectFields();
    if (!entityRepos.some((r) => r.fqcn === repositoryFqcn)) {
      repositoryFqcn = entityRepos[0]?.fqcn ?? '';
    }
  });

  /** Everything that describes *this one thing* rather than the entity it belongs to. Cleared on
   *  an entity change (a name written for one entity means nothing on another) and after *Add and
   *  continue*, which is the same situation: same target, next thing. */
  function resetSubjectFields() {
    projectionFields = [];
    conditions = [];
    orders = [];
    assignments = [];
    propertyFilter = '';
    nameOverride = '';
    withQuery = false;
    queryName = '';
    queryText = '';
    limit = '';
    attrName = '';
    attrColumn = '';
    attrDefault = '';
    attrValidation = [];
    attrMappedBy = '';
  }

  // ── Condition rows ──────────────────────────────────────────────────────────

  function addCondition() {
    conditions = [
      ...conditions,
      { path: properties[0]?.path ?? '', keyword: '', ignore_case: false, or: false },
    ];
  }
  function removeCondition(i: number) {
    conditions = conditions.filter((_, at) => at !== i);
  }
  /** Swap a condition with its neighbour. Spring Data evaluates left to right, so this changes the
   *  query rather than only how it reads. */
  function moveCondition(i: number, delta: number) {
    const to = i + delta;
    if (to < 0 || to >= conditions.length) return;
    const next = [...conditions];
    [next[i], next[to]] = [next[to], next[i]];
    conditions = next;
  }
  function patch(i: number, change: Partial<JpaCondition>) {
    conditions = conditions.map((c, at) => (at === i ? { ...c, ...change } : c));
  }
  function addOrder() {
    orders = [...orders, { path: properties[0]?.path ?? '', desc: false }];
  }
  function patchOrder(i: number, change: Partial<{ path: string; desc: boolean }>) {
    orders = orders.map((o, at) => (at === i ? { ...o, ...change } : o));
  }
  function toggle(list: string[], path: string, on: boolean): string[] {
    return on ? [...list, path] : list.filter((f) => f !== path);
  }

  // ── Live preview ────────────────────────────────────────────────────────────
  let result = $state<JpaGenerated | null>(null);
  let error = $state('');

  /** The buffer the insertion targets, when it happens to be open. Sent so the offset is computed
   *  against the text the user can see rather than a stale copy on disk. */
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
          kind: attrRelation ? 'base' : attrKind,
          column: attrColumn.trim(),
          optional: attrNullable,
          unique: attrUnique,
          length: attrLength.trim() ? Number(attrLength) : null,
          default_value: attrDefault.trim(),
          validation: attrValidation,
          relation: attrRelation,
          collection: attrCollection,
          mapped_by: attrMappedBy.trim(),
          lazy: attrLazy,
          cascade: attrCascade,
          orphan_removal: attrOrphan,
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
        subject,
        distinct,
        limit: limit.trim() ? Number(limit) : null,
        conditions,
        order_by: orders
          .filter((o) => o.path)
          .map((o) => [o.path, o.desc ? 'desc' : 'asc'] as [string, string]),
        returns,
        sorted,
        projection: projectionType.trim(),
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
  /** One pane of the preview: the generated text, how to colour it, and what it is for.
   *
   *  Local to this modal on purpose. `CodePreview` is the shared **excerpt viewer** — a slice of a
   *  file with a banded line — and three other consumers use it exactly that way. What this modal
   *  wants is a *generated-output* pane with tabs and a caption, which is a different widget, so it
   *  is composed here out of `Tabs` + `CodePreview` rather than by widening the shared one until it
   *  serves both. */
  interface PreviewPane {
    id: string;
    label: string;
    code: string;
    language: ReturnType<typeof languageForPath>;
    detail?: string;
  }

  /** The pane's tabs. One view unless there is genuinely a second — the column a field implies is
   *  a different question from the field, and reading them side by side is the point. */
  const previewTabs = $derived.by((): PreviewPane[] | undefined => {
    if (!result?.ddl) return undefined;
    return [
      { id: 'java', label: 'Java', code: result.preview, language: javaLang, detail: destination },
      {
        id: 'ddl',
        label: 'DDL',
        code: result.ddl,
        language: sqlLang,
        detail: 'a starting point — no dialect, no back-fill',
      },
    ];
  });

  /** Which pane is showing. Reset whenever the panes change, so a DDL tab does not survive a
   *  regeneration that no longer produces one. */
  let previewTab = $state('java');
  const shownPane = $derived(
    previewTabs?.find((t) => t.id === previewTab) ?? previewTabs?.[0],
  );

  async function apply(): Promise<boolean> {
    const r = result;
    if (!r || busy) return false;
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
          { start: r.insertion.offset, end: r.insertion.offset, new_text: r.insertion.text },
        ]);
        await projectStore.saveText(r.insertion.file, next);
        await projectStore.openFile(r.insertion.file);
        toastStore.show('Added', 'success');
      } else {
        toastStore.show('Nothing to generate', 'info');
        return false;
      }
      return true;
    } catch {
      toastStore.show('Could not generate', 'error');
      return false;
    } finally {
      busy = false;
    }
  }

  async function generate() {
    if (await apply()) onClose();
  }

  /** Write it and stay, cleared and ready for the next one.
   *
   *  Adding one field to an entity is rare; adding five in a row is what actually happens, and
   *  reopening the dialog four times means re-choosing the entity, the type and the constraints
   *  each time. The target stays, the thing you were describing does not. */
  async function generateAndContinue() {
    if (await apply()) resetSubjectFields();
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); void generate(); }
    // The reference's own binding for adding a row, and the one that makes the rows usable
    // without reaching for the mouse.
    if (e.altKey && e.key === 'Insert' && hasConditions) {
      e.preventDefault();
      addCondition();
    }
  }

  const entityOptions = $derived(selectableEntities.map((e) => ({ value: e.fqcn, label: e.simple })));
  const propertyOptions = $derived(
    properties.map((p) => ({ value: p.path, label: `${p.path} · ${p.type_text}` })),
  );
  const keywordOptions = $derived(
    (model?.keywords ?? []).map((k) => ({ value: k.keyword, label: k.label })),
  );

  /**
   * The parameter a condition binds — the answer to "not equal to *what*".
   *
   * The row used to say the attribute and the operator and stop there, which is half a sentence: a
   * comparison has a right-hand side, and it is the thing the caller will actually pass. The
   * backend sends the arity and whether the argument is a collection precisely so this can be
   * shown without the frontend inventing rules.
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
  /** Every mapped type is a legal relation target, `@Embeddable` included. */
  const targetOptions = $derived((model?.entities ?? []).map((e) => ({ value: e.simple, label: e.simple })));
  /** The method name the backend will emit — derived, so it is shown rather than typed. It is also
   *  the one thing worth reading before the body, which is why it sits above the preview. */
  const methodName = $derived(result?.preview.match(/\b(\w+)\s*\(/)?.[1] ?? '');
  /** Only the to-many sides take a `mappedBy`; on a `@ManyToOne` it is not a thing that exists. */
  const canMapBy = $derived(['OneToMany', 'ManyToMany', 'OneToOne'].includes(attrRelation));
  const isToMany = $derived(['OneToMany', 'ManyToMany'].includes(attrRelation));
  const title = $derived(spec?.title ?? 'Generate');
  /** What the header names, and the reason the pickers below can disappear. */
  const target = $derived(
    lockedTo === 'repository'
      ? (entityRepos.find((r) => r.fqcn === repositoryFqcn)?.simple ?? entity?.simple ?? '')
      : (entity?.simple ?? ''),
  );
  /** The package, so the header says *which* `Customer` without a second line of chrome. */
  const targetPackage = $derived(entity?.fqcn.split('.').slice(0, -1).join('.') ?? '');
  /** Whether the entity is still a choice. It is not when the button that opened this already
   *  named one — or named a repository, which names one. */
  const asksForEntity = $derived(lockedTo === null);
  /** Whether the repository is still a choice: only when the dialog needs one and the file it came
   *  from did not supply it. */
  const asksForRepository = $derived(lockedTo !== 'repository');
  /** The picker's own label, so one block serves both the projection and the update. */
  const pickerTitle = $derived(body === 'modify' ? 'Set' : 'Expose');
  const pickerHint = $derived(
    body === 'modify' ? 'Each becomes one bound parameter' : 'Each property becomes one getter',
  );
  const picked = $derived(body === 'modify' ? assignments : projectionFields);
  /** Whether *Add and continue* makes sense: it does for anything that splices into a file, and
   *  not for a generator whose whole output is one new file. */
  const repeatable = $derived(!writesFile && !!result?.insertion);

  const ATTRIBUTE_KINDS = [
    { value: 'base', label: 'Base', description: 'primitive or wrapper', icon: Type },
    { value: 'enum', label: 'Enum', description: '@Enumerated(STRING)', icon: ListChecks },
    { value: 'embedded', label: 'Embedded', description: '@Embedded value type', icon: Box },
    { value: 'lob', label: 'LOB', description: 'long text or binary', icon: FileText },
  ];
  /** Cardinality, named the way it is drawn rather than the way it is annotated — `N → 1` is what
   *  you know about the relation before you know what it is called. */
  const CARDINALITIES = $derived([
    { value: 'ManyToOne', label: 'N → 1', description: `many ${target || 'rows'} to one`, icon: ArrowRight },
    { value: 'OneToMany', label: '1 → N', description: `one ${target || 'row'} to many`, icon: ArrowLeft },
    { value: 'OneToOne', label: '1 → 1', description: 'exactly one each way', icon: Link2 },
    { value: 'ManyToMany', label: 'N ↔ N', description: 'many on both sides', icon: ArrowLeftRight },
  ]);
  const SUBJECTS = [
    { value: 'find', label: 'Find' },
    { value: 'count', label: 'Count' },
    { value: 'exists', label: 'Exists' },
    { value: 'delete', label: 'Delete' },
  ];
  const returnOptions = $derived(
    JPA_RETURN_SHAPES.map(([value, label, description]) => ({ value, label, description })),
  );
  const validationChips = $derived(
    JPA_VALIDATIONS.map(([id, tip]) => ({ id, label: `@${id}`, tooltip: tip })),
  );
  const cascadeChips = $derived(JPA_CASCADE_TYPES.map((id) => ({ id, label: id })));
  /** A finder is the only thing with a shape to choose — `count` is a `long` and `exists` is a
   *  `boolean`, and offering a return-type row there would be offering a decision that does not
   *  exist. */
  const isFinder = $derived(subject === 'find');
</script>

<Modal {onClose} width="1000px" height="680px" padBody={false} ariaLabel={title}>
  {#snippet header()}
    <ModalHeader {onClose}>
      <span class="jg-crest"><Database size={15} /></span>
      <span class="jg-heading">
        <span class="modal-title">{title}</span>
        {#if target}
          <span class="jg-sub">
            {target}{#if targetPackage}<span class="jg-sub-dim"> · {targetPackage}</span>{/if}
          </span>
        {/if}
      </span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="jg" onkeydown={onKeydown}>
    {#if !spec}
      <EmptyState
        message="Unknown action"
        description="This build of the UI does not know what to open for it."
      />
    {:else if model && selectableEntities.length === 0}
      <EmptyState
        message="No entities in this project"
        description="Nothing here can be generated without an @Entity class to generate it for."
      />
    {:else}
      <div class="jg-split">
      <div class="jg-form">
        {#if asksForEntity}
          <FormSection label="Entity" first>
            <FormRow label="Class" wideControl>
              <Select value={entityFqcn} options={entityOptions} onchange={(v) => (entityFqcn = v)} />
            </FormRow>
          </FormSection>
        {/if}

        {#if body === 'repository'}
          <FormSection label="Repository" first={!asksForEntity}>
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
          </FormSection>

        {:else if body === 'projection'}
          <FormSection label="Projection" first={!asksForEntity}>
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
          </FormSection>

        {:else if body === 'attribute'}
          <FormSection label="What it maps to" first={!asksForEntity}>
            <RadioGroup
              value={attrRelation ? 'relation' : 'value'}
              appearance="segment"
              size="sm"
              block
              options={[
                { value: 'value', label: 'A value', description: 'a column on this table' },
                { value: 'relation', label: 'Another entity', description: 'a relation' },
              ]}
              onchange={(v) => (attrRelation = v === 'relation' ? attrRelation || 'ManyToOne' : '')}
            />
            {#if attrRelation}
              <RadioGroup
                value={attrRelation}
                appearance="card"
                size="sm"
                block
                options={CARDINALITIES}
                onchange={(v) => (attrRelation = v)}
              />
            {:else}
              <RadioGroup
                value={attrKind}
                appearance="card"
                size="sm"
                block
                options={ATTRIBUTE_KINDS}
                onchange={(v) => (attrKind = v as JpaAttributeKind)}
              />
            {/if}
          </FormSection>

          <FormSection label="Definition">
            <FormRow label="Field name" wideControl>
              <Input value={attrName} placeholder="createdAt" oninput={(v) => (attrName = v)} />
            </FormRow>
            {#if attrRelation}
              <FormRow label="Target entity" wideControl>
                <Select value={attrTarget} options={targetOptions} onchange={(v) => (attrTarget = v)} />
              </FormRow>
              {#if isToMany}
                <FormRow
                  label="Held in"
                  description="A Set is the safe default: a List of children makes Hibernate delete and re-insert the whole collection on any change unless it is ordered."
                >
                  <RadioGroup
                    value={attrCollection}
                    appearance="segment"
                    size="sm"
                    options={[
                      { value: 'Set', label: 'Set' },
                      { value: 'List', label: 'List' },
                      { value: 'Map', label: 'Map' },
                    ]}
                    onchange={(v) => (attrCollection = v)}
                  />
                </FormRow>
              {/if}
            {:else}
              <FormRow label="Java type" wideControl>
                <Input value={attrType} placeholder="String" oninput={(v) => (attrType = v)} />
              </FormRow>
              <FormRow
                label="Column"
                wideControl
                description="Left empty, the provider derives it from the field name."
              >
                <Input value={attrColumn} placeholder="(derived)" oninput={(v) => (attrColumn = v)} />
              </FormRow>
              <FormRow label="Default value" wideControl>
                <Input value={attrDefault} placeholder="(none)" oninput={(v) => (attrDefault = v)} />
              </FormRow>
            {/if}
          </FormSection>

          {#if attrRelation}
            <FormSection label="Direction and lifecycle">
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
              <FormRow label="Fetch lazily" description="EAGER on a collection is how a page becomes a hundred queries.">
                <Toggle bind:checked={attrLazy} size="sm" ariaLabel="Fetch lazily" />
              </FormRow>
              {#if ['OneToMany', 'OneToOne'].includes(attrRelation)}
                <FormRow
                  label="Orphan removal"
                  description="A child taken out of the collection is deleted, rather than left with a dangling key."
                >
                  <Toggle bind:checked={attrOrphan} size="sm" ariaLabel="Orphan removal" />
                </FormRow>
              {/if}
              <div class="jg-chips">
                <span class="jg-chips-label">Cascade</span>
                <ChipBar
                  items={cascadeChips}
                  selected={attrCascade}
                  multi
                  size="sm"
                  onSelect={(ids) => (attrCascade = Array.isArray(ids) ? ids : [ids])}
                />
              </div>
              <Alert variant="info" compact>
                The owning side is the one that carries the foreign key. Bennu writes
                <code>mappedBy</code> on the other side; the helper methods that keep both
                collections in step are still yours to write.
              </Alert>
            </FormSection>
          {:else}
            <FormSection label="Constraints">
              <div class="jg-inline">
                <FormRow label="Nullable">
                  <Toggle bind:checked={attrNullable} size="sm" ariaLabel="Nullable" />
                </FormRow>
                <FormRow label="Unique">
                  <Toggle bind:checked={attrUnique} size="sm" ariaLabel="Unique" />
                </FormRow>
                <FormRow label="Length" wideControl>
                  <Input value={attrLength} placeholder="255" oninput={(v) => (attrLength = v)} />
                </FormRow>
              </div>
            </FormSection>

            <FormSection
              label="Bean validation"
              hint="A NOT NULL column with no @NotNull fails at flush, from the database, instead of at validation. Usually you want both."
            >
              <ChipBar
                items={validationChips}
                selected={attrValidation}
                multi
                size="sm"
                onSelect={(ids) => (attrValidation = Array.isArray(ids) ? ids : [ids])}
              />
            </FormSection>
          {/if}

          <FormSection label="Also write">
            <FormRow label="Getter and setter">
              <Toggle bind:checked={attrAccessors} size="sm" ariaLabel="Write a getter and a setter" />
            </FormRow>
          </FormSection>

        {:else if body === 'named-query'}
          <FormSection label="Named query" first={!asksForEntity}>
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
          </FormSection>

        {:else if body === 'lifecycle'}
          <FormSection label="Callback" first={!asksForEntity}>
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
          </FormSection>

        {:else if entityRepos.length === 0}
          <Alert
            variant="warning"
            compact
            text="This entity has no repository yet — generate one first, and the method has somewhere to live."
          />
        {:else}
          <FormSection label="What it does" first={!asksForEntity}>
            {#if asksForRepository}
              <FormRow label="Repository" wideControl>
                <Select value={repositoryFqcn} options={repoOptions} onchange={(v) => (repositoryFqcn = v)} />
              </FormRow>
            {/if}
            {#if body === 'query'}
              <div class="jg-inline">
                <RadioGroup
                  value={subject}
                  appearance="segment"
                  size="sm"
                  options={SUBJECTS}
                  onchange={(v) => (subject = v)}
                />
                <FormRow label="Limit" wideControl>
                  <Input value={limit} placeholder="(none)" oninput={(v) => (limit = v)} />
                </FormRow>
              </div>
              <FormRow label="Distinct" description="Drops the duplicates a join produces.">
                <Toggle bind:checked={distinct} size="sm" ariaLabel="Distinct" />
              </FormRow>
            {/if}
            <FormRow
              label="Method name"
              wideControl
              description={nameOverride.trim() && body === 'query'
                ? 'A name Spring Data cannot parse is no longer a derived query, so the method arrives with its @Query written out.'
                : 'Left empty it is built from the conditions below, which is why it cannot be misspelled.'}
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
          </FormSection>
        {/if}

        <!-- The property picker, shared by the projection and the bulk update: both are "choose
             some of this entity's properties", and both are unusable without a filter on an entity
             with forty fields. -->
        {#if body === 'projection' || (body === 'modify' && spec && !spec.delete)}
          <FormSection label={pickerTitle} hint={pickerHint}>
            {#snippet aside()}
              {#if picked.length > 0}
                <Badge variant="count" label={String(picked.length)} />
              {/if}
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
            {/snippet}
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
          </FormSection>
        {/if}

        {#if hasConditions && entityRepos.length > 0}
          <FormSection label={body === 'modify' ? 'Affects rows where' : 'Conditions'}>
            {#snippet aside()}
              {#if conditions.length > 0}
                <Badge variant="count" label={String(conditions.length)} />
              {/if}
            {/snippet}
            {#each conditions as c, i (i)}
              {#if i > 0}
                <div class="jg-join">
                  <RadioGroup
                    value={c.or ? 'or' : 'and'}
                    appearance="segment"
                    size="sm"
                    options={[{ value: 'and', label: 'AND' }, { value: 'or', label: 'OR' }]}
                    onchange={(v) => patch(i, { or: v === 'or' })}
                  />
                  <span class="jg-join-rule" aria-hidden="true"></span>
                </div>
              {/if}
              <div class="jg-cond">
                <Select value={c.path} options={propertyOptions} onchange={(v) => patch(i, { path: v })} />
                <Select value={c.keyword} options={keywordOptions} onchange={(v) => patch(i, { keyword: v })} />
                <span class="jg-param" class:jg-none={parameterOf(c) === '—'}>{parameterOf(c)}</span>
                <Toggle
                  checked={c.ignore_case}
                  size="sm"
                  ariaLabel="Ignore case"
                  onchange={(v) => patch(i, { ignore_case: v })}
                />
                <!-- Order is not cosmetic: Spring Data evaluates the conditions left to right, so
                     `a or b and c` and `a and b or c` are different queries. -->
                <IconButton
                  tooltip="Move earlier"
                  size={22}
                  disabled={i === 0}
                  onclick={() => moveCondition(i, -1)}
                >
                  <ChevronUp size={12} />
                </IconButton>
                <IconButton
                  tooltip="Move later"
                  size={22}
                  disabled={i === conditions.length - 1}
                  onclick={() => moveCondition(i, 1)}
                >
                  <ChevronDown size={12} />
                </IconButton>
                <IconButton tooltip="Remove this condition" size={22} variant="danger" onclick={() => removeCondition(i)}>
                  <Trash2 size={12} />
                </IconButton>
              </div>
            {/each}
            <button type="button" class="jg-add" onclick={addCondition}>
              <Plus size={12} /> Add condition <Kbd keys={['Alt', 'Ins']} />
            </button>
          </FormSection>

          {#if body === 'modify'}
            <FormSection label="Result">
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
            </FormSection>
          {:else}
            <FormSection label="Ordering">
              {#each orders as o, i (i)}
                <div class="jg-order">
                  <Select value={o.path} options={propertyOptions} onchange={(v) => patchOrder(i, { path: v })} />
                  <RadioGroup
                    value={o.desc ? 'desc' : 'asc'}
                    appearance="segment"
                    size="sm"
                    options={[{ value: 'asc', label: 'Ascending' }, { value: 'desc', label: 'Descending' }]}
                    onchange={(v) => patchOrder(i, { desc: v === 'desc' })}
                  />
                  <IconButton
                    tooltip="Remove this ordering"
                    size={22}
                    variant="danger"
                    onclick={() => (orders = orders.filter((_, at) => at !== i))}
                  >
                    <Trash2 size={12} />
                  </IconButton>
                </div>
              {/each}
              <button type="button" class="jg-add" onclick={addOrder}>
                <Plus size={12} /> Add ordering
              </button>
            </FormSection>

            <FormSection label="Result">
              {#if isFinder}
                <RadioGroup
                  value={returns}
                  appearance="card"
                  size="sm"
                  block
                  options={returnOptions}
                  onchange={(v) => (returns = v as JpaReturnShape)}
                />
                <FormRow
                  label="Take a Sort parameter"
                  description={returns === 'page' || returns === 'slice'
                    ? 'Not offered here: the Pageable this method already takes carries its own Sort, and a method taking both does not compile.'
                    : 'The caller decides the ordering, instead of it being fixed in the name.'}
                >
                  <Toggle
                    checked={sorted && returns !== 'page' && returns !== 'slice'}
                    disabled={returns === 'page' || returns === 'slice'}
                    size="sm"
                    ariaLabel="Take a Sort parameter"
                    onchange={(v) => (sorted = v)}
                  />
                </FormRow>
                <FormRow
                  label="Projection"
                  wideControl
                  description="An interface with getters for the columns you want. Left empty, the method returns the entity."
                >
                  <Input
                    value={projectionType}
                    placeholder={`(the ${entity?.simple ?? 'entity'})`}
                    oninput={(v) => (projectionType = v)}
                  />
                </FormRow>
              {:else}
                <p class="jg-note">
                  A <code>{subject}</code> has one shape:
                  <code>{subject === 'exists' ? 'boolean' : 'long'}</code>.
                </p>
              {/if}
            </FormSection>
          {/if}
        {/if}
      </div>

      <!-- The preview beside the form, not under it — the same arrangement as Find in files, for
           the same reason: what you are building and what it produces are read together. -->
      <div class="jg-preview">
        {#if body === 'query' && methodName}
          <!-- The name is the one thing worth reading before the body: it is what Spring Data
               parses, and it built itself out of the conditions rather than being typed. -->
          <div class="jg-name" use:tooltip={'The method name Spring Data will parse'}>{methodName}</div>
        {/if}
        <div class="jg-preview-head">
          {#if previewTabs}
            <Tabs
              items={previewTabs.map((t) => ({ id: t.id, label: t.label }))}
              value={shownPane?.id ?? 'java'}
              size="sm"
              onSelect={(id) => (previewTab = id)}
            />
          {:else}
            <span class="jg-preview-title">Preview</span>
          {/if}
          {#if shownPane?.detail ?? (previewTabs ? undefined : destination)}
            <span class="jg-preview-detail">
              {shownPane?.detail ?? destination}
            </span>
          {/if}
        </div>
        {#if error}
          <Alert variant="error">{error}</Alert>
        {:else if (shownPane?.code ?? result?.preview ?? '') === ''}
          <EmptyState message="Fill the form and the generated code appears here." />
        {:else}
          <div class="jg-preview-code">
            <CodePreview
              text={shownPane?.code ?? result?.preview ?? ''}
              language={shownPane?.language ?? javaLang}
            />
          </div>
        {/if}
      </div>
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter>
      <span class="jg-hints">
        <Kbd keys={['Ctrl', 'Enter']} /> confirm
        <Kbd keys={['Esc']} /> close
      </span>
      <Button variant="ghost" onclick={onClose}>Cancel</Button>
      {#if repeatable}
        <Button variant="secondary" disabled={!result || busy} onclick={() => void generateAndContinue()}>
          Add and continue
        </Button>
      {/if}
      <Button
        variant="primary"
        disabled={!result || busy}
        tooltip={{ content: writesFile ? 'Create the file' : 'Add to the repository', shortcut: 'Ctrl+Enter' }}
        onclick={() => void generate()}
      >
        {writesFile ? 'Create file' : 'Add'}
      </Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .jg { display: flex; flex-direction: column; height: 100%; min-height: 0; }

  /* ── Header ────────────────────────────────────────────────────────────────── */
  /* An icon tile rather than a bare glyph: at 15px an outline icon next to 14px text reads as
     punctuation, and the dialog loses the one thing that says what family it belongs to. */
  .jg-crest {
    width: 26px; height: 26px; flex-shrink: 0;
    display: grid; place-items: center;
    border-radius: var(--radius-sm);
    background: var(--accent-subtle, rgba(255, 255, 255, 0.06));
    color: var(--accent-primary);
  }
  .jg-heading { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .jg-sub {
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    font-family: var(--font-code);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .jg-sub-dim { color: var(--text-disabled); }

  /* ── Two columns: what you are building, and what it produces ───────────────── */
  .jg-split {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 5fr) minmax(0, 6fr);
    gap: 14px;
    padding: 12px 16px 14px;
  }

  .jg-form {
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding-right: 4px;
    overflow-y: auto;
  }
  /* FormRow's control column is sized for a settings page; in a dense dialog it should take the
     width it is given rather than hugging a chevron. */
  .jg-form :global(.fr-row) { padding: 3px 0; }
  .jg-form :global(.fr-control .select),
  .jg-form :global(.fr-control .input) { width: 100%; }

  .jg-preview { min-height: 0; display: flex; flex-direction: column; gap: 8px; }
  /* The code pane takes what the head leaves, so the preview grows with the modal instead of
     scrolling the whole column. */
  .jg-preview-code { flex: 1; min-height: 0; display: flex; }
  .jg-preview-code > :global(.cp) { flex: 1; min-height: 0; }
  .jg-preview-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    min-height: 22px;
  }
  .jg-preview-title { font-size: 11px; font-weight: 600; color: var(--text-secondary); }
  .jg-preview-detail {
    font-size: 11px;
    color: var(--text-tertiary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* The derived method name, above the code it heads. */
  .jg-name {
    flex-shrink: 0;
    padding: 9px 12px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--text-primary);
    word-break: break-all;
    line-height: 1.5;
  }

  /* ── Rows ──────────────────────────────────────────────────────────────────── */
  /* Several controls on one line, each keeping its own label. */
  .jg-inline { display: flex; align-items: flex-end; gap: 12px; flex-wrap: wrap; }
  .jg-inline > :global(*) { min-width: 0; }

  .jg-chips { display: flex; flex-direction: column; gap: 5px; }
  .jg-chips-label { font-size: var(--font-size-2xs); color: var(--text-muted); }

  /* A condition, and the join that precedes it. The joiner sits between the rows rather than
     inside one, because that is where the word belongs — it is about the pair. */
  .jg-cond {
    display: grid;
    grid-template-columns: minmax(0, 1.2fr) minmax(0, 1fr) minmax(0, 0.7fr) auto auto auto auto;
    align-items: center;
    gap: 6px;
  }
  .jg-join { display: flex; align-items: center; gap: 8px; padding: 1px 0; }
  .jg-join-rule { flex: 1; height: 1px; background: var(--border-subtle); }
  .jg-param {
    font-family: var(--font-code);
    font-size: var(--font-size-3xs);
    color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .jg-none { color: var(--text-disabled); }

  .jg-order {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 6px;
  }

  .jg-add {
    display: flex; align-items: center; justify-content: center; gap: 6px;
    width: 100%; padding: 7px;
    border: 1px dashed var(--border-subtle);
    border-radius: var(--radius-sm);
    background: none; cursor: pointer;
    font: inherit; font-size: var(--font-size-2xs);
    color: var(--text-muted);
  }
  .jg-add:hover { border-color: var(--accent-primary); color: var(--text-primary); }
  .jg-add:focus-visible { outline: 1px solid var(--accent-primary); outline-offset: -1px; }

  /* ── The property picker ───────────────────────────────────────────────────── */
  .jg-filter {
    display: flex; align-items: center; gap: 5px;
    padding: 2px 7px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-disabled);
  }
  .jg-filter input {
    width: 92px; border: 0; background: none; outline: none;
    font: inherit; font-size: var(--font-size-2xs); color: var(--text-primary);
  }
  .jg-rows {
    max-height: 190px; overflow-y: auto;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
  }
  .jg-check {
    display: flex; align-items: center; gap: 8px;
    padding: 4px 9px; cursor: pointer;
    font-size: var(--font-size-2xs);
  }
  .jg-check:hover { background: var(--bg-hover); }
  .jg-path { flex: 1; min-width: 0; font-family: var(--font-code); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .jg-type { flex-shrink: 0; color: var(--text-disabled); font-family: var(--font-code); }

  /* ── The named-query editor ────────────────────────────────────────────────── */
  .jg-area { display: flex; flex-direction: column; gap: 5px; }
  .jg-area-label { font-size: var(--font-size-2xs); color: var(--text-muted); }
  .jg-textarea {
    min-height: 88px; resize: vertical;
    padding: 8px 10px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--text-primary);
    outline: none;
  }
  .jg-textarea:focus { border-color: var(--accent-primary); }

  .jg-note {
    margin: 0;
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    line-height: 1.5;
  }
  .jg-pad { padding: 10px; }

  /* ── Footer ────────────────────────────────────────────────────────────────── */
  /* The bindings, on the left, where they are readable without being loud. A dialog meant to be
     driven from the keyboard should say so. */
  .jg-hints {
    margin-right: auto;
    display: flex; align-items: center; gap: 6px;
    font-size: var(--font-size-3xs);
    color: var(--text-disabled);
  }
</style>
