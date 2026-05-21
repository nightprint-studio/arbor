<!--
  FormNodeVecField — Vec2/Vec3/Vec4/Quat editor used by the Bevy BRP
  inspector and any other plugin that emits a `vec_field` node.

  The change handler fires `vf.action` with `{entity, type_name, path,
  value}` directly via `ctx.firePluginAction` (NOT through the standard
  button-action helper) because vec writes are not transactional and the
  plugin owns the round-trip.
-->
<script lang="ts">
  import TypePill from '$lib/components/shared/ui/TypePill.svelte';
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
  const vVal     = $derived<Record<string, number>>(
    (vf.value && typeof vf.value === 'object') ? vf.value : {}
  );
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
      {@const av = (vVal[axis] ?? 0) as number}
      <div class="pf-vec-axis" data-axis={axis}>
        <span class="pf-vec-axis-label">{axis.toUpperCase()}</span>
        <input
          type="number"
          class="pf-vec-axis-input"
          step="any"
          readonly={vro}
          disabled={ctx.disabled}
          value={av}
          onchange={(e) => {
            if (vro || !vf.action) return;
            const raw = (e.currentTarget as HTMLInputElement).value;
            const num = Number(raw);
            if (!Number.isFinite(num)) return;
            const base = (vf.payload?.base_path ?? '') as string;
            const subPath = vIsArray ? base + '[' + ai + ']' : base + '.' + axis;
            ctx.firePluginAction(ctx.pluginName, vf.action, JSON.stringify({
              entity:    vf.payload?.entity,
              type_name: vf.payload?.type_name,
              path:      subPath,
              value:     num,
            }));
          }}
        />
      </div>
    {/each}
  </div>
  {#if vf.pill}
    <TypePill label={vf.pill} kind={vf.pill_kind ?? vf.pill} />
  {/if}
</div>
