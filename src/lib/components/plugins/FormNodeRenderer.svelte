<!--
  FormNodeRenderer — pure renderer of a FormNode[] tree.

  Has no opinion about Modal chrome, submit/cancel actions, validation
  patterns, or liveState semantics beyond echoing it back when an inline
  button action fires.

  Two consumers:
    · PluginFormModal     — owns Modal chrome + submit/cancel + validation +
                            form.css injection + Wizard footer.
    · ContributableModal  — aggregates several FormNodeRenderer instances,
                            one per contributed section, and orchestrates
                            parallel saves.

  Public API (via `bind:this`):
    · getValues()    → current Record<string, any>
    · getLiveState() → echoed liveState (Record<string, unknown> | undefined)
    · wizardNext() / wizardBack() — drive the first wizard node
    · wizardInfo (bindable prop) — reflects the active wizard state

  ── Refactor notes (god-objects Phase 2) ────────────────────────────────
  This file is the *dispatcher*: it owns state, listeners, the public API
  and the recursive `renderNode` snippet. Each node type then delegates
  to a focused sub-renderer:
    · FormNodeLayout         — structural / content nodes
    · FormNodeButtons        — button, menu_button, suggest_grid
    · FormNodeCharts         — counter_grid, score_gauge, time_series_chart,
                               data_table, filter_bar
    · FormNodeField          — every value-bearing node (text/select/file/
                               tree/table/kv_list/…) + the leaf `field`
    · FormNodeVecField       — Vec2/Vec3/Vec4/Quat editor
    · FormNodePipelineEditor — wraps PluginPipelineEditor

  All `.pf-*` CSS lives in `form-nodes/form-node-styles.css` (imported
  once below); sub-renderers don't ship styles of their own.
-->
<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';
  import { listen } from '@tauri-apps/api/event';
  import { X as XIcon, ChevronRight } from 'lucide-svelte';
  import { PLUGIN_ICONS } from '$lib/utils/plugin-icons';
  import PluginIcon from '$lib/components/plugins/PluginIcon.svelte';
  import type {
    FormNode, FormFieldKvList, FormCondition,
    FormFieldAutocomplete, FormSelectOption,
  } from '$lib/types/plugin';
  import { firePluginAction } from '$lib/ipc/plugin';
  import { uiStore }          from '$lib/stores/ui.svelte';
  import { tooltip }          from '$lib/actions/tooltip';
  import FilePickerModal      from '$lib/components/shared/FilePickerModal.svelte';

  import type { FormNodeCtx } from './form-nodes/ctx';
  import {
    sectionBody, containerStyle, rowStyle, toArr, treeKey, fmtRange,
    prettify, normalizeOptions, selectLabelOf, selectItemCount,
    buildSelectDropdownItems, wrapSelectChange, multiselectSummary,
  } from './form-nodes/helpers';
  import {
    normalizeNode, collectFields, flattenAll,
    buildCollapseMap, buildActiveTabMap, buildWizardStepMap,
  } from './form-nodes/normalize';

  import FormNodeLayout        from './form-nodes/FormNodeLayout.svelte';
  import FormNodeButtons       from './form-nodes/FormNodeButtons.svelte';
  import FormNodeCharts        from './form-nodes/FormNodeCharts.svelte';
  import FormNodeField         from './form-nodes/FormNodeField.svelte';
  import FormNodeVecField      from './form-nodes/FormNodeVecField.svelte';
  import FormNodePipelineEditor from './form-nodes/FormNodePipelineEditor.svelte';

  import './form-nodes/form-node-styles.css';

  interface Props {
    pluginName:        string;
    nodes:             FormNode[];
    initialValues?:    Record<string, unknown>;
    initialState?:     Record<string, unknown>;
    validationErrors?: Record<string, string>;
    disabled?:         boolean;
    sidebarLayout?:    boolean;
    onValueChange?:    (name: string, value: unknown) => void;
    onNodesChange?:    (newNodes: FormNode[]) => void;
    onClose?:          () => void;
    wizardInfo?:       WizardInfo;
  }

  export interface WizardInfo {
    has:       boolean;
    isFirst:   boolean;
    isLast:    boolean;
    nextLabel: string;
    backLabel: string;
  }

  let {
    pluginName,
    nodes:             initialNodes,
    initialValues,
    initialState,
    validationErrors = {},
    disabled = false,
    sidebarLayout = false,
    onValueChange,
    onNodesChange,
    onClose,
    wizardInfo = $bindable<WizardInfo>({
      has: false, isFirst: true, isLast: true, nextLabel: 'Next', backLabel: 'Back',
    }),
  }: Props = $props();

  // ── Reactive state ─────────────────────────────────────────────────────

  // svelte-ignore state_referenced_locally
  let nodes     = $state<FormNode[]>(initialNodes.map(normalizeNode));
  // svelte-ignore state_referenced_locally
  let liveState = $state<Record<string, unknown> | undefined>(initialState);

  // Typed loosely so bind:value / bind:checked can write freely without casts.
  let values = $state<Record<string, any>>((() => {
    const base = Object.fromEntries(collectFields(nodes));
    if (initialValues) {
      for (const [k, v] of Object.entries(initialValues)) {
        if (k in base) base[k] = v;
      }
    }
    return base;
  })());

  // kv_list values are stored as Record<string,string> but edited as arrays.
  function kvObjToRows(obj: Record<string, string>): { key: string; val: string }[] {
    return Object.entries(obj ?? {}).map(([key, val]) => ({ key, val }));
  }
  function kvRowsToObj(rows: { key: string; val: string }[]): Record<string, string> {
    const obj: Record<string, string> = {};
    for (const { key, val } of rows) { if (key) obj[key] = val; }
    return obj;
  }

  // svelte-ignore state_referenced_locally
  let kvRows = $state<Record<string, { key: string; val: string }[]>>(
    Object.fromEntries(
      nodes
        .flatMap(flattenAll)
        .filter(n => n.type === 'kv_list')
        .map(n => [(n as FormFieldKvList).name, kvObjToRows(values[(n as FormFieldKvList).name] ?? {})])
    )
  );

  $effect(() => {
    for (const [name, rows] of Object.entries(kvRows)) {
      values[name] = kvRowsToObj(rows);
    }
  });

  // svelte-ignore state_referenced_locally
  let collapsedMap   = $state(buildCollapseMap(nodes, sectionBody));
  // svelte-ignore state_referenced_locally
  let activeTabMap   = $state(buildActiveTabMap(nodes));
  let navQueryMap    = $state<Record<string, string>>({});
  // svelte-ignore state_referenced_locally
  let wizardStepMap  = $state(buildWizardStepMap(nodes));

  // tree expansion: `${fieldName}::${nodeValue}` → expanded?
  let treeExpanded = $state<Record<string, boolean>>({});
  (function initTreeExpansion(ns: FormNode[]) {
    const walk = (ns: FormNode[]) => {
      if (!Array.isArray(ns)) return;
      for (const n of ns) {
        if (n.type === 'tree' && (n as any).expanded) {
          const field = (n as any).name;
          const fill = (list: any) => {
            if (!Array.isArray(list)) return;
            for (const t of list) {
              if (Array.isArray(t?.children) && t.children.length) {
                treeExpanded[treeKey(field, t.value)] = true;
                fill(t.children);
              }
            }
          };
          fill((n as any).nodes);
        }
        if ('children' in n && Array.isArray((n as any).children)) walk((n as any).children);
        if (n.type === 'tabs')   for (const t of (n as any).tabs ?? []) walk(t.children ?? []);
        if (n.type === 'wizard') for (const s of (n as any).steps ?? []) walk(s.children ?? []);
        if (n.type === 'switch') {
          const s = n as any;
          for (const arr of Object.values(s.cases ?? {})) walk(arr as FormNode[]);
          if (s.default) walk(s.default);
        }
        if (n.type === 'tree_layout') {
          const t = n as any;
          walk(t.nav_children     ?? []);
          walk(t.content_children ?? []);
        }
      }
    };
    walk(ns);
  })(untrack(() => nodes));

  // tree_layout collapsed: persisted in localStorage.
  const LS_COLLAPSE_PREFIX = 'arbor:tree-layout-collapsed:';
  let treeLayoutCollapsed = $state<Record<string, boolean>>({});

  (function initTreeLayoutCollapsed(ns: FormNode[]) {
    const walk = (list: FormNode[]) => {
      if (!Array.isArray(list)) return;
      for (const n of list) {
        if (n.type === 'tree_layout' && (n as any).nav_collapsible) {
          const id = n.id ?? '';
          if (id) {
            const stored = typeof window !== 'undefined'
              ? window.localStorage.getItem(LS_COLLAPSE_PREFIX + id) : null;
            if (stored === '1') treeLayoutCollapsed[id] = true;
            else if (stored === '0') treeLayoutCollapsed[id] = false;
            else treeLayoutCollapsed[id] = !!(n as any).nav_collapsed_default;
          } else {
            treeLayoutCollapsed['_anon_' + Math.random().toString(36).slice(2, 8)] =
              !!(n as any).nav_collapsed_default;
          }
        }
        if ('children' in n && Array.isArray((n as any).children)) walk((n as any).children);
        if (n.type === 'tabs')   for (const t of (n as any).tabs ?? []) walk(t.children ?? []);
        if (n.type === 'wizard') for (const s of (n as any).steps ?? []) walk(s.children ?? []);
        if (n.type === 'tree_layout') {
          walk((n as any).nav_children ?? []);
          walk((n as any).content_children ?? []);
        }
      }
    };
    walk(ns);
  })(untrack(() => nodes));

  function toggleTreeLayoutCollapsed(id: string) {
    if (!id) return;
    const next = !treeLayoutCollapsed[id];
    treeLayoutCollapsed[id] = next;
    try {
      window.localStorage.setItem(LS_COLLAPSE_PREFIX + id, next ? '1' : '0');
    } catch { /* private mode / quota — ignore */ }
  }

  // Unnamed filter_bars store their state per-node here; named ones write
  // into `values[name]` like any other field.
  let filterBarState = $state<Record<string, { search: string; filters: Record<string, string[]> }>>({});

  // ── Menu portal anchor ─────────────────────────────────────────────────
  // Anchor is pinned via position: fixed so menus escape any clipped parent.
  let openMenuId = $state<string | null>(null);
  let menuAnchor = $state<{ x: number; y: number; w: number } | null>(null);

  function openMenu(e: MouseEvent, menuId: string) {
    const btn = e.currentTarget as HTMLElement | null;
    if (!btn) return;
    const rect = btn.getBoundingClientRect();
    menuAnchor = { x: rect.left, y: rect.bottom + 4, w: rect.width };
    openMenuId = menuId;
  }
  function closeMenu() { openMenuId = null; menuAnchor = null; }
  function isMenuOpen(id: string): boolean { return openMenuId === id; }

  // ── File picker ────────────────────────────────────────────────────────
  let filePickerOpenFor = $state<string | null>(null);
  const filePickerField = $derived.by<any | null>(() => {
    if (!filePickerOpenFor) return null;
    for (const n of nodes.flatMap(flattenAll)) {
      if (n.type === 'file' && (n as any).name === filePickerOpenFor) return n;
    }
    return null;
  });
  function openFilePicker(name: string) { filePickerOpenFor = name; }

  // ── Autocomplete ────────────────────────────────────────────────────────
  let autoOpen       = $state<Record<string, boolean>>({});
  let autoDynOptions = $state<Record<string, { value: string; label: string; group?: string }[]>>({});
  let autoActiveIdx  = $state<Record<string, number>>({});
  const autoDebounce: Record<string, ReturnType<typeof setTimeout>> = {};

  function filterAutocomplete(field: FormFieldAutocomplete, q: string) {
    const overrideOpts = resolvedOptions(field as any);
    const base =
      field.source_action ? (autoDynOptions[field.id] ?? [])
      : ((overrideOpts ?? field.options ?? []) as any[]).map(o =>
          typeof o === 'string' ? { value: o, label: prettify(o) } : o
        );
    const lq = (q ?? '').toLowerCase();
    if (!lq) return base.slice(0, 100);
    const scored = base
      .map((o: any) => {
        const t = `${o.label} ${o.value}`.toLowerCase();
        let s = 0;
        if (t.startsWith(lq))      s = 85;
        else if (t.includes(lq))   s = 55;
        else {
          let idx = 0, ok = true;
          for (const ch of lq) {
            const f = t.indexOf(ch, idx);
            if (f === -1) { ok = false; break; }
            idx = f + 1;
          }
          if (ok) s = 30;
        }
        return { o, s };
      })
      .filter(x => x.s > 0)
      .sort((a, b) => b.s - a.s)
      .slice(0, 100)
      .map(x => x.o);
    return scored;
  }

  function onAutocompleteInput(field: FormFieldAutocomplete) {
    autoOpen[field.id]      = true;
    autoActiveIdx[field.id] = 0;
    if (field.source_action) {
      const prev = autoDebounce[field.id];
      if (prev) clearTimeout(prev);
      const delay = field.debounce_ms ?? 150;
      autoDebounce[field.id] = setTimeout(() => {
        firePluginAction(
          pluginName,
          field.source_action!,
          JSON.stringify({
            id:    field.id,
            query: values[field.name] ?? '',
            state: liveState,
          }),
        ).catch(() => { /* ignore — plugin may not be loaded */ });
      }, delay);
    }
  }

  function pickAutocomplete(field: FormFieldAutocomplete, value: string) {
    values[field.name]  = value;
    autoOpen[field.id]  = false;
  }

  // ── Listeners for plugin-driven updates ─────────────────────────────────

  let unlistenAuto:       (() => void) | null = null;
  let unlistenFormUpdate: (() => void) | null = null;
  onMount(async () => {
    unlistenAuto = await listen<any>('plugin:autocomplete-options', (ev) => {
      const p = ev.payload ?? {};
      if (p.plugin_name !== pluginName) return;
      const opts = Array.isArray(p.options) ? p.options : [];
      autoDynOptions[p.id] = opts.map((o: any) =>
        typeof o === 'string'
          ? { value: o, label: prettify(o) }
          : { value: o.value ?? '', label: o.label ?? String(o.value ?? ''), group: o.group },
      );
    });

    // Dynamic field updates from arbor.ui.form.{setOptions,setDisabled,setValue,replace}
    unlistenFormUpdate = await listen<any>('plugin:form-update', (ev) => {
      const p = ev.payload ?? {};
      if (p.plugin_name !== pluginName) return;

      if (p.op === 'replace') {
        const cfg = (p.payload ?? {}) as {
          nodes?: FormNode[];
          state?: Record<string, unknown>;
          set_values?: Record<string, unknown>;
        };
        const newNodes = (cfg.nodes ?? []).map(normalizeNode);

        const snapshot = { ...values };
        const fresh: Record<string, any> = {};
        for (const [n, v] of collectFields(newNodes)) {
          fresh[n] = n in snapshot ? snapshot[n] : v;
        }
        values = fresh;

        const kvFresh: Record<string, { key: string; val: string }[]> = {};
        for (const n of newNodes.flatMap(flattenAll)) {
          if (n.type === 'kv_list') {
            const name = (n as FormFieldKvList).name;
            kvFresh[name] = kvObjToRows(values[name] ?? {});
          }
        }
        kvRows = kvFresh;

        const newCollapse = buildCollapseMap(newNodes, sectionBody);
        for (const k of Object.keys(newCollapse)) {
          if (k in collapsedMap) newCollapse[k] = collapsedMap[k];
        }
        collapsedMap = newCollapse;

        const newTabs = buildActiveTabMap(newNodes);
        for (const k of Object.keys(newTabs)) {
          if (k in activeTabMap) newTabs[k] = activeTabMap[k];
        }
        activeTabMap = newTabs;

        const newWizard = buildWizardStepMap(newNodes);
        for (const k of Object.keys(newWizard)) {
          if (k in wizardStepMap) newWizard[k] = wizardStepMap[k];
        }
        wizardStepMap = newWizard;

        nodes = newNodes;
        if (cfg.state !== undefined) liveState = cfg.state;

        if (cfg.set_values && typeof cfg.set_values === 'object') {
          for (const [k, v] of Object.entries(cfg.set_values)) {
            values[k] = v;
          }
        }

        onNodesChange?.(newNodes);
        return;
      }

      const name = String(p.name ?? '');
      if (!name) return;
      switch (p.op) {
        case 'set_options': {
          const prev = fieldOverrides[name] ?? {};
          fieldOverrides[name] = { ...prev, options: p.payload };
          // Refresh dynamic autocomplete cache when the field is an
          // autocomplete keyed by id — consumers using id (not name) still work.
          for (const n of nodes.flatMap(flattenAll)) {
            if (n.type === 'autocomplete' && (n as any).name === name) {
              const optsArr = Array.isArray(p.payload) ? p.payload : [];
              autoDynOptions[(n as any).id] = optsArr.map((o: any) =>
                typeof o === 'string'
                  ? { value: o, label: prettify(o) }
                  : { value: o.value ?? '', label: o.label ?? String(o.value ?? ''), group: o.group },
              );
              break;
            }
          }
          break;
        }
        case 'set_disabled': {
          const prev = fieldOverrides[name] ?? {};
          fieldOverrides[name] = { ...prev, disabled: !!p.payload };
          break;
        }
        case 'set_value': {
          values[name] = p.payload;
          notifyChange(name, p.payload);
          break;
        }
      }
    });
  });
  onDestroy(() => {
    if (unlistenAuto)       unlistenAuto();
    if (unlistenFormUpdate) unlistenFormUpdate();
  });

  // ── Dynamic field overrides (set via plugin:form-update) ────────────────
  let fieldOverrides = $state<Record<string, { options?: any; disabled?: boolean }>>({});

  function resolvedOptions(n: any): any {
    const ov = fieldOverrides[n?.name];
    return ov?.options !== undefined ? ov.options : n?.options;
  }
  function resolvedDisabled(n: any): boolean {
    if (disabled) return true;
    const ov = fieldOverrides[n?.name];
    return ov?.disabled !== undefined ? ov.disabled : !!n?.disabled;
  }

  // ── Condition evaluator ─────────────────────────────────────────────────
  function evalCond(c: FormCondition): boolean {
    if ('and' in c) return c.and.every(evalCond);
    if ('or'  in c) return c.or.some(evalCond);
    if ('not' in c) return !evalCond(c.not);
    const v = values[c.field];
    /* eslint-disable eqeqeq */
    if ('eq'  in c && c.eq  !== undefined) return v == c.eq;
    if ('neq' in c && c.neq !== undefined) return v != c.neq;
    /* eslint-enable eqeqeq */
    if ('gt'  in c && c.gt  !== undefined) return (v as number) >  c.gt;
    if ('lt'  in c && c.lt  !== undefined) return (v as number) <  c.lt;
    if ('gte' in c && c.gte !== undefined) return (v as number) >= c.gte;
    if ('lte' in c && c.lte !== undefined) return (v as number) <= c.lte;
    if ('in'        in c && c.in)        return (c.in        as unknown[]).includes(v);
    if ('in_values' in c && (c as any).in_values) return ((c as any).in_values as unknown[]).includes(v);
    if ('nin'       in c && c.nin)       return !(c.nin      as unknown[]).includes(v);
    return true;
  }

  function visible(n: FormNode): boolean {
    return !n.show_if || evalCond(n.show_if);
  }

  // ── Wizard ───────────────────────────────────────────────────────────────
  const rootWizard = $derived.by<FormNode | null>(() => {
    for (const n of nodes) if (n.type === 'wizard') return n;
    return null;
  });

  $effect(() => {
    if (!rootWizard) {
      wizardInfo = {
        has: false, isFirst: true, isLast: true, nextLabel: 'Next', backLabel: 'Back',
      };
      return;
    }
    const w   = rootWizard as any;
    const idx = wizardStepIndex(w);
    wizardInfo = {
      has:       true,
      isFirst:   idx <= 0,
      isLast:    idx >= (w.steps?.length ?? 0) - 1,
      nextLabel: w.next_label ?? 'Next',
      backLabel: w.back_label ?? 'Back',
    };
  });

  export function wizardNext() {
    if (!rootWizard) return;
    const w = rootWizard as any;
    wizardGoTo(w, wizardStepIndex(w) + 1);
  }
  export function wizardBack() {
    if (!rootWizard) return;
    const w = rootWizard as any;
    wizardGoTo(w, wizardStepIndex(w) - 1);
  }

  function wizardStepIndex(w: any): number {
    const cur = wizardStepMap[w.id];
    return Math.max(0, (w.steps ?? []).findIndex((s: any) => s.id === cur));
  }
  function wizardGoTo(w: any, idx: number) {
    const steps = w.steps ?? [];
    const clamped = Math.max(0, Math.min(steps.length - 1, idx));
    wizardStepMap[w.id] = steps[clamped]?.id ?? wizardStepMap[w.id];
  }

  // ── Ctrl+B: toggle modal's tree_layout sidebar ──────────────────────────
  // Listens in the CAPTURE phase and `stopImmediatePropagation` so AppShell's
  // window-level handler doesn't see the event when a modal owns the binding.
  onMount(() => {
    function firstCollapsibleTreeLayoutId(list: FormNode[]): string | null {
      for (const n of list) {
        if (n.type === 'tree_layout' && (n as any).nav_collapsible) return n.id ?? null;
        if ('children' in n && Array.isArray((n as any).children)) {
          const hit = firstCollapsibleTreeLayoutId((n as any).children);
          if (hit) return hit;
        }
        if (n.type === 'tabs') {
          for (const t of (n as any).tabs ?? []) {
            const hit = firstCollapsibleTreeLayoutId(t.children ?? []);
            if (hit) return hit;
          }
        }
        if (n.type === 'tree_layout') {
          const hit = firstCollapsibleTreeLayoutId(
            [...((n as any).nav_children ?? []), ...((n as any).content_children ?? [])]);
          if (hit) return hit;
        }
      }
      return null;
    }

    function onKey(e: KeyboardEvent) {
      const isToggle = (e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey
                        && (e.key === 'b' || e.key === 'B');
      if (!isToggle) return;
      const id = firstCollapsibleTreeLayoutId(nodes);
      if (!id) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      toggleTreeLayoutCollapsed(id);
    }
    window.addEventListener('keydown', onKey, { capture: true });
    return () => window.removeEventListener('keydown', onKey, { capture: true } as AddEventListenerOptions);
  });

  // ── Public API ──────────────────────────────────────────────────────────
  export function getValues(): Record<string, any> {
    return { ...values };
  }
  export function getLiveState(): Record<string, unknown> | undefined {
    return liveState;
  }

  // ── Inline action plumbing ──────────────────────────────────────────────
  let actionPending = $state<string | null>(null);

  function buildActionPayload(extra?: Record<string, unknown>): string {
    const payload: Record<string, unknown> = { ...values };
    if (liveState !== undefined) payload.state = liveState;
    if (extra) Object.assign(payload, extra);
    return JSON.stringify(payload);
  }

  async function handleButtonAction(action: string, closeAfter: boolean, extra?: Record<string, unknown>) {
    if (actionPending) return;
    actionPending = action;
    try {
      await firePluginAction(pluginName, action, buildActionPayload(extra));
      if (closeAfter) onClose?.();
    } catch (err) {
      uiStore.showToast(`Action failed: ${err}`, 'error');
    } finally {
      actionPending = null;
    }
  }

  function notifyChange(name: string, value: unknown) {
    onValueChange?.(name, value);
  }

  // ── Bound select helpers (partial application capturing the live state) ─

  function setSelectValue(fieldName: string, multiple: boolean, value: string) {
    if (multiple) {
      const cur = Array.isArray(values[fieldName]) ? [...(values[fieldName] as string[])] : [];
      const idx = cur.indexOf(value);
      if (idx >= 0) cur.splice(idx, 1); else cur.push(value);
      values[fieldName] = cur;
    } else {
      values[fieldName] = value;
    }
  }
  function fireChangeAction(action: string, extra: Record<string, unknown>) {
    handleButtonAction(action, false, extra);
  }

  // ── Shared rendering context handed to every sub-renderer ───────────────
  // Re-evaluating this lazily would defeat the proxy reactivity; the
  // object itself is stable across renders and Svelte 5's $state ensures
  // mutations on the proxied fields propagate everywhere.
  const ctx: FormNodeCtx = {
    get pluginName()          { return pluginName; },

    get values()              { return values; },
    get fieldOverrides()      { return fieldOverrides; },
    get collapsedMap()        { return collapsedMap; },
    get activeTabMap()        { return activeTabMap; },
    get navQueryMap()         { return navQueryMap; },
    get wizardStepMap()       { return wizardStepMap; },
    get treeExpanded()        { return treeExpanded; },
    get treeLayoutCollapsed() { return treeLayoutCollapsed; },
    get filterBarState()      { return filterBarState; },
    get autoOpen()            { return autoOpen; },
    get autoDynOptions()      { return autoDynOptions; },
    get autoActiveIdx()       { return autoActiveIdx; },
    get kvRows()              { return kvRows; },
    get validationErrors()    { return validationErrors; },
    get disabled()            { return disabled; },
    get actionPending()       { return actionPending; },

    visible,
    evalCond,
    resolvedDisabled,
    resolvedOptions,

    notifyChange,
    handleButtonAction,

    openMenu,
    closeMenu,
    isMenuOpen,

    openFilePicker,

    toggleTreeLayoutCollapsed,
    wizardStepIndex,

    filterAutocomplete,
    onAutocompleteInput,
    pickAutocomplete,

    buildSelectDropdownItems: (raw, fieldName, multiple, current) =>
      buildSelectDropdownItems(raw, fieldName, multiple, current, setSelectValue),
    wrapSelectChange: (items, action) =>
      wrapSelectChange(items, action, fireChangeAction),
    multiselectSummary,
    selectLabelOf,
    selectItemCount,
    normalizeOptions,

    fmtRange,
    treeKey,
    toArr,
    sectionBody,
    containerStyle,
    rowStyle,

    firePluginAction,
  };

  // ── Dispatch by node type ───────────────────────────────────────────────
  // Categorised so we don't list 40 types in the if/else; sub-renderers
  // own their own internal switch when they handle a group.
  const LAYOUT_TYPES = new Set([
    'container', 'row', 'section', 'copy_link', 'icon', 'separator',
    'paragraph', 'alert', 'code', 'label', 'divider', 'info_card',
    'chip_bar', 'form_field', 'tabs', 'tree_layout', 'wizard',
    'card_row', 'cfg_list', 'switch',
  ]);
  const BUTTON_TYPES = new Set(['button', 'menu_button', 'suggest_grid']);
  const CHART_TYPES  = new Set([
    'counter_grid', 'score_gauge', 'time_series_chart',
    'data_table', 'filter_bar',
  ]);
</script>

<!-- ── Body: sidebar mode or flat scrollable column ─────────────────────── -->
{#if sidebarLayout && nodes.length > 0 && nodes[0].type === 'tabs'}
  {@const sidebarNode = nodes[0] as any}
  {@const sidebarTabId = activeTabMap[nodes[0].id!]}
  {@const navQuery = (navQueryMap[nodes[0].id!] ?? '').trim().toLowerCase()}
  {@const tabMatches = (t: any) => {
    if (!navQuery) return true;
    const hay = `${t.label ?? ''}\n${t.group ?? ''}\n${t.meta ?? ''}\n${t.tooltip ?? ''}`.toLowerCase();
    return hay.includes(navQuery);
  }}
  {@const filteredTabs = (sidebarNode.tabs ?? []).filter(tabMatches)}
  {@const visibleActive = filteredTabs.find((t: any) => t.id === sidebarTabId) ?? filteredTabs[0]}
  {@const sidebarActiveTab = visibleActive ?? (sidebarNode.tabs ?? []).find((t: any) => t.id === sidebarTabId)}
  {@const totalTabs = (sidebarNode.tabs ?? []).length}
  {@const hiddenTabs = Math.max(0, totalTabs - filteredTabs.length)}
  {@const navGroups = (() => {
    const groups: { label?: string; items: any[] }[] = [];
    let cur: { label?: string; items: any[] } | null = null;
    for (const t of filteredTabs) {
      if (!cur || cur.label !== (t.group ?? '')) {
        cur = { label: t.group, items: [] };
        groups.push(cur);
      }
      cur.items.push(t);
    }
    return groups;
  })()}
  <div class="pf-sidebar-body">
    <nav class="pf-sidebar-nav" class:pf-sidebar-nav-hassearch={!!sidebarNode.nav_search}>
      {#if sidebarNode.nav_header}
        <div class="pf-nav-headline">{sidebarNode.nav_header}</div>
      {/if}
      {#if sidebarNode.nav_search}
        <div class="pf-nav-search">
          <input
            type="text"
            class="pf-nav-search-input"
            placeholder={sidebarNode.nav_search_placeholder ?? 'Search…'}
            value={navQueryMap[nodes[0].id!] ?? ''}
            oninput={(e) => { navQueryMap[nodes[0].id!] = (e.currentTarget as HTMLInputElement).value; }}
          />
          {#if (navQueryMap[nodes[0].id!] ?? '') !== ''}
            <button
              type="button"
              class="pf-nav-search-clear"
              aria-label="Clear search"
              onclick={() => { navQueryMap[nodes[0].id!] = ''; }}
            >
              <XIcon size={11} />
            </button>
          {/if}
        </div>
      {/if}
      <div class="pf-nav-scroll">
        {#each navGroups as grp}
          {#if grp.label}
            <div class="pf-nav-group">{grp.label}</div>
          {/if}
          {#each grp.items as tab (tab.id)}
            {@const Icon = tab.icon ? PLUGIN_ICONS[tab.icon] : null}
            <button
              class="pf-nav-item"
              class:pf-nav-active={sidebarTabId === tab.id || (!sidebarTabId && sidebarActiveTab?.id === tab.id)}
              type="button"
              use:tooltip={tab.tooltip ?? ''}
              onclick={() => { activeTabMap[nodes[0].id!] = tab.id; }}
            >
              {#if Icon}<Icon size={11} class="pf-nav-icon" />{/if}
              <span class="pf-nav-label-block">
                <span class="pf-nav-label-text">{tab.label}</span>
                {#if tab.meta}<span class="pf-nav-meta">{tab.meta}</span>{/if}
              </span>
              {#if tab.badge}
                <span class="pf-nav-badge" data-kind={tab.badge_kind ?? 'muted'}>{tab.badge}</span>
              {/if}
              <ChevronRight size={11} class="pf-nav-chev" />
            </button>
          {/each}
        {/each}
        {#if filteredTabs.length === 0}
          <div class="pf-nav-empty">No matches</div>
        {/if}
      </div>
      {#if sidebarNode.nav_footer || hiddenTabs > 0}
        <div class="pf-nav-footline">
          {#if sidebarNode.nav_footer}{sidebarNode.nav_footer}{:else}{hiddenTabs} hidden{/if}
        </div>
      {/if}
    </nav>
    <div class="pf-sidebar-content" class:pf-sidebar-content-flush={sidebarActiveTab?.flush}>
      {#if sidebarActiveTab}
        {#each (sidebarActiveTab.children ?? []) as child (child.id)}
          {@render renderNode(child)}
        {/each}
      {/if}
    </div>
  </div>
{:else}
  {@const bodyFlush = nodes.length === 1 && nodes[0].type === 'tree_layout'}
  <div class="pf-body" class:pf-body-flush={bodyFlush}>
    {#each nodes as node (node.id)}
      {@render renderNode(node)}
    {/each}
  </div>
{/if}

<!-- ── Menu-button dropdown portal (position: fixed) ────────────────────── -->
{#if openMenuId && menuAnchor}
  {@const mn = nodes.flatMap(flattenAll).find(x => x.id === openMenuId && x.type === 'menu_button') as any}
  {#if mn}
    <div
      class="pf-menu-backdrop"
      role="presentation"
      onclick={closeMenu}
    ></div>
    <div
      class="pf-menu-dropdown"
      style="left:{menuAnchor.x}px; top:{menuAnchor.y}px; min-width:{Math.max(menuAnchor.w, 180)}px"
      role="menu"
      in:fly={{ y: -4, duration: animStore.dFast, easing: cubicOut }}
    >
      {#each (mn.options ?? []) as opt, i (i)}
        {#if opt.separator || (!opt.label && !opt.action)}
          <div class="pf-menu-sep" role="separator"></div>
        {:else if opt.heading}
          <div class="pf-menu-heading">{opt.label}</div>
        {:else}
          <button
            type="button"
            class="pf-menu-item"
            class:pf-menu-item-danger={opt.variant === 'danger'}
            disabled={!!opt.disabled}
            role="menuitem"
            onclick={() => {
              closeMenu();
              if (opt.action) handleButtonAction(opt.action, false, opt.extra);
            }}
          >
            {#if opt.icon}<span class="pf-menu-icon"><PluginIcon name={opt.icon} size={12} /></span>{:else}<span class="pf-menu-icon-spacer"></span>{/if}
            <span class="pf-menu-label">{opt.label ?? ''}</span>
          </button>
        {/if}
      {/each}
    </div>
  {/if}
{/if}

<!-- ── File picker portal (opened by `file` field nodes) ────────────────── -->
{#if filePickerOpenFor && filePickerField}
  {@const pm = (filePickerField.pick_mode ?? 'file') as 'file' | 'folder' | 'save'}
  <FilePickerModal
    mode={pm}
    title={filePickerField.label ?? (pm === 'folder' ? 'Select Folder' : pm === 'save' ? 'Save As' : 'Select File')}
    extensions={filePickerField.extensions}
    initialPath={typeof values[filePickerOpenFor] === 'string' && values[filePickerOpenFor] ? (values[filePickerOpenFor] as string) : undefined}
    onConfirm={(path) => { values[filePickerOpenFor!] = path; filePickerOpenFor = null; }}
    onCancel={() => { filePickerOpenFor = null; }}
  />
{/if}

<!-- ── Recursive node dispatcher ────────────────────────────────────────── -->
{#snippet renderNode(node: FormNode)}
  {#if visible(node)}
    {#if LAYOUT_TYPES.has(node.type)}
      <FormNodeLayout {node} {ctx} {renderNode} />
    {:else if BUTTON_TYPES.has(node.type)}
      <FormNodeButtons {node} {ctx} />
    {:else if CHART_TYPES.has(node.type)}
      <FormNodeCharts {node} {ctx} />
    {:else if node.type === 'pipeline_editor'}
      <FormNodePipelineEditor {node} {ctx} {renderNode} />
    {:else if (node.type as string) === 'vec_field'}
      <FormNodeVecField {node} {ctx} />
    {:else}
      <FormNodeField {node} {ctx} />
    {/if}
  {/if}
{/snippet}
