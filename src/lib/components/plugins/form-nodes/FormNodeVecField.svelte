<!--
  FormNodeVecField — Vec2/Vec3/Vec4/Quat editor used by the Bevy BRP
  inspector and any other plugin that emits a `vec_field` node.

  The legacy change handler fires `vf.action` with `{entity, type_name, path,
  value}` directly via `ctx.firePluginAction` (NOT through the standard
  button-action helper) because vec writes are not transactional and the
  plugin owns the round-trip. When the node instead carries a `dispatch`
  target it goes through the scoped channel: `{node_id, slot:'change',
  value:{axis,index,value}}` (and can target a command), tracked per node.
-->
<script lang="ts">
  import { onDestroy } from 'svelte';
  import TypePill from '$lib/components/shared/internal/TypePill.svelte';
  import type { FormNode } from '$lib/types/plugin';
  import type { FormNodeCtx } from './ctx';

  interface Props {
    node: FormNode;
    ctx:  FormNodeCtx;
  }
  let { node, ctx }: Props = $props();

  const vf       = $derived(node as any);
  const vaxes    = $derived<string[]>(
    Array.isArray(vf.axes) && vf.axes.length > 0 ? vf.axes : ['x', 'y', 'z']
  );
  const vro      = $derived<boolean>(!!vf.readonly);
  const vIsArray = $derived<boolean>(!!vf.is_array_origin);
  // ── The lanes, live ─────────────────────────────────────────────────────────
  //
  // The node's `value` is the STARTING point, not the current one. This field does not take
  // part in the form's `values` — it dispatches each lane and the plugin owns the number — so
  // reading the inputs straight off the node meant every lane snapped back the moment anything
  // re-rendered. And something always did: the very handler the drag fires patches some other
  // node, the renderer re-runs, and four sliders you had just moved returned to zero.
  //
  // So the lanes are held here and re-seeded when the plugin actually replaces `value` — which
  // is what loading a preset or randomising does, and what should move the controls.
  let live = $state<Record<string, number>>({});
  $effect(() => {
    const from = (vf.value && typeof vf.value === 'object') ? vf.value : {};
    // Reading `vf.value` is what subscribes this: it re-runs when the plugin patches the node
    // and not when anything else in the tree changes.
    live = { ...from };
  });
  const laneValue = (axis: string): number => live[axis] ?? 0;

  // ── Optional slider per lane ────────────────────────────────────────────────
  // Off unless the plugin asks, because most vec fields are coordinates and a slider needs a
  // range nobody can guess. Where a range IS known — a shader parameter, a normalised weight,
  // a colour channel — dragging is how you find the value, and typing is how you set one you
  // already know. Both, side by side, rather than a choice between them.
  const hasSlider = $derived<boolean>(!!vf.slider);
  const sMin      = $derived<number>(Number.isFinite(vf.min)  ? Number(vf.min)  : 0);
  const sMax      = $derived<number>(Number.isFinite(vf.max)  ? Number(vf.max)  : 1);
  const sStep     = $derived<number>(Number.isFinite(vf.step) ? Number(vf.step) : 0.01);

  // Trailing-edge debounce for the slider, keyed per lane so two axes don't share a timer.
  // Opt-in via `debounce_ms`: a lane whose handler only writes a number wants every event,
  // but one that rebuilds a scene per drag pixel needs to be told how often is enough.
  // The numeric input is never debounced — it commits once, on change.
  const debounceMs = $derived<number>(Number.isFinite(vf.debounce_ms) ? Number(vf.debounce_ms) : 0);
  const laneTimers: Record<string, ReturnType<typeof setTimeout>> = {};

  function emitDebounced(axis: string, ai: number, num: number) {
    if (debounceMs <= 0) { emit(axis, ai, num); return; }
    // The lane moves immediately even though the dispatch waits: debouncing what the plugin
    // hears is the point, debouncing what the user sees is a control that lags its own drag.
    if (Number.isFinite(num)) live[axis] = num;
    if (laneTimers[axis]) clearTimeout(laneTimers[axis]);
    laneTimers[axis] = setTimeout(() => { delete laneTimers[axis]; emit(axis, ai, num); }, debounceMs);
  }

  /** One lane changed. Same two routes the numeric input already uses. */
  function emit(axis: string, ai: number, num: number) {
    if (vro || !Number.isFinite(num)) return;
    live[axis] = num;
    if (vf.dispatch) {
      ctx.handleScopedDispatch(
        vf.id, 'change', vf.dispatch, { axis, index: ai, value: num },
        { stateKeys: vf.scope_state },
      );
      return;
    }
    if (!vf.action) return;
    const base = (vf.payload?.base_path ?? '') as string;
    const subPath = vIsArray ? base + '[' + ai + ']' : base + '.' + axis;
    ctx.firePluginAction(ctx.pluginName, vf.action, JSON.stringify({
      entity:    vf.payload?.entity,
      type_name: vf.payload?.type_name,
      path:      subPath,
      value:     num,
    }));
  }

  onDestroy(() => { for (const t of Object.values(laneTimers)) clearTimeout(t); });
</script>

<div
  class="pf-field pf-field-vec {(node as any).class ?? ''}"
  class:pf-field-compact={vf.compact}
  class:pf-field-highlight={vf.highlight}
  style={(node as any).style}
>
  {#if vf.label}
    <!-- svelte-ignore a11y_label_has_associated_control -->
    <label class="pf-label">{vf.label}</label>
  {/if}
  <div class="pf-vec-axes">
    {#each vaxes as axis, ai (axis)}
      {@const av = laneValue(axis)}
      <div class="pf-vec-axis" class:pf-vec-axis-slider={hasSlider} data-axis={axis}>
        <span class="pf-vec-axis-label">{axis.toUpperCase()}</span>
        {#if hasSlider}
          <!-- `oninput`, not `onchange`: the point of a slider on a live surface is that the
               thing you are looking at moves while you drag. `onchange` would only fire on
               release, which is a worse numeric input. -->
          <input
            type="range"
            class="pf-vec-axis-range"
            min={sMin}
            max={sMax}
            step={sStep}
            disabled={ctx.disabled || vro}
            value={av}
            oninput={(e) => emitDebounced(axis, ai, Number((e.currentTarget as HTMLInputElement).value))}
          />
        {/if}
        <input
          type="number"
          class="pf-vec-axis-input"
          step={hasSlider ? sStep : 'any'}
          readonly={vro}
          disabled={ctx.disabled}
          value={av}
          onchange={(e) => emit(axis, ai, Number((e.currentTarget as HTMLInputElement).value))}
        />
      </div>
    {/each}
  </div>
  {#if vf.pill}
    <TypePill label={vf.pill} kind={vf.pill_kind ?? vf.pill} />
  {/if}
</div>
