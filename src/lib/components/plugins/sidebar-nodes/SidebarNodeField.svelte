<!--
  SidebarNodeField — all value-bearing leaves:
    · field        (bool / number / range / text / select / readonly)
    · color_field  (Bevy Srgba — hex + per-channel alpha)
    · vec_field    (Vec2/3/4 + Quat drag-to-edit)
    · entity_ref   (clickable entity-id pill)

  Mutations fire `node.action` with `{ ...node.payload, value }` for
  scalar `field` nodes, or with per-channel/per-axis paths derived from
  `node.payload.base_path` for the composite editors.
-->
<script lang="ts">
  import { Pin, PinOff } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { SidebarNodeCtx } from './ctx';
  import { packHexColor, parseHexColor } from './helpers';

  interface Props {
    node:  any;
    index: number;
    ctx:   SidebarNodeCtx;
  }
  let { node: n, index: i, ctx }: Props = $props();
</script>

{#if n.type === 'field'}
  {@const fid    = ctx.fieldKey(n, i)}
  {@const fkind  = n.kind ?? (typeof n.value === 'boolean' ? 'bool'
                            : typeof n.value === 'number'  ? 'number'
                            : 'text')}
  {@const fval   = ctx.fieldValue(fid, n.value)}
  {@const fro    = !!n.readonly}
  <div class="node-field" class:field-readonly={fro} class:field-highlight={!!n.highlight}>
    {#if n.label}
      <label class="field-label" for={`pf-side-${fid}`}>{n.label}</label>
    {/if}
    {#if fkind === 'bool' || fkind === 'checkbox'}
      <input
        id={`pf-side-${fid}`}
        class="field-checkbox"
        type="checkbox"
        disabled={fro}
        checked={!!fval}
        onchange={(e) => ctx.commitField(n, fid, (e.currentTarget as HTMLInputElement).checked)}
      />
    {:else if fkind === 'number'}
      <input
        id={`pf-side-${fid}`}
        class="field-input"
        type="number"
        min={n.min}
        max={n.max}
        step={n.step ?? 'any'}
        readonly={fro}
        value={fval ?? ''}
        oninput={(e) => {
          const raw = (e.currentTarget as HTMLInputElement).value;
          if (raw === '') return;
          const num = Number(raw);
          if (!Number.isNaN(num)) ctx.commitField(n, fid, num);
        }}
      />
    {:else if fkind === 'range'}
      {@const rmin  = typeof n.min  === 'number' ? n.min  : 0}
      {@const rmax  = typeof n.max  === 'number' ? n.max  : 100}
      {@const rstep = typeof n.step === 'number' ? n.step : 1}
      <input
        id={`pf-side-${fid}`}
        class="field-range"
        type="range"
        min={rmin}
        max={rmax}
        step={rstep}
        disabled={fro}
        value={Number(fval ?? rmin)}
        oninput={(e) => {
          const raw = (e.currentTarget as HTMLInputElement).value;
          const num = Number(raw);
          if (!Number.isNaN(num)) ctx.commitField(n, fid, num);
        }}
      />
      {#if n.value_label != null}
        <span class="field-range-value">{n.value_label}</span>
      {/if}
    {:else if fkind === 'select'}
      {@const opts = Array.isArray(n.options) ? n.options : []}
      <select
        id={`pf-side-${fid}`}
        class="field-input field-select"
        disabled={fro}
        value={fval ?? ''}
        onchange={(e) => ctx.commitField(n, fid, (e.currentTarget as HTMLSelectElement).value)}
      >
        {#each opts as opt, oi (oi)}
          {@const ov = typeof opt === 'string' ? opt : (opt.value ?? '')}
          {@const ol = typeof opt === 'string' ? opt : (opt.label ?? opt.value ?? '')}
          <option value={ov}>{ol}</option>
        {/each}
      </select>
    {:else if fkind === 'readonly'}
      <span class="field-readonly-value">{String(fval ?? '')}</span>
    {:else}
      <input
        id={`pf-side-${fid}`}
        class="field-input"
        type="text"
        placeholder={n.placeholder ?? ''}
        readonly={fro}
        value={fval ?? ''}
        oninput={(e) => ctx.commitField(n, fid, (e.currentTarget as HTMLInputElement).value)}
      />
    {/if}
    {#if n.suffix}
      <span class="field-suffix">{n.suffix}</span>
    {/if}
    {#if n.pin_action}
      <button
        type="button"
        class="field-pin"
        class:pinned={!!n.pinned}
        use:tooltip={n.pinned ? 'Unpin live chart' : 'Pin live chart'}
        onclick={() => ctx.fireAction(n.pin_action, n.pin_payload ?? n.payload ?? {})}
      >
        {#if n.pinned}<PinOff size={11} />{:else}<Pin size={11} />{/if}
      </button>
    {/if}
  </div>

{:else if n.type === 'color_field'}
  {@const cfid       = ctx.fieldKey(n, i)}
  {@const cval       = (n.value ?? {}) as Record<string, number>}
  {@const cchannels  = (n.channels ?? { r: '.red', g: '.green', b: '.blue', a: '.alpha' }) as Record<string, string>}
  {@const cnameR     = cchannels.r != null ? cchannels.r.replace(/^\./, '') : 'red'}
  {@const cnameG     = cchannels.g != null ? cchannels.g.replace(/^\./, '') : 'green'}
  {@const cnameB     = cchannels.b != null ? cchannels.b.replace(/^\./, '') : 'blue'}
  {@const cnameA     = cchannels.a != null ? cchannels.a.replace(/^\./, '') : 'alpha'}
  {@const cR         = (ctx.fieldValue(cfid + '::r', cval[cnameR] ?? 0)) as number}
  {@const cG         = (ctx.fieldValue(cfid + '::g', cval[cnameG] ?? 0)) as number}
  {@const cB         = (ctx.fieldValue(cfid + '::b', cval[cnameB] ?? 0)) as number}
  {@const cA         = (ctx.fieldValue(cfid + '::a', cval[cnameA] ?? 1)) as number}
  {@const cHex       = packHexColor(cR, cG, cB)}
  {@const cro        = !!n.readonly}
  {@const setChannel = (ch: string, key: string, v: number) => {
    ctx.setFieldDraft(cfid + '::' + key, v);
    ctx.commitColorChannel(n, ch, v);
  }}
  <div class="node-color-field" class:field-readonly={cro} class:field-highlight={!!n.highlight}>
    {#if n.label}
      <label class="field-label" for={`pf-side-${cfid}`}>{n.label}</label>
    {/if}
    <input
      id={`pf-side-${cfid}`}
      class="field-color-swatch"
      type="color"
      disabled={cro}
      value={cHex}
      oninput={(e) => {
        const next = parseHexColor((e.currentTarget as HTMLInputElement).value);
        if (!next) return;
        setChannel('r', 'r', next.r);
        setChannel('g', 'g', next.g);
        setChannel('b', 'b', next.b);
      }}
    />
    <input
      class="field-input field-color-hex"
      type="text"
      spellcheck="false"
      readonly={cro}
      value={cHex}
      oninput={(e) => {
        const next = parseHexColor((e.currentTarget as HTMLInputElement).value);
        if (!next) return;
        setChannel('r', 'r', next.r);
        setChannel('g', 'g', next.g);
        setChannel('b', 'b', next.b);
      }}
    />
    {#if n.alpha !== false}
      <span class="field-color-alpha-label">A</span>
      <input
        class="field-input field-color-alpha"
        type="number"
        min="0" max="1" step="0.01"
        readonly={cro}
        value={cA}
        oninput={(e) => {
          const raw = (e.currentTarget as HTMLInputElement).value;
          if (raw === '') return;
          const v = Number(raw);
          if (Number.isFinite(v)) setChannel('a', 'a', v);
        }}
      />
    {/if}
  </div>

{:else if n.type === 'vec_field'}
  {@const vfid    = ctx.fieldKey(n, i)}
  {@const vaxes   = (Array.isArray(n.axes) && n.axes.length > 0 ? n.axes : ['x', 'y', 'z']) as string[]}
  {@const vro     = !!n.readonly}
  <div class="node-vec-field" class:field-readonly={vro} class:field-highlight={!!n.highlight}>
    {#if n.label}
      <span class="field-label">{n.label}</span>
    {/if}
    <div class="vec-axes">
      {#each vaxes as axis (axis)}
        {@const av = ctx.vecAxisValue(n, axis)}
        <div class="vec-axis" class:vec-axis-x={axis === 'x'}
                              class:vec-axis-y={axis === 'y'}
                              class:vec-axis-z={axis === 'z'}
                              class:vec-axis-w={axis === 'w'}>
          <button
            type="button"
            class="vec-axis-label"
            disabled={vro}
            title={`Drag to edit · double-click to reset · Shift = fine, Ctrl = coarse`}
            onmousedown={(e) => !vro && ctx.startVecDrag(n, axis, av, e)}
            ondblclick={() => !vro && ctx.resetVecAxis(n, axis)}
          >{axis.toUpperCase()}</button>
          <input
            type="number"
            class="field-input vec-axis-input"
            step="any"
            readonly={vro}
            value={av}
            oninput={(e) => {
              const raw = (e.currentTarget as HTMLInputElement).value;
              if (raw === '') return;
              const num = Number(raw);
              if (!Number.isNaN(num)) {
                ctx.setFieldDraft(vfid + '::' + axis, num);
                ctx.commitVecAxis(n, axis, num);
              }
            }}
          />
        </div>
      {/each}
    </div>
  </div>

{:else if n.type === 'entity_ref'}
  {@const eid    = (n.value ?? n.entity) as number}
  {@const ename  = (n.name_hint ?? '') as string}
  {@const eact   = (n.action ?? 'bevy-brp:goto_entity') as string}
  <div class="node-entity-ref">
    {#if n.label}
      <span class="field-label">{n.label}</span>
    {/if}
    <button
      type="button"
      class="entity-ref-pill"
      title={`Entity ${eid}${ename ? ' · ' + ename : ''} — click to focus`}
      onclick={() => ctx.fireAction(eact, { entity: eid })}
    >
      <span class="entity-ref-id">#{eid}</span>
      {#if ename}
        <span class="entity-ref-sep">·</span>
        <span class="entity-ref-name">{ename}</span>
      {/if}
    </button>
  </div>
{/if}
