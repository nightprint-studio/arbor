<script lang="ts">
  /**
   * Bennu Run Configurations — the IntelliJ-style run-configuration EDITOR.
   *
   * Two panes:
   *   • LEFT  — the list of NAMED configs for the open project. Create (+),
   *     duplicate, delete; arrow-key navigation; a ● marks the ACTIVE config
   *     (what the titlebar ▶ / Shift+F10 launches). Right-click for the same
   *     actions via the shared context menu.
   *   • RIGHT — the form for the selected config. Which fields depends on its KIND:
   *     an Application takes a module, a main class (typeable, with a picker listing the
   *     entry points found in that module), program args, VM args, working directory and
   *     environment; a Spring Boot one adds its active profiles and may leave the class
   *     empty; a JUnit one takes a scope and its target and nothing else.
   *
   * The **module** comes first for a reason: it decides which compiled output the run uses,
   * so on a reactor it is the difference between a run and a `ClassNotFoundException` — and
   * it is the answer to "what am I creating this for", which was previously nowhere on
   * screen. Picking a main class fills it in, since the class already says which module it
   * is in.
   *
   * Every edit funnels straight into {@link bennuRunConfigStore}, which persists to
   * `<root>/.arbor/config.toml` on a short debounce — so there's no separate "Apply",
   * and the configurations are still there tomorrow. "Run" builds then launches the
   * SELECTED config; the ● (Set active) button makes it the default target.
   *
   * Keyboard-first: the config list auto-focuses when non-empty; ↑/↓ move the
   * selection, Enter runs it; Tab cycles the form fields; Ctrl/Cmd+Enter saves &
   * closes (everything is already saved — this just dismisses); Esc cancels
   * (handled by <Modal>).
   */
  import {
    Play, Plus, Copy, Trash2, SlidersHorizontal, CircleDot, Circle, X, Search, ChevronDown,
    Layers, Leaf,
  } from 'lucide-svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuMainClassStore } from '$lib/stores/bennu/main-classes.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuRunStore } from '$lib/stores/bennu/run.svelte';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import {
    bennuRunConfigStore, isJvmKind, isRunKind, runKindLabel, RUN_KINDS,
    type RunConfig, type EnvVar, type RunConfigKind, type TestScopeKind,
  } from '$lib/stores/bennu/run-config.svelte';
  import { springStore } from '$lib/stores/bennu/spring.svelte';
  import { runKindIcon } from './run-kinds';

  let { onClose }: { onClose: () => void } = $props();

  const project = $derived(projectStore.project);
  const root = $derived(project?.root ?? null);
  /** Whether the project has Spring at all — what gates offering a Spring Boot kind. */
  const hasSpring = $derived(
    !!projectStore.capabilities?.spring_annotation_di || !!projectStore.capabilities?.spring_xml_di,
  );
  /** The kinds offered by the + menu. An existing configuration of a kind that is not
   *  offered still appears in the list and still runs — the project may have changed, or the
   *  file may be shared with someone whose checkout has more in it. */
  const offeredKinds = $derived(RUN_KINDS.filter((k) => k.capability !== 'spring' || hasSpring));

  // ── Selection ───────────────────────────────────────────────────────────────
  // Selected = which config the form edits (distinct from ACTIVE = what ▶ Run
  // launches). Seed from the active config so opening lands on the run target.
  let selectedId = $state<string | null>(null);

  const configs = $derived(root ? bennuRunConfigStore.configsFor(root) : []);
  /** The same grouping the title-bar selector shows — one function in the store, so the
   *  editor and the selector can never disagree about what a category holds. */
  const groups = $derived(root ? bennuRunConfigStore.groupedFor(root) : []);
  /** The rows in DISPLAY order (groups flattened), which is what ↑/↓ must walk: navigating
   *  a tree by the order it was stored in, rather than the order it is drawn in, jumps. */
  const ordered = $derived(groups.flatMap((g) => g.configs));
  const activeId = $derived(root ? bennuRunConfigStore.activeIdFor(root) : null);
  const selected = $derived<RunConfig | null>(
    configs.find((c) => c.id === selectedId) ?? null,
  );

  // Keep the selection valid as the list mutates (create/delete): fall back to the
  // active config, then the first, then null.
  $effect(() => {
    if (!root) return;
    if (selectedId && configs.some((c) => c.id === selectedId)) return;
    selectedId = activeId ?? configs[0]?.id ?? null;
  });

  // ── Entry-point discovery ───────────────────────────────────────────────────
  // Every class in the project declaring `public static void main(String[])`. Read through
  // the shared store: ▷ and the Spring Boot resolution want the same list, and this modal
  // gets opened repeatedly — one read per project, not one per opening.
  const mains = $derived(root ? bennuMainClassStore.forRoot(root) : []);
  const scanning = $derived(!!root && bennuMainClassStore.isLoading(root));

  $effect(() => {
    const r = root;
    if (!r || projectStore.isDemo) return;
    void bennuMainClassStore.load(r);
  });

  // ── Modules ─────────────────────────────────────────────────────────────────
  // On a reactor the module is the field that decides the CLASSPATH: the root of a
  // multi-module project compiles nothing, so a configuration without one launched against
  // a directory that does not exist. It is also the answer to "what am I creating this for",
  // which was previously nowhere on screen.
  const modules = $derived(project?.modules ?? []);
  const isMultiModule = $derived(modules.length > 0);
  const moduleOptions = $derived([
    { value: '', label: isMultiModule ? '(project root)' : '(the project)' },
    ...modules.map((m) => ({ value: m, label: m })),
  ]);

  /** The entry points to offer: those of the selected module, or all of them when the
   *  configuration has no module yet (which is how you find out which module to pick). */
  const modulesMains = $derived(
    selected?.module ? mains.filter((m) => (m.module ?? '') === selected.module) : mains,
  );

  /** The Boot entry point the selected module resolves to on its own, when there is exactly
   *  one — what a Spring Boot configuration runs with its main class left empty. */
  const detectedBootClass = $derived.by(() => {
    const boots = modulesMains.filter((m) => m.spring_boot);
    return boots.length === 1 ? boots[0].fqcn : '';
  });

  const mainClassHint = $derived.by(() => {
    if (scanning) return 'Looking for entry points…';
    if (selected?.kind === 'springboot') {
      return detectedBootClass
        ? `Optional — left empty, this runs the module's @SpringBootApplication (${detectedBootClass}).`
        : 'No @SpringBootApplication found in this module, so name the class to start.';
    }
    return modulesMains.length
      ? `Fully qualified class with a public static void main. ${modulesMains.length} found${selected?.module ? ' in this module' : ' in this project'}.`
      : 'Fully qualified class with a public static void main. None found — type one if it lives outside the source roots.';
  });

  /** The picker: one row per entry point, with its module in the meta column on a
   *  multi-module project — which is what tells two same-named `App`s apart. Choosing one
   *  also sets the MODULE, because the class already says which one it is in and making you
   *  say it again is how the two come to disagree. */
  const mainClassItems = $derived<DropdownItem[]>(
    modulesMains.map((m) => ({
      kind: 'item',
      id: m.fqcn + (m.module ?? ''),
      label: m.fqcn,
      meta: m.module ?? undefined,
      icon: m.spring_boot ? Leaf : Play,
      onclick: () => patch({ mainClass: m.fqcn, module: m.module ?? '' }),
    })),
  );

  // ── Spring profiles ─────────────────────────────────────────────────────────
  // Detected from the project's own `application-<profile>.yml` / `.properties` — the model
  // the Spring extension already builds, so this costs a read of something in memory rather
  // than a second scan of the tree. The field stays free text: a profile can be invented on
  // the spot (one that only exists in a `spring.config.activate.on-profile` block, or one
  // you are about to add), and a picker that refused it would be worse than no picker.
  $effect(() => {
    const r = root;
    if (r && !projectStore.isDemo) void springStore.loadOverview(r);
  });

  const detectedProfiles = $derived(
    [...new Set(springStore.propertyFiles.map((f) => f.profile).filter(Boolean))].sort(),
  );

  /** Toggle `profile` in the selected config's comma-separated list — profiles compose
   *  (`dev,local`), so the picker adds and removes rather than replacing. */
  function toggleProfile(profile: string) {
    if (!selected) return;
    const current = selected.profiles.split(',').map((p) => p.trim()).filter(Boolean);
    const next = current.includes(profile)
      ? current.filter((p) => p !== profile)
      : [...current, profile];
    patch({ profiles: next.join(',') });
  }

  const activeProfiles = $derived(
    new Set((selected?.profiles ?? '').split(',').map((p) => p.trim()).filter(Boolean)),
  );

  const profileItems = $derived<DropdownItem[]>(
    detectedProfiles.map((p) => ({
      kind: 'item',
      id: p,
      label: p,
      active: activeProfiles.has(p),
      onclick: () => toggleProfile(p),
    })),
  );

  const profilesHint = $derived(
    detectedProfiles.length
      ? `Comma-separated, as Spring spells them. Found in this project: ${detectedProfiles.join(', ')}.`
      : 'Comma-separated, as Spring spells them. No application-<profile> file found — type one anyway if it is declared inside a YAML document.',
  );

  // ── List actions ────────────────────────────────────────────────────────────
  /** The + menu: one entry per offered kind, the way IntelliJ's "Add New Configuration" is
   *  a list of types rather than a button that guesses. */
  const addItems = $derived<DropdownItem[]>(
    offeredKinds.map((k) => ({
      kind: 'item',
      id: k.id,
      label: k.label,
      icon: runKindIcon(k.id),
      onclick: () => createConfig(k.id),
    })),
  );

  function createConfig(kind: RunConfigKind = 'application') {
    if (!root) return;
    // Seed from the project when the answer is not in doubt: a JVM configuration on a project
    // with exactly one entry point (or a Spring Boot one with exactly one Boot entry point)
    // is a configuration for that entry point, and its module comes with it. Anything
    // ambiguous is left blank for you to choose — guessing between two is worse than asking.
    const candidates = kind === 'springboot' ? mains.filter((m) => m.spring_boot) : mains;
    const only = isJvmKind(kind) && candidates.length === 1 ? candidates[0] : null;
    selectedId = bennuRunConfigStore.create(root, kind, {
      ...(only
        ? {
            name: only.fqcn.split('.').pop() || 'Application',
            mainClass: only.fqcn,
            module: only.module ?? '',
          }
        : {}),
    });
    focusNameSoon();
  }
  function duplicateConfig(id: string) {
    if (!root) return;
    const nid = bennuRunConfigStore.duplicate(root, id);
    if (nid) { selectedId = nid; focusNameSoon(); }
  }
  function deleteConfig(id: string) {
    if (!root) return;
    selectedId = bennuRunConfigStore.remove(root, id);
  }
  function setActive(id: string) {
    if (!root) return;
    bennuRunConfigStore.setActive(root, id);
  }

  // ── Form edits — every change persists straight into the store ───────────────
  function patch(p: Partial<Omit<RunConfig, 'id'>>) {
    if (root && selectedId) bennuRunConfigStore.update(root, selectedId, p);
  }
  function addEnv() {
    if (!selected) return;
    patch({ env: [...selected.env, { key: '', value: '' }] });
  }
  function updateEnv(idx: number, next: Partial<EnvVar>) {
    if (!selected) return;
    const env = selected.env.map((e, i) => (i === idx ? { ...e, ...next } : e));
    patch({ env });
  }
  function removeEnv(idx: number) {
    if (!selected) return;
    patch({ env: selected.env.filter((_, i) => i !== idx) });
  }

  // ── Run the SELECTED config (build then launch) ──────────────────────────────
  // A JVM configuration needs a class; a JUnit one is runnable as soon as it exists (an
  // empty scope means "everything", which is a legitimate thing to run).
  const canRun = $derived(
    !!root &&
      !!selected &&
      !bennuRunStore.active &&
      // A kind this build doesn't know (written by a newer Bennu) is shown but not runnable.
      isRunKind(selected.kind) &&
      // A Spring Boot configuration is runnable with an empty class when the module has one
      // `@SpringBootApplication` — that IS the class, and typing it changes nothing.
      (isJvmKind(selected.kind)
        ? selected.mainClass.trim().length > 0 ||
          (selected.kind === 'springboot' && !!detectedBootClass)
        : true),
  );
  function runSelected() {
    if (!root || !selected || !canRun) return;
    // Make the config we're launching the active one, so the titlebar ▶ keeps
    // running the same target next time.
    bennuRunConfigStore.setActive(root, selected.id);
    const cfg = selected;
    onClose();
    // The whole configuration, not just its main class: VM args, working directory and
    // environment are the reason it is a configuration and not a class name.
    void bennuRunStore.runConfig(root, cfg);
  }

  // ── Keyboard nav on the config list ──────────────────────────────────────────
  let listEl = $state<HTMLUListElement | undefined>();
  let nameEl = $state<HTMLInputElement | undefined>();

  function focusNameSoon() {
    queueMicrotask(() => nameEl?.focus());
  }

  function onListKeydown(e: KeyboardEvent) {
    if (!ordered.length) return;
    // Walks the DISPLAYED order, so ↓ from the last Application lands on the first JUnit
    // rather than wherever that configuration happens to sit in the stored list.
    const idx = ordered.findIndex((c) => c.id === selectedId);
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedId = ordered[Math.min(idx + 1, ordered.length - 1)]?.id ?? selectedId;
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedId = ordered[Math.max(idx - 1, 0)]?.id ?? selectedId;
    } else if (e.key === 'Home') {
      e.preventDefault();
      selectedId = ordered[0]?.id ?? selectedId;
    } else if (e.key === 'End') {
      e.preventDefault();
      selectedId = ordered[ordered.length - 1]?.id ?? selectedId;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      runSelected();
    } else if (e.key === 'Delete' && selectedId) {
      e.preventDefault();
      deleteConfig(selectedId);
    }
  }

  function openRowMenu(e: MouseEvent, cfg: RunConfig) {
    e.preventDefault();
    const items: MenuItem[] = [
      { id: 'active', label: 'Set as active', icon: CircleDot, disabled: cfg.id === activeId },
      { id: 'duplicate', label: 'Duplicate', icon: Copy },
      { id: 'delete', label: 'Delete', icon: Trash2, danger: true },
    ];
    bennuContextMenuStore.show(e.clientX, e.clientY, items, (id) => {
      if (id === 'active') setActive(cfg.id);
      else if (id === 'duplicate') duplicateConfig(cfg.id);
      else if (id === 'delete') deleteConfig(cfg.id);
    });
  }

  // Ctrl/Cmd+Enter = save & close (everything is live-saved; this just dismisses).
  function onBodyKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      onClose();
    }
  }

  // Auto-focus the list ONCE, the first time it mounts with configs, so ↑/↓ work
  // immediately. Guarded so later list mutations (create → focus the name field)
  // don't yank focus back to the list.
  let listFocused = false;
  $effect(() => {
    if (!listFocused && configs.length && listEl) {
      listFocused = true;
      const el = listEl;
      queueMicrotask(() => el.focus());
    }
  });

  /** Roughly what will be run, for the kind being edited. A sketch, not the real command —
   *  that one comes back from the backend and is printed by the Run console, which is the
   *  only place it can be accurate about the JDK and the resolved classpath. */
  const cmdPreview = $derived.by(() => {
    if (!selected) return '';
    if (selected.kind === 'junit') {
      const target = selected.testTarget.trim();
      if (selected.testScope === 'module' && target) return `mvn -pl ${target} test`;
      if (selected.testScope === 'class' && target) return `mvn test -Dtest=${target}`;
      return 'mvn test';
    }
    const mod = selected.module.trim();
    const vm = [
      selected.vmArgs.trim(),
      selected.kind === 'springboot' && selected.profiles.trim()
        ? `-Dspring.profiles.active=${selected.profiles.trim()}`
        : '',
    ]
      .filter(Boolean)
      .join(' ');
    const prog = selected.programArgs.trim();
    const cls = selected.mainClass.trim() || detectedBootClass || 'com.example.Main';
    const cp = `${mod ? mod + '/' : ''}target/classes:<deps>`;
    return `java ${vm ? vm + ' ' : ''}-cp ${cp} ${cls}${prog ? ' ' + prog : ''}`;
  });
</script>

<Modal
  {onClose}
  width="820px"
  height="600px"
  padBody={false}
  ariaLabel="Bennu Run Configurations"
>
  {#snippet header()}
    <ModalHeader {onClose}>
      <SlidersHorizontal size={14} />
      <span class="modal-title">Run Configurations</span>
      {#if project}<span class="hdr-name">{project.name}</span>{/if}
    </ModalHeader>
  {/snippet}

  {#if !project}
    <div class="empty-wrap">
      <EmptyState message="Open a project to configure a run." />
    </div>
  {:else}
    <div class="split">
      <!-- LEFT — config list ─────────────────────────────────────────────── -->
      <aside class="list-pane">
        <div class="list-toolbar">
          <span class="list-title">Configurations</span>
          <div class="list-tools">
            <Dropdown items={addItems} position="fixed" direction="down" width="220px">
              {#snippet trigger({ toggle, open })}
                <button
                  class="icon-btn"
                  class:open
                  type="button"
                  onclick={toggle}
                  use:tooltip={'Add new configuration'}
                  aria-label="Add new configuration"
                  aria-haspopup="menu"
                  aria-expanded={open}
                >
                  <Plus size={14} />
                </button>
              {/snippet}
            </Dropdown>
            <button
              class="icon-btn"
              onclick={() => selectedId && duplicateConfig(selectedId)}
              disabled={!selectedId}
              use:tooltip={'Duplicate'}
              aria-label="Duplicate configuration"
            >
              <Copy size={14} />
            </button>
            <button
              class="icon-btn"
              onclick={() => selectedId && deleteConfig(selectedId)}
              disabled={!selectedId}
              use:tooltip={'Delete'}
              aria-label="Delete configuration"
            >
              <Trash2 size={14} />
            </button>
          </div>
        </div>

        {#if configs.length === 0}
          <div class="list-empty">
            <EmptyState message="No run configurations yet." compact />
            <Button variant="secondary" size="sm" onclick={() => createConfig('application')}>
              {#snippet iconStart()}<Plus size={13} />{/snippet}
              Add configuration
            </Button>
          </div>
        {:else}
          <!-- Grouped by category, like IntelliJ's — a flat list of eight names says nothing
               about which of them is a test and which starts a server. One `ul` and not one
               per group, so the whole thing stays a single listbox for the keyboard. -->
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <ul
            class="cfg-list"
            role="listbox"
            aria-label="Run configurations"
            tabindex="0"
            bind:this={listEl}
            onkeydown={onListKeydown}
          >
            {#each groups as group (group.kind)}
              {@const GroupIcon = runKindIcon(group.kind)}
              <li class="cfg-group" role="presentation">
                <GroupIcon size={12} />
                <span>{group.label}</span>
                <span class="cfg-group-n">{group.configs.length}</span>
              </li>
              {#each group.configs as cfg (cfg.id)}
                <li role="option" aria-selected={cfg.id === selectedId}>
                  <button
                    class="cfg-row"
                    class:selected={cfg.id === selectedId}
                    onclick={() => (selectedId = cfg.id)}
                    ondblclick={runSelected}
                    oncontextmenu={(e) => openRowMenu(e, cfg)}
                    title={cfg.id === activeId ? 'Active configuration' : ''}
                  >
                    <span class="cfg-mark" class:active={cfg.id === activeId}>
                      {#if cfg.id === activeId}
                        <CircleDot size={13} />
                      {:else}
                        <Circle size={13} />
                      {/if}
                    </span>
                    <span class="cfg-name">{cfg.name || 'Unnamed'}</span>
                    <!-- Which module it is for. On a reactor, four configurations called
                         "Application" are otherwise indistinguishable in this list. -->
                    {#if isMultiModule}
                      {@const mod = cfg.kind === 'junit' ? (cfg.testScope === 'module' ? cfg.testTarget : '') : cfg.module}
                      {#if mod}<span class="cfg-mod" title={mod}>{mod}</span>{/if}
                    {/if}
                    {#if cfg.id === activeId}<span class="cfg-badge">active</span>{/if}
                  </button>
                </li>
              {/each}
            {/each}
          </ul>
        {/if}
      </aside>

      <!-- RIGHT — form for the selected config ────────────────────────────── -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <section class="form-pane" onkeydown={onBodyKeydown}>
        {#if !selected}
          <EmptyState message="Select or create a configuration to edit it." />
        {:else}
          <div class="form-head">
            <FormField label="Name">
              {#snippet actions()}
                <!-- What kind of thing this is, stated where you are editing it. Read-only:
                     changing an Application into a JUnit run is not an edit, it is a
                     different configuration — make that one and delete this. -->
                {@const KindIcon = runKindIcon(selected.kind)}
                <span class="kind-pill"><KindIcon size={11} />{runKindLabel(selected.kind)}</span>
              {/snippet}
              <Input
                value={selected.name}
                bind:element={nameEl}
                placeholder="Application"
                oninput={(v) => patch({ name: v })}
              />
            </FormField>
            <Button
              variant={selected.id === activeId ? 'tonal' : 'secondary'}
              size="sm"
              onclick={() => setActive(selected!.id)}
              disabled={selected.id === activeId}
              tooltip={{ content: 'Make this the ▶ Run target' }}
            >
              {#snippet iconStart()}<CircleDot size={13} />{/snippet}
              {selected.id === activeId ? 'Active' : 'Set active'}
            </Button>
          </div>

          <!-- JUnit: what to run, and nothing about classpaths or a main class — a test run
               goes through Maven, which decides those. -->
          {#if selected.kind === 'junit'}
            <FormField label="Run" hint="What the run covers. Rerun and Rerun-failed still apply to it.">
              <RadioGroup
                value={selected.testScope}
                size="sm"
                block
                options={[
                  { value: 'all', label: 'Whole project' },
                  { value: 'module', label: 'A module' },
                  { value: 'class', label: 'A class' },
                ]}
                onchange={(v) => patch({ testScope: v as TestScopeKind })}
              />
            </FormField>

            {#if selected.testScope === 'module'}
              <FormField
                label="Module"
                hint={isMultiModule
                  ? 'Its tests, and those of the modules it builds.'
                  : 'This project has one module, so this is the whole project.'}
              >
                <!-- Chosen, not typed: the project already knows its modules, and a typo here
                     produces a Maven error about a missing project rather than a hint. -->
                <Select
                  value={selected.testTarget}
                  options={moduleOptions}
                  fill
                  disabled={!isMultiModule}
                  searchable={modules.length > 12}
                  onchange={(v) => patch({ testTarget: v })}
                />
              </FormField>
            {:else if selected.testScope === 'class'}
              <FormField
                label="Class"
                hint="The test class — its simple name, or the fully qualified one when two modules share it."
              >
                <Input
                  value={selected.testTarget}
                  placeholder="OrderServiceTest"
                  oninput={(v) => patch({ testTarget: v })}
                />
              </FormField>
            {/if}
          {:else}

          <!-- Above the class, because it narrows what the class picker offers and it is the
               field that decides the classpath. Shown on a single-module project too, where
               it has one option and says so — a form whose fields appear and disappear is
               harder to learn than one with an obvious answer already in it. -->
          <FormField
            label="Module"
            hint={isMultiModule
              ? 'Whose compiled classes and dependencies the run uses. The reactor root usually compiles nothing.'
              : 'This project has one module.'}
          >
            <Select
              value={selected.module}
              options={moduleOptions}
              fill
              disabled={!isMultiModule}
              searchable={modules.length > 12}
              onchange={(v) => patch({ module: v })}
            />
          </FormField>

          <FormField label="Main class" hint={mainClassHint}>
            {#snippet actions()}
              <Dropdown items={mainClassItems} position="fixed" direction="down" width="380px">
                {#snippet trigger({ toggle, open })}
                  <button
                    class="pick-btn"
                    class:open
                    type="button"
                    onclick={toggle}
                    disabled={!modulesMains.length}
                    use:tooltip={modulesMains.length
                      ? 'Pick one of the entry points found here'
                      : 'No class here declares a main method'}
                    aria-haspopup="menu"
                    aria-expanded={open}
                  >
                    <Search size={12} />
                    Choose{modulesMains.length ? ` (${modulesMains.length})` : ''}
                    <ChevronDown size={12} />
                  </button>
                {/snippet}
              </Dropdown>
            {/snippet}
            <Input
              value={selected.mainClass}
              placeholder={selected.kind === 'springboot' && detectedBootClass
                ? detectedBootClass
                : 'com.example.Main'}
              oninput={(v) => patch({ mainClass: v })}
            />
          </FormField>

          {#if selected.kind === 'springboot'}
            <FormField label="Active profiles" hint={profilesHint}>
              {#snippet actions()}
                <Dropdown items={profileItems} position="fixed" direction="down" width="240px">
                  {#snippet trigger({ toggle, open })}
                    <button
                      class="pick-btn"
                      class:open
                      type="button"
                      onclick={toggle}
                      disabled={!detectedProfiles.length}
                      use:tooltip={detectedProfiles.length
                        ? 'Profiles this project declares — click to add or remove'
                        : 'No application-<profile> file found in this project'}
                      aria-haspopup="menu"
                      aria-expanded={open}
                    >
                      <Layers size={12} />
                      Detected{detectedProfiles.length ? ` (${detectedProfiles.length})` : ''}
                      <ChevronDown size={12} />
                    </button>
                  {/snippet}
                </Dropdown>
              {/snippet}
              <!-- Free text, with the picker as a shortcut and never as a gate: a profile can
                   exist only inside a `spring.config.activate.on-profile` block, or not exist
                   yet because you are about to add it. -->
              <Input
                value={selected.profiles}
                placeholder="dev,local"
                oninput={(v) => patch({ profiles: v })}
              />
            </FormField>
          {/if}

          <FormField label="Program arguments" hint="Passed to the program after the main class.">
            <Input
              value={selected.programArgs}
              placeholder="--port 8080 input.txt"
              oninput={(v) => patch({ programArgs: v })}
            />
          </FormField>

          <FormField label="VM arguments" hint="JVM options (-Xmx…, -D…), passed before the classpath.">
            <Input
              value={selected.vmArgs}
              placeholder="-Xmx512m -Dfile.encoding=UTF-8"
              oninput={(v) => patch({ vmArgs: v })}
            />
          </FormField>

          <FormField label="Working directory" hint="Empty = the project root.">
            <Input
              value={selected.workingDir}
              placeholder={project?.root ?? '/path/to/project'}
              oninput={(v) => patch({ workingDir: v })}
            />
          </FormField>

          <FormField label="Environment variables">
            {#snippet actions()}
              <button
                class="icon-btn"
                onclick={addEnv}
                use:tooltip={'Add variable'}
                aria-label="Add environment variable"
              >
                <Plus size={13} />
              </button>
            {/snippet}
            {#if selected.env.length === 0}
              <div class="env-empty">No environment variables.</div>
            {:else}
              <div class="env-rows">
                {#each selected.env as row, i (i)}
                  <div class="env-row">
                    <Input
                      value={row.key}
                      placeholder="NAME"
                      ariaLabel="Variable name"
                      oninput={(v) => updateEnv(i, { key: v })}
                    />
                    <span class="env-eq">=</span>
                    <Input
                      value={row.value}
                      placeholder="value"
                      ariaLabel="Variable value"
                      oninput={(v) => updateEnv(i, { value: v })}
                    />
                    <button
                      class="icon-btn"
                      onclick={() => removeEnv(i)}
                      use:tooltip={'Remove'}
                      aria-label="Remove variable"
                    >
                      <X size={13} />
                    </button>
                  </div>
                {/each}
              </div>
            {/if}
          </FormField>
          {/if}

          <p class="cmd-preview"><span class="cmd">{cmdPreview}</span></p>
        {/if}
      </section>
    </div>
  {/if}

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="foot-hint">Changes save as you type.</span>
      <div class="footer-actions">
        <Button variant="secondary" size="sm" onclick={onClose}>Close</Button>
        <Button
          variant="primary"
          size="sm"
          onclick={runSelected}
          disabled={!canRun}
          tooltip={{ content: 'Build & run this configuration', shortcut: 'Enter' }}
        >
          {#snippet iconStart()}<Play size={13} />{/snippet}
          Run
        </Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .hdr-name {
    font-size: var(--font-size-xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }

  .empty-wrap { display: flex; align-items: center; justify-content: center; height: 100%; }

  /* Two-pane split — edge-to-edge (padBody={false}). Left list on --bg-base with
     a divider, right form scrolls. */
  .split {
    display: grid;
    grid-template-columns: 240px 1fr;
    height: 100%;
    min-height: 0;
  }

  /* ── Left: list pane ──────────────────────────────────────────────────── */
  .list-pane {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--border-subtle);
    background: var(--bg-base);
  }
  .list-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 8px 8px 12px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .list-title {
    font-size: var(--font-size-xs); font-weight: 600; letter-spacing: 0.02em;
    color: var(--text-secondary); text-transform: uppercase;
  }
  .list-tools { display: flex; align-items: center; gap: 2px; }

  .list-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 24px 12px;
  }

  .cfg-list {
    list-style: none;
    margin: 0;
    padding: 4px;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }
  .cfg-list:focus-visible { outline: none; }
  .cfg-list:focus-visible .cfg-row.selected {
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .cfg-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-size: var(--font-size-sm);
    text-align: left;
    cursor: pointer;
    transition: background var(--transition-fast);
  }
  .cfg-row:hover { background: var(--bg-hover); }
  .cfg-row.selected { background: var(--bg-selected, var(--bg-hover)); }

  /* A category heading. Quiet on purpose — it groups the rows, it is not one of them, and a
     tree whose branches shout louder than its leaves is harder to scan, not easier. */
  .cfg-group {
    display: flex; align-items: center; gap: 6px;
    padding: 9px 8px 3px;
    font-size: var(--font-size-2xs); font-weight: 600;
    letter-spacing: 0.05em; text-transform: uppercase;
    color: var(--text-muted);
  }
  .cfg-group:first-child { padding-top: 4px; }
  .cfg-group-n { margin-left: auto; font-weight: 500; letter-spacing: 0; opacity: 0.7; }

  /* The rows sit under their heading — the indent is what makes the grouping readable at a
     glance rather than only after reading the words. */
  .cfg-list :global(li[role='option']) { padding-left: 10px; }

  .cfg-mark { display: inline-flex; color: var(--text-muted); flex-shrink: 0; }
  .cfg-mark.active { color: var(--success); }
  .cfg-name {
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  /* The module, in the row. Truncates from the LEFT: `…/services/core` keeps the part that
     tells two modules apart, where a normal ellipsis would keep the shared prefix. */
  .cfg-mod {
    max-width: 88px; flex-shrink: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left;
    font-family: var(--font-code); font-size: var(--font-size-3xs); color: var(--text-disabled);
  }
  .cfg-badge {
    font-size: var(--font-size-3xs); text-transform: uppercase; letter-spacing: 0.4px; font-weight: 700;
    color: var(--success);
    background: color-mix(in srgb, var(--success) 16%, transparent);
    border-radius: var(--radius-sm); padding: 1px 5px; flex-shrink: 0;
  }

  /* ── Right: form pane ─────────────────────────────────────────────────── */
  .form-pane {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px 18px;
    overflow-y: auto;
    min-height: 0;
  }
  .form-head {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: end;
    gap: 12px;
  }

  /* What kind this configuration is, in the field's action slot: a label, not a control —
     the kind is decided when it is created. */
  .kind-pill {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 1px 7px;
    border-radius: var(--radius-sm);
    background: var(--bg-overlay);
    color: var(--text-muted);
    font-size: var(--font-size-2xs);
  }

  .env-empty { font-size: var(--font-size-xs); color: var(--text-muted); padding: 2px 0; }
  .env-rows { display: flex; flex-direction: column; gap: 6px; }
  .env-row {
    display: grid;
    grid-template-columns: 1fr auto 1.4fr auto;
    align-items: center;
    gap: 6px;
  }
  .env-eq { color: var(--text-muted); font-family: var(--font-code); }

  /* Shared small icon button — matches the list toolbar + field actions. */
  .icon-btn {
    display: inline-flex; align-items: center; justify-content: center;
    width: 24px; height: 24px;
    background: transparent; border: none; border-radius: var(--radius-sm);
    color: var(--text-secondary); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .icon-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .icon-btn:disabled { opacity: 0.4; cursor: default; }

  /* The entry-point picker, in the field's own action slot — a labelled control, not an
     icon button: "Choose (3)" says both what it does and that there is something to
     choose, which an icon at this size cannot. */
  .pick-btn {
    display: inline-flex; align-items: center; gap: 5px;
    height: 24px; padding: 0 8px;
    background: transparent; border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary); cursor: pointer;
    font: var(--font-size-xs) var(--font-ui-sans);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .pick-btn:hover:not(:disabled), .pick-btn.open { background: var(--bg-hover); color: var(--text-primary); }
  .pick-btn:disabled { opacity: 0.4; cursor: default; }

  .cmd-preview { margin: 2px 0 0; }
  .cmd {
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted);
    background: var(--bg-elevated); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 6px 9px; display: block;
    overflow-x: auto; white-space: nowrap;
  }

  .foot-hint { font-size: var(--font-size-xs); color: var(--text-muted); }
  .footer-actions { display: flex; align-items: center; gap: 8px; }
</style>
