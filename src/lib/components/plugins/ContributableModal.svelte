<!--
  ContributableModal — Phase 2 primitive.

  A modal whose content is AGGREGATED from cross-plugin contributions, in
  contrast to PluginFormModal which shows a closed form owned by a single
  plugin.

  Shape (tree_nav layout):
    ┌─────────────┬────────────────────────┐
    │ Category A  │ Section 1 ............ │  ← FormNodeRenderer
    │ Category B  │ Section 2 ............ │  ← FormNodeRenderer
    │ Category C  │ ......................│
    └─────────────┴────────────────────────┘
                  [Cancel]  [Save]

  Categories are read from `<plugin>::<container_id>:category` (one entry =
  one nav row). Sections are read from `<plugin>::<container_id>:section`
  and filtered by `payload.category === selectedCategoryId`.

  Save semantics — PARALLEL best-effort (Promise.allSettled, NOT atomic):
   1. For every visible section with an `on_save` action, fire
      `firePluginAction(sec.plugin, sec.on_save, slice)` in parallel — the
      slice is *unprefixed* so the contributing plugin sees the field names
      it originally declared.
   2. Aggregate failures into a single toast.
   3. If the host container has its own `on_save`, fire it with the FULL
      namespaced state `{ sections: { [plugin]: { [field]: value } } }`,
      again with the `<plugin>::` prefix stripped per bucket.

  Phase 5 — the backend prefixes every form-DSL field name with
  `<contributing-plugin>::` before storing the section payload (so two
  plugins using the same `name` don't collide). We strip the prefix here
  before fanning the values back out, so plugin code stays oblivious.
-->
<script lang="ts">
  import Modal       from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button      from '$lib/components/shared/ui/Button.svelte';
  import { Send }    from 'lucide-svelte';
  import PluginIcon         from '$lib/components/plugins/PluginIcon.svelte';
  import FormNodeRenderer   from './FormNodeRenderer.svelte';
  import { containerStore }    from '$lib/stores/container.svelte';
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import { pluginStore }       from '$lib/stores/plugin.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { firePluginAction }  from '$lib/ipc/plugin';
  import { uiStore }           from '$lib/stores/ui.svelte';
  import type { FormNode } from '$lib/types/plugin';

  let { containerId, onClose }: { containerId: string; onClose: () => void } = $props();

  // Plugin-supplied dimensions reference a 1920×1080 viewport and scale
  // linearly with the actual window. So `width = "960px"` declared by the
  // plugin renders as `50vw` (960/1920 = 50%) on any window size. Values
  // already expressed in viewport units / percentages / functions
  // (`vw`, `vh`, `%`, `min(...)`, …) are passed through unchanged.
  const REF_W = 1920;
  const REF_H = 1080;
  function scaleDim(val: string | undefined, fallbackPx: number, axis: 'w' | 'h'): string {
    const ref  = axis === 'w' ? REF_W : REF_H;
    const unit = axis === 'w' ? 'vw'  : 'vh';
    const raw  = val ?? `${fallbackPx}px`;
    const m = /^\s*([0-9]+(?:\.[0-9]+)?)px\s*$/.exec(raw);
    if (!m) return raw;
    const px = parseFloat(m[1]);
    return `${(px / ref) * 100}${unit}`;
  }

  // ── Container definition ─────────────────────────────────────────────────
  // Falls back to a placeholder so the template can render before the store
  // has the def cached (the store fetches asynchronously).
  const def = $derived(containerStore.getDef(containerId));
  const ownerPlugin = $derived(containerId.split('::')[0]);
  const categoryPoint = $derived(def?.category_point ?? `${containerId}:category`);
  const sectionPoint  = $derived(def?.section_point  ?? `${containerId}:section`);

  // ── Pre-open hook ────────────────────────────────────────────────────────
  // Fired ONCE per modal open. Lets the host re-contribute its categories /
  // sections with fresh state. Categories/sections are reactive $derived, so
  // they re-render automatically as contributions arrive.
  let onLoadFiredFor = $state<string | null>(null);
  $effect(() => {
    if (!def || onLoadFiredFor === containerId) return;
    onLoadFiredFor = containerId;
    if (def.on_load && ownerPlugin) {
      firePluginAction(ownerPlugin, def.on_load, '{}')
        .catch(err => console.warn(`[ContributableModal] on_load failed: ${err}`));
    }
  });

  // ── Categories (nav) ─────────────────────────────────────────────────────
  interface Category { plugin: string; id: string; label: string; icon?: string; description?: string; priority: number; }

  const categories = $derived.by<Category[]>(() => {
    const point = categoryPoint;
    return contributionStore.forPoint(point)
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
      .map(c => {
        const p = c.payload as { label?: string; icon?: string; description?: string };
        return {
          plugin:      c.plugin_name,
          id:          c.item_id,
          label:       p.label ?? c.item_id,
          icon:        p.icon,
          description: p.description,
          priority:    c.priority ?? 100,
        };
      })
      .sort((a, b) => a.priority - b.priority || a.label.localeCompare(b.label));
  });

  let selectedCategory = $state<string | null>(null);

  // Auto-select the first category once the list is known. Done with an
  // $effect so the selection survives re-renders without flicker.
  $effect(() => {
    if (selectedCategory == null && categories.length > 0) {
      selectedCategory = categories[0].id;
    }
  });

  // ── Sections ─────────────────────────────────────────────────────────────
  interface Section {
    plugin:    string;
    itemId:    string;
    category?: string;
    label?:    string;
    icon?:     string;
    nodes:     FormNode[];
    on_save?:  string;
    state?:    Record<string, unknown>;
    priority:  number;
  }

  const allSections = $derived.by<Section[]>(() => {
    const point = sectionPoint;
    return contributionStore.forPoint(point)
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
      .map(c => {
        const p = c.payload as {
          category?: string; label?: string; icon?: string;
          nodes?: FormNode[]; on_save?: string; state?: Record<string, unknown>;
        };
        return {
          plugin:   c.plugin_name,
          itemId:   c.item_id,
          category: p.category,
          label:    p.label,
          icon:     p.icon,
          nodes:    Array.isArray(p.nodes) ? p.nodes : [],
          on_save:  p.on_save,
          state:    p.state,
          priority: c.priority ?? 100,
        };
      })
      .sort((a, b) => a.priority - b.priority || (a.label ?? '').localeCompare(b.label ?? ''));
  });

  const visibleSections = $derived(
    selectedCategory == null
      ? allSections
      : allSections.filter(s => !s.category || s.category === selectedCategory)
  );

  // ── Renderer refs (one per visible section, keyed by `${plugin}::${itemId}`) ──
  // Used at submit time to harvest values from each renderer.
  let renderers = $state<Record<string, ReturnType<typeof FormNodeRenderer> | null>>({});
  function rendererKey(s: Section) { return `${s.plugin}::${s.itemId}`; }

  // ── Save (parallel best-effort) ──────────────────────────────────────────
  let saving = $state(false);

  /**
   * Strip the `<plugin>::` prefix the backend applied to form-DSL field
   * names so the contributing plugin sees its original names. Keys that
   * don't carry the prefix (legacy / non-prefixed) pass through untouched.
   */
  function unprefixValues(plugin: string, values: Record<string, unknown>): Record<string, unknown> {
    const prefix = `${plugin}::`;
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(values)) {
      out[k.startsWith(prefix) ? k.slice(prefix.length) : k] = v;
    }
    return out;
  }

  async function handleSave() {
    if (!def) return;
    saving = true;

    // Gather slice per section + namespaced full state in one pass.
    // Each section's renderer hands back values keyed by the *prefixed*
    // field name (since the backend prefixed them at register time). We
    // unprefix per contributing-plugin bucket before forwarding.
    const namespaced: Record<string, Record<string, unknown>> = {};
    const sectionTasks: Promise<unknown>[] = [];

    for (const s of allSections) {
      const r = renderers[rendererKey(s)];
      const rawValues = r?.getValues() ?? {};
      const values    = unprefixValues(s.plugin, rawValues);
      if (!namespaced[s.plugin]) namespaced[s.plugin] = {};
      Object.assign(namespaced[s.plugin], values);
      if (!s.on_save) continue;
      const liveState = r?.getLiveState();
      const payload: Record<string, unknown> = { ...values };
      if (liveState !== undefined) payload.state = liveState;
      sectionTasks.push(
        firePluginAction(s.plugin, s.on_save, JSON.stringify(payload))
          .catch(err => ({ __failed: true, plugin: s.plugin, label: s.label ?? s.itemId, err })),
      );
    }

    const results = await Promise.allSettled(sectionTasks);
    const failures = results
      .map(r => r.status === 'fulfilled' ? r.value : { __failed: true, err: r.reason })
      .filter((x): x is { __failed: true; plugin?: string; label?: string; err?: unknown } =>
        !!(x && typeof x === 'object' && (x as any).__failed));

    if (failures.length > 0) {
      const summary = failures
        .map(f => f.label ?? f.plugin ?? 'unknown')
        .join(', ');
      uiStore.showToast(
        `${failures.length} section${failures.length === 1 ? '' : 's'} failed to save: ${summary}`,
        'error',
      );
    }

    // Host-level on_save fires with the FULL namespaced state regardless of
    // section-level outcomes (the host can decide what to do with partial
    // data — e.g. refresh dependent UI).
    if (def.on_save && ownerPlugin) {
      try {
        await firePluginAction(ownerPlugin, def.on_save, JSON.stringify({ sections: namespaced }));
      } catch (err) {
        uiStore.showToast(`Host save failed: ${err}`, 'error');
      }
    }

    saving = false;
    if (failures.length === 0) onClose();
  }
</script>

{#if def}
  <Modal
    {onClose}
    width={scaleDim(def.width, 960, 'w')}
    height={scaleDim(def.height, 680, 'h')}
    padBody={false}
    ariaLabel={def.title}
  >
    {#snippet header()}
      <ModalHeader {onClose}>
        <span class="cm-host-tag">{ownerPlugin}</span>
        <span class="cm-title">{def.title}</span>
      </ModalHeader>
    {/snippet}

    <div class="cm-body" class:cm-tree-nav={def.layout === 'tree_nav'}>
      {#if def.layout === 'tree_nav'}
        <nav class="cm-nav">
          {#each categories as cat (cat.plugin + ':' + cat.id)}
            <button
              type="button"
              class="cm-nav-item"
              class:cm-nav-active={selectedCategory === cat.id}
              use:tooltip={cat.description ?? cat.label}
              onclick={() => selectedCategory = cat.id}
            >
              {#if cat.icon}<PluginIcon name={cat.icon} size={13} />{/if}
              <span>{cat.label}</span>
            </button>
          {/each}
          {#if categories.length === 0}
            <div class="cm-nav-empty">No categories registered</div>
          {/if}
        </nav>
      {/if}

      <div class="cm-content">
        {#each visibleSections as s (rendererKey(s))}
          <section class="cm-section">
            {#if s.label}
              <header class="cm-section-header">
                {#if s.icon}<PluginIcon name={s.icon} size={13} />{/if}
                <span>{s.label}</span>
                <span class="cm-section-tag">{s.plugin}</span>
              </header>
            {/if}
            <div class="cm-section-body">
              <FormNodeRenderer
                bind:this={renderers[rendererKey(s)]}
                pluginName={s.plugin}
                nodes={s.nodes}
                initialState={s.state}
                disabled={saving}
                onClose={() => onClose()}
              />
            </div>
          </section>
        {/each}
        {#if visibleSections.length === 0}
          <div class="cm-empty">No sections in this category</div>
        {/if}
      </div>
    </div>

    {#snippet footer()}
      <Button variant="secondary" onclick={onClose} disabled={saving}>
        {def.cancel_label ?? 'Cancel'}
      </Button>
      <Button variant="primary" onclick={handleSave} disabled={saving}>
        {#snippet iconStart()}<Send size={12} />{/snippet}
        {def.submit_label ?? 'Save'}
      </Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .cm-host-tag {
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
  .cm-title {
    font-size: var(--font-size-md);
    font-weight: 600;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Override the Modal's bg-base body card — we render two side-by-side
     cards (nav + content) with their own bg-base + radius, so the
     enclosing body chrome must blend with the outer modal chrome.
     `!important` because Modal.svelte's `.modal-body.no-pad` rule has
     equal specificity and would otherwise cascade-win depending on load order. */
  :global(.modal-body:has(> .cm-body)) {
    background: transparent !important;
    border: none !important;
    border-radius: 0 !important;
    margin: 0 !important;
    padding: 6px 8px 8px !important;
    overflow: hidden !important;
    display: flex !important;
    flex-direction: column !important;
  }

  .cm-body {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    gap: 8px;
  }
  .cm-body.cm-tree-nav {
    flex-direction: row;
  }

  .cm-nav {
    width: 220px;
    flex-shrink: 0;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    overflow-y: auto;
    padding: 6px 4px;
  }
  .cm-nav-item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 6px 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    cursor: pointer;
    text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .cm-nav-item:hover { background: var(--bg-hover); color: var(--text-primary); }
  .cm-nav-active {
    background: var(--accent-subtle);
    color: var(--accent);
  }
  .cm-nav-empty {
    padding: 14px 10px;
    color: var(--text-disabled);
    font-size: 11px;
    font-style: italic;
  }

  /* Content card — a single bg-base scroll surface. The cards inside
     (`.cm-section`) sit on top of it as bg-elevated panels, mirroring
     the IntelliJ Settings rhythm: dark base canvas, lighter cards.
     `min-height: 0` is required so the flex parent doesn't push the
     content past the modal's fixed height — that's what was killing
     overflow scroll. */
  .cm-content {
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
    padding: 14px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  /* Section card — bg-elevated to pop on the bg-base canvas. Header has
     a bottom border for separation; sections without a header collapse
     to just their padded body. `flex-shrink: 0` keeps each card at its
     natural height so the parent's overflow-y kicks in instead of the
     flex container shrinking everything to fit. */
  .cm-section {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
  }

  .cm-section-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-primary);
    font-weight: 600;
    font-size: var(--font-size-sm);
    flex-shrink: 0;
  }
  .cm-section-tag {
    margin-left: auto;
    font-size: 10px;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    letter-spacing: 0.3px;
  }

  /* Body padding lives on the section, not on the renderer (so the
     renderer stays chrome-free and reusable in non-card contexts). */
  .cm-section-body {
    padding: 14px 16px;
  }

  .cm-empty {
    color: var(--text-muted);
    font-size: var(--font-size-sm);
    padding: 24px;
    text-align: center;
  }
</style>
