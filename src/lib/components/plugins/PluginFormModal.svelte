<!--
  PluginFormModal — thin wrapper around `<FormNodeRenderer>` that adds the
  Modal chrome, the form-level submit / cancel pipeline, validation pattern
  enforcement, and inline plugin CSS injection.

  Aggregated settings panels (multi-plugin sections + tree nav) live in
  ContributableModal — this component handles the single-plugin form case.
  Keep it thin: anything that touches an individual node's markup belongs in
  the renderer.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import Modal       from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button      from '$lib/components/shared/ui/Button.svelte';
  import FormNodeRenderer, { type WizardInfo } from './FormNodeRenderer.svelte';
  import { Send, ChevronLeft, ChevronRight, Loader2 } from 'lucide-svelte';
  import type { PluginFormConfig, FormNode } from '$lib/types/plugin';
  import { firePluginAction } from '$lib/ipc/plugin';
  import { uiStore }          from '$lib/stores/ui.svelte';

  let {
    form,
    onClose,
  }: { form: PluginFormConfig; onClose: () => void } = $props();

  // ── Renderer ref + bindable wizard summary ───────────────────────────────
  let renderer: ReturnType<typeof FormNodeRenderer> | null = $state(null);
  let wizardInfo = $state<WizardInfo>({
    has: false, isFirst: true, isLast: true, nextLabel: 'Next', backLabel: 'Back',
  });

  // ── Validation ────────────────────────────────────────────────────────────
  // Pattern rules collected from the node tree. Re-collected when the
  // renderer signals a `replace` happened.
  interface ValidationRule { pattern?: string; pattern_hint?: string; }
  let validationRules: Record<string, ValidationRule> = $state({});
  let validationErrors = $state<Record<string, string>>({});

  function collectValidation(ns: FormNode[]): Record<string, ValidationRule> {
    const acc: Record<string, ValidationRule> = {};
    function walk(list: FormNode[]) {
      for (const n of list) {
        if (n.type === 'text' && (n as any).pattern) {
          acc[(n as any).name] = {
            pattern:      (n as any).pattern,
            pattern_hint: (n as any).pattern_hint,
          };
        }
        if (n.type === 'switch') {
          const s = n as any;
          for (const arr of Object.values(s.cases ?? {})) walk(arr as FormNode[]);
          if (s.default) walk(s.default);
          continue;
        }
        if (n.type === 'tabs') {
          for (const t of (n as any).tabs ?? []) walk(t.children ?? []);
          continue;
        }
        if (n.type === 'wizard') {
          for (const s of (n as any).steps ?? []) walk(s.children ?? []);
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

  // ── Custom CSS injection ──────────────────────────────────────────────────
  // Appended to <head> while the modal is mounted, cleaned up on destroy.
  onMount(() => {
    if (!form.css) return;
    const el = document.createElement('style');
    el.textContent = form.css;
    el.dataset.arborPlugin = form.plugin_name;
    document.head.appendChild(el);
    return () => el.remove();
  });

  // ── Loading overlay (form.loading) ────────────────────────────────────────
  // Tracked locally so plugins can toggle it live via
  // `arbor.ui.form.replace({ loading, nodes })` or the focused
  // `arbor.ui.form.set_loading(...)` API. Initial value comes from the
  // form config; the listener below picks up subsequent updates.
  // svelte-ignore state_referenced_locally
  let isLoading = $state(!!form.loading);
  // svelte-ignore state_referenced_locally
  let loadingLabel = $state<string>((form as any).loading_label ?? 'Loading…');

  // ── Programmatic close (arbor.ui.form.close) ──────────────────────────────
  // The renderer already listens to plugin:form-update for per-field ops
  // (set_value, replace, …), but the `close` op needs to fire onClose from
  // here — the renderer doesn't know about modal chrome. We register a
  // separate, narrow listener so the renderer's logic stays untouched.
  // The same listener also picks up the `loading` field on `replace`
  // payloads + the `set_loading` op so the overlay tracks the plugin's
  // fetching state without forcing a full form re-render per progress tick.
  onMount(() => {
    let unlisten: UnlistenFn | undefined;
    listen<any>('plugin:form-update', (ev) => {
      const p = ev.payload ?? {};
      if (p.plugin_name !== form.plugin_name) return;
      if (p.op === 'close') {
        onClose();
        return;
      }
      if (p.op === 'replace') {
        const cfg = (p.payload ?? {}) as { loading?: boolean; loading_label?: string };
        if (typeof cfg.loading === 'boolean') isLoading = cfg.loading;
        if (typeof cfg.loading_label === 'string') loadingLabel = cfg.loading_label;
        return;
      }
      if (p.op === 'set_loading') {
        if (typeof p.loading === 'boolean') isLoading = p.loading;
        if (typeof p.label === 'string') loadingLabel = p.label;
        else if (p.loading === false)    loadingLabel = 'Loading…';  // reset
      }
    }).then(u => { unlisten = u; });
    return () => { unlisten?.(); };
  });

  // ── Submit / cancel ───────────────────────────────────────────────────────
  let submitting = $state(false);

  function buildPayload(): string {
    const values    = renderer?.getValues()    ?? {};
    const liveState = renderer?.getLiveState();
    const payload: Record<string, unknown> = { ...values };
    if (liveState !== undefined) payload.state = liveState;
    return JSON.stringify(payload);
  }

  async function handleSubmit() {
    const values = renderer?.getValues() ?? {};
    // Inline pattern validation before submitting.
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
      // `keep_open` lets the plugin chain a follow-up flow (file picker,
      // confirm dialog, second form) without losing the user's current
      // selections to a premature unmount. The plugin closes us
      // explicitly via arbor.ui.form.close() once that flow finishes.
      // We still close on action failure so a buggy handler can't strand
      // the user inside an undismissable modal.
      if (!form.keep_open || actionFailed) onClose();
    }
  }

  async function handleCancel() {
    if (form.cancel_action) {
      const liveState = renderer?.getLiveState();
      try {
        await firePluginAction(form.plugin_name, form.cancel_action,
          liveState ? JSON.stringify({ state: liveState }) : '{}');
      } catch { /* intentional — best-effort */ }
    }
    onClose();
  }

  // ── Renderer callbacks ────────────────────────────────────────────────────
  // Clear the visible error for a field as soon as the user edits it. The
  // pre-submit pattern check repopulates errors on Submit.
  function onValueChange(name: string, _value: unknown) {
    if (validationErrors[name]) {
      const next = { ...validationErrors };
      delete next[name];
      validationErrors = next;
    }
  }

  // After `arbor.ui.form.replace` rebuilds the renderer's tree, recollect
  // validation rules and drop errors that reference fields that no longer
  // exist.
  function onNodesChange(newNodes: FormNode[]) {
    validationRules = collectValidation(newNodes);
    // Errors for now-missing fields are stale — the renderer can no longer
    // display them so just reset to a clean slate.
    validationErrors = {};
  }
</script>

<!-- Plugin form is ALWAYS rendered after the Plugin Manager modal in AppShell,
     so paint order alone is enough to stack it above. -->
<Modal onClose={handleCancel} width={form.width} height={form.height} padBody={false} ariaLabel="Plugin Form">
  {#snippet header()}
    <ModalHeader onClose={handleCancel}>
      <span class="pf-plugin-tag">{form.plugin_name}</span>
      <span class="pf-title-text">{form.title}</span>
    </ModalHeader>
  {/snippet}

  <div class="pf-modal">
    {#if form.description && !form.sidebar}
      <p class="pf-desc">{form.description}</p>
    {/if}

    <FormNodeRenderer
      bind:this={renderer}
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

    {#if isLoading}
      <!-- Translucent overlay with a centered spinner. Pointer-events:auto
           so the user can't click through to the (stale) form behind it,
           which is the whole point of the busy state. -->
      <div class="pf-loading" role="status" aria-live="polite">
        <div class="pf-loading-card">
          <Loader2 size={20} class="pf-spin" />
          <span class="pf-loading-text">{loadingLabel}</span>
        </div>
      </div>
    {/if}
  </div>

  {#snippet footer()}
    {#if !(form as any).hide_cancel}
      <Button variant="secondary" onclick={handleCancel} disabled={submitting}>
        {form.cancel_label ?? 'Cancel'}
      </Button>
    {/if}

    {#if wizardInfo.has}
      {#if !wizardInfo.isFirst}
        <Button variant="secondary" type="button"
                onclick={() => renderer?.wizardBack()} disabled={submitting}>
          {#snippet iconStart()}<ChevronLeft size={12} />{/snippet}
          {wizardInfo.backLabel}
        </Button>
      {/if}
      {#if !wizardInfo.isLast}
        <Button variant="primary" type="button"
                onclick={() => renderer?.wizardNext()} disabled={submitting}>
          {wizardInfo.nextLabel}
          {#snippet iconEnd()}<ChevronRight size={12} />{/snippet}
        </Button>
      {:else if !(form as any).hide_submit}
        <Button variant="primary" onclick={handleSubmit} disabled={submitting}>
          {#snippet iconStart()}<Send size={12} />{/snippet}
          {form.submit_label ?? 'Submit'}
        </Button>
      {/if}
    {:else if !(form as any).hide_submit}
      <Button variant="primary" onclick={handleSubmit} disabled={submitting}>
        {#snippet iconStart()}<Send size={12} />{/snippet}
        {form.submit_label ?? 'Submit'}
      </Button>
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
    position: relative;  /* anchor for .pf-loading overlay */
  }

  .pf-loading {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    /* `backdrop-filter: blur()` removed — see Modal.svelte for rationale.
       Bumped the base-color opacity to compensate for the lost diffusion. */
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
  .pf-loading-text {
    font-weight: 500;
  }
  :global(.pf-spin) {
    animation: pf-spin 1s linear infinite;
    color: var(--accent);
  }
  @keyframes pf-spin {
    to { transform: rotate(360deg); }
  }

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
</style>
