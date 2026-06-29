<!--
  PluginFormModal — thin wrapper around `<FormNodeRenderer>` that adds the
  Modal chrome, the form-level submit / cancel pipeline, validation pattern
  enforcement, and inline plugin CSS injection.

  Aggregated settings panels (multi-plugin sections + tree nav) live in
  ContributableModal — this component handles the single-plugin form case.
  Keep it thin: anything that touches an individual node's markup belongs in
  the renderer.

  ── Studio-shaped chrome ────────────────────────────────────────────────
  When `form.header`, `form.activity_bar`, `form.sidecars`, `form.footer`,
  or `form.state_block` is set, the plain ModalHeader + Submit/Cancel
  chrome is replaced by the corresponding Studio-shaped zones (see
  docs/studio-modal-plugin-design.md). Each zone hosts its own
  FormNodeRenderer instance; values across regions are aggregated at
  submit time and collisions on `name` warn (last-write-wins).

  Cross-region `show_if` is NOT supported in v1 — each renderer keeps its
  own field values internally. Forms that need cross-region reactivity
  should compose a single body with `tree_layout` instead.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import Modal       from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button      from '$lib/components/shared/ui/Button.svelte';
  import StateBlock  from '$lib/components/shared/ui/StateBlock.svelte';
  import Spinner     from '$lib/components/shared/ui/Spinner.svelte';
  import ExperimentalBadge from '$lib/components/shared/ui/ExperimentalBadge.svelte';
  import BrandIcon   from '$lib/components/shared/internal/BrandIcon.svelte';
  import FormNodeRenderer, { type WizardInfo } from './FormNodeRenderer.svelte';
  import PluginActivityBar from './PluginActivityBar.svelte';
  import { Send, ChevronLeft, ChevronRight, Loader2, AlertCircle } from 'lucide-svelte';
  import { PLUGIN_ICONS } from '$lib/utils/plugin-icons';
  import { tooltip } from '$lib/actions/tooltip';
  import type {
    PluginFormConfig, FormNode, FormActivityBarItem, FormSidecarCfg,
  } from '$lib/types/plugin';
  import { firePluginAction } from '$lib/ipc/plugin';
  import { uiStore }          from '$lib/stores/ui.svelte';

  let {
    form,
    onClose,
  }: { form: PluginFormConfig; onClose: () => void } = $props();

  // ── Renderer refs ───────────────────────────────────────────────────────
  // The body renderer drives wizardInfo + the default `replace` handler.
  // Sidecar / header-zone / footer-zone renderers each own their slice and
  // contribute values at submit time.
  let bodyRenderer: ReturnType<typeof FormNodeRenderer> | null = $state(null);
  let wizardInfo = $state<WizardInfo>({
    has: false, isFirst: true, isLast: true, nextLabel: 'Next', backLabel: 'Back',
  });

  type ZoneRef = ReturnType<typeof FormNodeRenderer> | null;
  let sidecarRefs   = $state<Record<string, ZoneRef>>({});
  let headerLeftRef:    ZoneRef = $state(null);
  let headerCentreRef:  ZoneRef = $state(null);
  let headerRightRef:   ZoneRef = $state(null);
  let footerStatusRef:  ZoneRef = $state(null);
  let footerCenterRef:  ZoneRef = $state(null);
  let footerRightRef:   ZoneRef = $state(null);

  // ── Validation ──────────────────────────────────────────────────────────
  // Pattern rules collected from the BODY node tree only. Cross-region
  // pattern validation is out of scope for v1 (body covers ≥99% of real cases).
  interface ValidationRule { pattern?: string; pattern_hint?: string; }
  let validationRules: Record<string, ValidationRule> = $state({});
  let validationErrors = $state<Record<string, string>>({});

  function collectValidation(ns: FormNode[]): Record<string, ValidationRule> {
    const acc: Record<string, ValidationRule> = {};
    // Defensive: plugins authored in Lua sometimes ship `children = {}` for
    // never-rendered sub-trees (e.g. `tabs.tabs[].children = {}` on a
    // `strip_only` tabs). Lua empty tables serialise as JSON `{}` (object,
    // not array), which is truthy enough to skip the `?? []` fallback and
    // then chokes the `for (const x of …)` loop with "is not iterable".
    // Normalising at the walk entry keeps every recursion guarded uniformly.
    const arr = (v: unknown): FormNode[] => (Array.isArray(v) ? (v as FormNode[]) : []);
    function walk(list: unknown) {
      for (const n of arr(list)) {
        if (n.type === 'text' && (n as any).pattern) {
          acc[(n as any).name] = {
            pattern:      (n as any).pattern,
            pattern_hint: (n as any).pattern_hint,
          };
        }
        if (n.type === 'switch') {
          const s = n as any;
          for (const arr2 of Object.values(s.cases ?? {})) walk(arr2);
          if (s.default) walk(s.default);
          continue;
        }
        if (n.type === 'tabs') {
          for (const t of arr((n as any).tabs)) walk((t as any).children);
          continue;
        }
        if (n.type === 'wizard') {
          for (const s of arr((n as any).steps)) walk((s as any).children);
          continue;
        }
        if ('children' in n) walk((n as any).children);
      }
    }
    walk(ns);
    return acc;
  }
  // svelte-ignore state_referenced_locally
  validationRules = collectValidation(form.nodes);

  // ── Custom CSS injection ────────────────────────────────────────────────
  onMount(() => {
    if (!form.css) return;
    const el = document.createElement('style');
    el.textContent = form.css;
    el.dataset.arborPlugin = form.plugin_name;
    document.head.appendChild(el);
    return () => el.remove();
  });

  // ── State-block + loading overlay state ─────────────────────────────────
  // svelte-ignore state_referenced_locally
  let isLoading = $state(!!form.loading);
  // svelte-ignore state_referenced_locally
  let loadingLabel = $state<string>(form.loading_label ?? 'Loading…');

  // Active state-block override (takes precedence over body when set).
  // Three mutually-exclusive states: loading | error | empty | null.
  // svelte-ignore state_referenced_locally
  let stateBlockKind = $state<'loading' | 'error' | 'empty' | null>(
    form.state_block?.loading ? 'loading'
    : form.state_block?.error ? 'error'
    : form.state_block?.empty ? 'empty'
    : null
  );
  // svelte-ignore state_referenced_locally
  let stateBlockLoadingLabel = $state<string>(form.state_block?.loading?.label ?? loadingLabel);
  // svelte-ignore state_referenced_locally
  let stateBlockErrorLabel   = $state<string>(form.state_block?.error?.label ?? '');
  // svelte-ignore state_referenced_locally
  let stateBlockEmpty = $state<{ title?: string; body?: string; cta_label?: string; cta_action?: string } | null>(
    form.state_block?.empty ? { ...form.state_block.empty } : null
  );

  // ── Activity-bar + sidecar state ────────────────────────────────────────
  const sidecarIds = $derived(Object.keys(form.sidecars ?? {}));
  // Sidecars anchor to the right edge by default (RON/JSON convention); a pane
  // can opt into the left edge (`side = "left"`) to sit beside a left-side
  // activity bar — rendered before the main body, bordered on its right.
  const leftSidecarIds  = $derived(sidecarIds.filter(id => form.sidecars?.[id]?.side === 'left'));
  const rightSidecarIds = $derived(sidecarIds.filter(id => form.sidecars?.[id]?.side !== 'left'));
  const activityBar = $derived(form.activity_bar);
  const activityBarSide = $derived(activityBar?.side ?? 'right');

  function pickItems(side: 'left' | 'right'): FormActivityBarItem[] {
    if (!activityBar) return [];
    if (activityBar.side === 'both') {
      return side === 'left'
        ? (activityBar.left_items ?? [])
        : (activityBar.right_items ?? []);
    }
    return activityBar.side === side ? (activityBar.items ?? []) : [];
  }
  const leftItems  = $derived(pickItems('left'));
  const rightItems = $derived(pickItems('right'));

  // Persist + restore the active sidecar id. The `default` field picks the
  // initial value when no stored preference exists; `always_open = true`
  // suppresses the "close to null" toggle behaviour.
  function readStoredSidecar(): string | null {
    const key = activityBar?.storage_key;
    if (!key || typeof window === 'undefined') return null;
    try {
      const v = window.localStorage.getItem(key);
      return v && sidecarIds.includes(v) ? v : null;
    } catch { return null; }
  }
  // svelte-ignore state_referenced_locally
  let activeSidecar = $state<string | null>(
    readStoredSidecar() ?? activityBar?.default ?? null
  );

  function writeStoredSidecar(v: string | null) {
    const key = activityBar?.storage_key;
    if (!key || typeof window === 'undefined') return;
    try {
      if (v === null) window.localStorage.removeItem(key);
      else            window.localStorage.setItem(key, v);
    } catch { /* ignore */ }
  }

  function onActivityBarSelect(id: string) {
    // Activity-bar items are routing-only (Q2 decision). Toggle off only
    // when the bar is not pinned open.
    const next: string | null =
      activeSidecar === id && !activityBar?.always_open ? null : id;
    activeSidecar = next;
    writeStoredSidecar(next);
  }

  // ── Programmatic-update listener (close / loading / sidecar / state_block) ──
  // The body renderer mounts its own listener for value/option/disabled/
  // patch ops; this one covers the modal-chrome-level ops only.
  onMount(() => {
    let unlisten: UnlistenFn | undefined;
    listen<any>('plugin:form-update', (ev) => {
      const p = ev.payload ?? {};
      if (p.plugin_name !== form.plugin_name) return;

      if (p.op === 'close') { onClose(); return; }

      if (p.op === 'replace') {
        const cfg = (p.payload ?? {}) as { loading?: boolean; loading_label?: string };
        if (typeof cfg.loading === 'boolean') isLoading = cfg.loading;
        if (typeof cfg.loading_label === 'string') loadingLabel = cfg.loading_label;
        return;
      }

      if (p.op === 'set_loading') {
        if (typeof p.loading === 'boolean') isLoading = p.loading;
        if (typeof p.label === 'string') loadingLabel = p.label;
        else if (p.loading === false)    loadingLabel = 'Loading…';
        return;
      }

      if (p.op === 'set_sidecar') {
        const id = typeof p.id === 'string' ? p.id : null;
        // Plugin called `set_sidecar(nil)` → close. Plugin set an unknown id
        // → log a warning and ignore (clamping prevents the activity bar
        // from showing nothing as "active" forever).
        if (id !== null && !sidecarIds.includes(id)) {
          // eslint-disable-next-line no-console
          console.warn(`[plugin:${form.plugin_name}] set_sidecar: unknown id "${id}" — known: ${sidecarIds.join(', ')}`);
          return;
        }
        activeSidecar = id;
        writeStoredSidecar(id);
        return;
      }

      if (p.op === 'set_state_block') {
        const name = p.name === null ? null : (typeof p.name === 'string' ? p.name : null);
        if (name === null) { stateBlockKind = null; return; }
        const cfg = (p.cfg ?? {}) as any;
        if (name === 'loading') {
          stateBlockLoadingLabel = typeof cfg.label === 'string' ? cfg.label : 'Loading…';
          stateBlockKind = 'loading';
        } else if (name === 'error') {
          stateBlockErrorLabel = typeof cfg.label === 'string' ? cfg.label : 'Error';
          stateBlockKind = 'error';
        } else if (name === 'empty') {
          stateBlockEmpty = {
            title:      typeof cfg.title      === 'string' ? cfg.title      : undefined,
            body:       typeof cfg.body       === 'string' ? cfg.body       : undefined,
            cta_label:  typeof cfg.cta_label  === 'string' ? cfg.cta_label  : undefined,
            cta_action: typeof cfg.cta_action === 'string' ? cfg.cta_action : undefined,
          };
          stateBlockKind = 'empty';
        }
      }
    }).then(u => { unlisten = u; });
    return () => { unlisten?.(); };
  });

  // ── Submit / cancel ─────────────────────────────────────────────────────
  let submitting = $state(false);

  function collectAllRefs(): ZoneRef[] {
    const refs: ZoneRef[] = [bodyRenderer];
    if (headerLeftRef)   refs.push(headerLeftRef);
    if (headerCentreRef) refs.push(headerCentreRef);
    if (headerRightRef)  refs.push(headerRightRef);
    if (footerStatusRef) refs.push(footerStatusRef);
    if (footerCenterRef) refs.push(footerCenterRef);
    if (footerRightRef)  refs.push(footerRightRef);
    for (const id of sidecarIds) {
      const r = sidecarRefs[id];
      if (r) refs.push(r);
    }
    return refs.filter(r => r != null);
  }

  function aggregateValues(): Record<string, unknown> {
    const all: Record<string, unknown> = {};
    const seenOwned = new Set<string>();
    for (const ref of collectAllRefs()) {
      const slice = ref!.getValues() ?? {};
      const owned = new Set(ref!.getOwnedFieldNames() ?? []);
      // First merge the orphan set_value writes (no collision counting).
      // Owned fields then take precedence and participate in collision detection.
      for (const [k, v] of Object.entries(slice)) {
        if (!owned.has(k)) {
          if (!(k in all)) all[k] = v;
          continue;
        }
        if (seenOwned.has(k)) {
          // Owned-vs-owned collision across regions = plugin error per the
          // round-3 decision. Last-write-wins is preserved (the latter ref
          // overwrites) so a value still ships, but warn loudly.
          // eslint-disable-next-line no-console
          console.warn(`[plugin:${form.plugin_name}] name collision across regions: "${k}" — last value wins`);
        }
        seenOwned.add(k);
        all[k] = v;
      }
    }
    return all;
  }

  function buildPayload(): string {
    const values    = aggregateValues();
    const liveState = bodyRenderer?.getLiveState();
    const payload: Record<string, unknown> = { ...values };
    if (liveState !== undefined) payload.state = liveState;
    return JSON.stringify(payload);
  }

  async function handleSubmit() {
    const values = aggregateValues();
    for (const [name, rule] of Object.entries(validationRules)) {
      const v = String(values[name] ?? '');
      if (rule.pattern && v && !new RegExp(rule.pattern).test(v)) {
        validationErrors = {
          ...validationErrors,
          [name]: rule.pattern_hint ?? `${name} format is invalid`,
        };
        return;
      }
    }

    submitting = true;
    let actionFailed = false;
    try {
      await firePluginAction(form.plugin_name, form.submit_action, buildPayload());
    } catch (err) {
      actionFailed = true;
      uiStore.showToast(`Plugin action failed: ${err}`, 'error');
    } finally {
      submitting = false;
      if (!form.keep_open || actionFailed) onClose();
    }
  }

  async function handleCancel() {
    if (form.cancel_action) {
      const liveState = bodyRenderer?.getLiveState();
      try {
        await firePluginAction(form.plugin_name, form.cancel_action,
          liveState ? JSON.stringify({ state: liveState }) : '{}');
      } catch { /* intentional — best-effort */ }
    }
    onClose();
  }

  async function fireEmptyCta() {
    const action = stateBlockEmpty?.cta_action;
    if (!action) return;
    try {
      await firePluginAction(form.plugin_name, action, '{}');
    } catch (err) {
      uiStore.showToast(`Plugin action failed: ${err}`, 'error');
    }
  }

  // ── Renderer callbacks ──────────────────────────────────────────────────
  function onValueChange(name: string, _value: unknown) {
    if (validationErrors[name]) {
      const next = { ...validationErrors };
      delete next[name];
      validationErrors = next;
    }
  }

  function onNodesChange(newNodes: FormNode[], reason: 'replace' | 'patch' = 'replace') {
    validationRules = collectValidation(newNodes);

    if (reason === 'replace') {
      validationErrors = {};
      return;
    }
    let changed = false;
    const next: Record<string, string> = {};
    for (const [k, v] of Object.entries(validationErrors)) {
      if (k in validationRules) next[k] = v;
      else changed = true;
    }
    if (changed) validationErrors = next;
  }

  // ── Header helpers ──────────────────────────────────────────────────────
  const HeaderIconLucide = $derived.by(() => {
    const ic = form.header?.icon;
    if (ic && 'lucide' in ic) return PLUGIN_ICONS[ic.lucide] ?? null;
    return null;
  });

  // ── Sanity warnings (mount-time) ────────────────────────────────────────
  // Surface activity-bar items whose `id` has no matching sidecar — the
  // routing-only contract requires every item to point at a defined pane.
  onMount(() => {
    const allItems = [...leftItems, ...rightItems];
    for (const it of allItems) {
      if ('separator' in it) continue;
      if (!sidecarIds.includes(it.id)) {
        // eslint-disable-next-line no-console
        console.warn(
          `[plugin:${form.plugin_name}] activity_bar item "${it.id}" has no matching sidecar (known: ${sidecarIds.join(', ') || '<none>'})`
        );
      }
    }
  });

  const hasCustomHeader = $derived(!!form.header);
  const hasActivityBar  = $derived(!!activityBar);
  const hasSidecars     = $derived(sidecarIds.length > 0);
  const hasCustomFooter = $derived(!!form.footer && (
    (form.footer.status && form.footer.status.length > 0) ||
    (form.footer.center && form.footer.center.length > 0) ||
    (form.footer.right  && form.footer.right.length  > 0)
  ));

  // Sidecars always-mounted with width animation; only one visible at a time.
  function sidecarWidth(cfg: FormSidecarCfg | undefined): number {
    return cfg?.width ?? 320;
  }
</script>

<Modal
  onClose={handleCancel}
  width={form.width}
  height={form.height}
  padBody={false}
  ariaLabel="Plugin Form"
  showLeftRail={hasActivityBar && (activityBarSide === 'left'  || activityBarSide === 'both') && leftItems.length  > 0}
  showRightRail={hasActivityBar && (activityBarSide === 'right' || activityBarSide === 'both') && rightItems.length > 0}
>
  <!-- Rail snippets are always declared; Modal honours the explicit
       `showLeftRail` / `showRightRail` booleans to decide whether to mount
       the 38px ActivityBar shell. Conditional `{#snippet}` inside `{#if}`
       (the obvious-looking alternative) wasn't reliably picked up as a
       prop in Svelte 5 — items declared inside an active branch sometimes
       arrived as `undefined` on the child component. The explicit boolean
       is the durable contract. -->
  {#snippet leftRail()}
    <PluginActivityBar items={leftItems} activeId={activeSidecar} onSelect={onActivityBarSelect} />
  {/snippet}
  {#snippet rightRail()}
    <PluginActivityBar items={rightItems} activeId={activeSidecar} onSelect={onActivityBarSelect} />
  {/snippet}

  {#snippet header()}
    {#if hasCustomHeader}
      <!-- Studio-shaped header: icon · title · subtitle · meta · left · centre · right -->
      <div class="pf-shdr">
        {#if form.header!.icon}
          {#if HeaderIconLucide}
            {@const Ic = HeaderIconLucide}
            <span class="pf-shdr-icon" aria-hidden="true"><Ic size={18} /></span>
          {:else if 'brand' in form.header!.icon!}
            <span class="pf-shdr-icon" aria-hidden="true">
              <BrandIcon brand={form.header!.icon!.brand} size={18} />
            </span>
          {:else if 'image' in form.header!.icon!}
            <img class="pf-shdr-icon-img" src={form.header!.icon!.image} alt="" width="18" height="18" />
          {/if}
        {/if}
        <span class="pf-shdr-title" use:tooltip={form.header!.tooltip ?? ''}>
          {form.title}
          {#if form.header!.dirty}<span class="pf-shdr-dirty" use:tooltip={'Unsaved changes'}>●</span>{/if}
        </span>
        {#if form.header!.subtitle}
          <span class="pf-shdr-sub">{form.header!.subtitle}</span>
        {/if}
        {#if form.header!.size_meta}
          <span class="pf-shdr-meta">{form.header!.size_meta}</span>
        {/if}
        {#if form.header!.experimental}
          <ExperimentalBadge description={form.header!.experimental.description} />
        {/if}

        {#if form.header!.left && form.header!.left.length > 0}
          <div class="pf-shdr-zone">
            <FormNodeRenderer bind:this={headerLeftRef} pluginName={form.plugin_name}
              nodes={form.header!.left} region="header.left" chrome="inline" {validationErrors}
              disabled={submitting || isLoading}
              {onValueChange} {onClose} />
          </div>
        {/if}
        {#if form.header!.centre && form.header!.centre.length > 0}
          <div class="pf-shdr-zone pf-shdr-centre">
            <FormNodeRenderer bind:this={headerCentreRef} pluginName={form.plugin_name}
              nodes={form.header!.centre} region="header.centre" chrome="inline" {validationErrors}
              disabled={submitting || isLoading}
              {onValueChange} {onClose} />
          </div>
        {/if}
        <div class="pf-shdr-spacer"></div>
        {#if form.header!.right && form.header!.right.length > 0}
          <div class="pf-shdr-zone">
            <FormNodeRenderer bind:this={headerRightRef} pluginName={form.plugin_name}
              nodes={form.header!.right} region="header.right" chrome="inline" {validationErrors}
              disabled={submitting || isLoading}
              {onValueChange} {onClose} />
          </div>
        {/if}
        <button class="mac-close-btn pf-shdr-close" onclick={handleCancel} aria-label="Close"
                use:tooltip={'Close'}></button>
      </div>
    {:else}
      <ModalHeader onClose={handleCancel}>
        <span class="pf-plugin-tag">{form.plugin_name}</span>
        <span class="pf-title-text">{form.title}</span>
      </ModalHeader>
    {/if}
  {/snippet}

  <!-- Always-mounted sidecar: closing collapses width to 0 (CSS), opening
       transitions back. The inner pinned-width wrapper keeps the content laid
       out at the target size during the animation, so the visual reads as
       "slides in" instead of "content squishes." Field values survive
       close/open because the renderer is never unmounted. `side` decides the
       border edge (left panes border-right, beside a left activity bar). -->
  <div class="pf-modal">
    {#snippet sidecar(id: string, side: 'left' | 'right')}
      {@const cfg = form.sidecars![id]}
      {@const w   = sidecarWidth(cfg)}
      {@const open = activeSidecar === id}
      <aside class="pf-sidecar" class:pf-sidecar-open={open} class:pf-sidecar-left={side === 'left'}
             style="--pf-sw:{w}px">
        <div class="pf-sidecar-inner" style="width:{w}px">
          {#if cfg.title}
            <header class="pf-sidecar-title">{cfg.title}</header>
          {/if}
          <div class="pf-sidecar-body">
            <FormNodeRenderer
              bind:this={sidecarRefs[id]}
              pluginName={form.plugin_name}
              nodes={cfg.children}
              region={`sidecar:${id}`}
              {validationErrors}
              disabled={submitting || isLoading}
              {onValueChange}
              {onClose}
            />
          </div>
        </div>
      </aside>
    {/snippet}

    <!-- The body-row + sidecars frame is rendered for EVERY state — loading
         / error / empty / populated. Without this, the activity bar (which
         lives outside the body via Modal's rightRail) stays clickable in
         a state_block but the sidecars don't mount, leaving an `active`
         icon on the rail with nothing actually shown. Plugins author
         sidecar children that gracefully handle the no-document case
         (e.g. a state_block "Open a file to enable filtering"), and the
         active sidecar slides in over the state-fill the same way it
         would over a populated body. -->
    <div class="pf-body-row" class:pf-body-row-sidecars={hasSidecars}>
      {#if hasSidecars}
        {#each leftSidecarIds as id (id)}
          {@render sidecar(id, 'left')}
        {/each}
      {/if}
      <div class="pf-body-main">
        {#if stateBlockKind === 'loading'}
          <div class="pf-state-fill">
            <StateBlock tone="loading">
              {#snippet spinner()}<Spinner size="lg" label={stateBlockLoadingLabel} />{/snippet}
            </StateBlock>
          </div>
        {:else if stateBlockKind === 'error'}
          <div class="pf-state-fill">
            <StateBlock tone="error" label={stateBlockErrorLabel}>
              {#snippet icon()}<AlertCircle size={18} />{/snippet}
            </StateBlock>
          </div>
        {:else if stateBlockKind === 'empty'}
          <div class="pf-state-fill">
            <div class="pf-empty">
              {#if stateBlockEmpty?.title}
                <div class="pf-empty-title">{stateBlockEmpty.title}</div>
              {/if}
              {#if stateBlockEmpty?.body}
                <p class="pf-empty-body">{stateBlockEmpty.body}</p>
              {/if}
              {#if stateBlockEmpty?.cta_label && stateBlockEmpty?.cta_action}
                <div class="pf-empty-cta">
                  <Button variant="primary" onclick={fireEmptyCta}>{stateBlockEmpty.cta_label}</Button>
                </div>
              {/if}
            </div>
          </div>
        {:else}
          {#if form.description && !form.sidebar}
            <p class="pf-desc">{form.description}</p>
          {/if}
          <FormNodeRenderer
            bind:this={bodyRenderer}
            bind:wizardInfo
            pluginName={form.plugin_name}
            nodes={form.nodes}
            initialState={form.state}
            sidebarLayout={!!form.sidebar}
            {validationErrors}
            disabled={submitting || isLoading}
            {onValueChange}
            {onNodesChange}
            {onClose}
          />
        {/if}
      </div>

      {#if hasSidecars}
        {#each rightSidecarIds as id (id)}
          {@render sidecar(id, 'right')}
        {/each}
      {/if}
    </div>

    {#if isLoading}
      <!-- Translucent loading overlay above the body (legacy `form.loading`
           channel — independent of state_block which fully replaces the body). -->
      <div class="pf-loading" role="status" aria-live="polite">
        <div class="pf-loading-card">
          <Loader2 size={20} class="pf-spin" />
          <span class="pf-loading-text">{loadingLabel}</span>
        </div>
      </div>
    {/if}
  </div>

  {#snippet footer()}
    {#if hasCustomFooter}
      <div class="pf-footer-row">
        {#if form.footer!.status && form.footer!.status.length > 0}
          <div class="pf-footer-zone pf-footer-status">
            <FormNodeRenderer bind:this={footerStatusRef} pluginName={form.plugin_name}
              nodes={form.footer!.status} region="footer.status" chrome="inline" {validationErrors}
              disabled={submitting || isLoading}
              {onValueChange} {onClose} />
          </div>
        {/if}
        <div class="pf-footer-spacer"></div>
        {#if form.footer!.center && form.footer!.center.length > 0}
          <div class="pf-footer-zone">
            <FormNodeRenderer bind:this={footerCenterRef} pluginName={form.plugin_name}
              nodes={form.footer!.center} region="footer.center" chrome="inline" {validationErrors}
              disabled={submitting || isLoading}
              {onValueChange} {onClose} />
          </div>
        {/if}
        {#if form.footer!.right && form.footer!.right.length > 0}
          <div class="pf-footer-zone">
            <FormNodeRenderer bind:this={footerRightRef} pluginName={form.plugin_name}
              nodes={form.footer!.right} region="footer.right" chrome="inline" {validationErrors}
              disabled={submitting || isLoading}
              {onValueChange} {onClose} />
          </div>
        {:else}
          <!-- Default CTAs when `footer.right` is absent — same as no-footer-cfg case. -->
          {#if !form.hide_cancel}
            <Button variant="secondary" onclick={handleCancel} disabled={submitting}>
              {form.cancel_label ?? 'Cancel'}
            </Button>
          {/if}
          {#if !form.hide_submit}
            <Button variant="primary" onclick={handleSubmit} disabled={submitting}>
              {#snippet iconStart()}<Send size={12} />{/snippet}
              {form.submit_label ?? 'Submit'}
            </Button>
          {/if}
        {/if}
      </div>
    {:else}
      {#if !form.hide_cancel}
        <Button variant="secondary" onclick={handleCancel} disabled={submitting}>
          {form.cancel_label ?? 'Cancel'}
        </Button>
      {/if}

      {#if wizardInfo.has}
        {#if !wizardInfo.isFirst}
          <Button variant="secondary" type="button"
                  onclick={() => bodyRenderer?.wizardBack()} disabled={submitting}>
            {#snippet iconStart()}<ChevronLeft size={12} />{/snippet}
            {wizardInfo.backLabel}
          </Button>
        {/if}
        {#if !wizardInfo.isLast}
          <Button variant="primary" type="button"
                  onclick={() => bodyRenderer?.wizardNext()} disabled={submitting}>
            {wizardInfo.nextLabel}
            {#snippet iconEnd()}<ChevronRight size={12} />{/snippet}
          </Button>
        {:else if !form.hide_submit}
          <Button variant="primary" onclick={handleSubmit} disabled={submitting}>
            {#snippet iconStart()}<Send size={12} />{/snippet}
            {form.submit_label ?? 'Submit'}
          </Button>
        {/if}
      {:else if !form.hide_submit}
        <Button variant="primary" onclick={handleSubmit} disabled={submitting}>
          {#snippet iconStart()}<Send size={12} />{/snippet}
          {form.submit_label ?? 'Submit'}
        </Button>
      {/if}
    {/if}
  {/snippet}
</Modal>

<style>
  /* Modal-chrome only: shared pf-* layout styles for the body live in
     FormNodeRenderer.svelte (colocated with the markup that uses them). */

  .pf-modal {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    position: relative;
  }

  .pf-loading {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in srgb, var(--bg-base) 82%, transparent);
    z-index: 10;
  }
  .pf-loading-card {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    padding: 10px 16px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.18);
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
  }
  .pf-loading-text { font-weight: 500; }
  :global(.pf-spin) {
    animation: pf-spin 1s linear infinite;
    color: var(--accent);
  }
  @keyframes pf-spin { to { transform: rotate(360deg); } }

  .pf-plugin-tag {
    font-size: 10px;
    font-weight: 600;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    border-radius: var(--radius-sm);
    padding: 2px 7px;
    flex-shrink: 0;
    letter-spacing: 0.4px;
    text-transform: uppercase;
  }

  .pf-title-text {
    font-size: var(--font-size-md);
    font-weight: 600;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pf-desc {
    padding: 10px 18px 4px;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    margin: 0;
    line-height: 1.55;
    flex-shrink: 0;
    border-bottom: 1px solid var(--border-subtle);
  }

  /* ── State-block fallback fill ──────────────────────────────────────── */
  .pf-state-fill {
    display: flex;
    flex: 1;
    min-height: 0;
    align-items: center;
    justify-content: center;
    padding: 32px;
  }
  .pf-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    max-width: 480px;
    text-align: center;
  }
  .pf-empty-title {
    font-size: var(--font-size-md);
    font-weight: 600;
    color: var(--text-primary);
  }
  .pf-empty-body {
    margin: 0;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    line-height: 1.55;
  }
  .pf-empty-cta { margin-top: 4px; }

  /* ── Body row with sidecars ─────────────────────────────────────────── */
  .pf-body-row {
    display: flex;
    flex-direction: row;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .pf-body-main {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* Sidecars: always mounted (children preserved across open/close), width
     transitions to 0 when closed. The inner wrapper stays at its full target
     width so child content doesn't squish — the parent's overflow:hidden
     crops it as the width animates. Mirror of StudioModal's slide pattern. */
  .pf-sidecar {
    flex-shrink: 0;
    width: 0;
    overflow: hidden;
    transition: width 220ms cubic-bezier(0.2, 0.7, 0.2, 1);
    border-left: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
  }
  .pf-sidecar-open { width: var(--pf-sw, 320px); }
  /* Left-anchored pane sits before the main body, so its divider is on the
     right edge (towards the body) rather than the default left. */
  .pf-sidecar-left {
    border-left: none;
    border-right: 1px solid var(--border-subtle);
  }
  .pf-sidecar-inner {
    height: 100%;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .pf-sidecar-title {
    flex-shrink: 0;
    padding: 8px 12px;
    font-size: var(--font-size-xs);
    font-weight: 600;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    color: var(--text-secondary);
    border-bottom: 1px solid var(--border-subtle);
  }
  .pf-sidecar-body {
    flex: 1;
    min-height: 0;
    /* Inner FormNodeRenderer's .pf-body already handles vertical scroll.
       Cut horizontal here so rich content (kbd chips, tables) is clipped
       instead of producing a second scrollbar on the pane. */
    overflow: hidden;
  }

  /* ── Studio-shaped header ───────────────────────────────────────────── */
  .pf-shdr {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: 1;
    min-width: 0;
  }
  .pf-shdr-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .pf-shdr-icon-img {
    width: 18px;
    height: 18px;
    object-fit: contain;
    flex-shrink: 0;
  }
  .pf-shdr-title {
    font-size: var(--font-size-md);
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 0;
    max-width: 320px;
  }
  .pf-shdr-dirty {
    color: var(--accent);
    margin-left: 4px;
    font-size: 10px;
    vertical-align: middle;
  }
  .pf-shdr-sub {
    font-size: var(--font-size-sm);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .pf-shdr-meta {
    font-size: 11px;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .pf-shdr-zone {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }
  .pf-shdr-centre { flex-shrink: 1; min-width: 0; }
  .pf-shdr-spacer { flex: 1; min-width: 4px; }
  .pf-shdr-close { margin-left: 6px; flex-shrink: 0; }

  /* ── Studio-shaped footer ───────────────────────────────────────────── */
  .pf-footer-row {
    display: contents; /* let Modal's footer flex layout drive the children */
  }
  .pf-footer-zone {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }
  .pf-footer-status {
    min-width: 0;
    font-size: 11px;
  }
  .pf-footer-spacer { flex: 1; min-width: 4px; }
</style>
