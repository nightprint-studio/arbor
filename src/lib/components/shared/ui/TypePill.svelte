<!--
  TypePill — small uppercase pill that surfaces the kind of a value next to
  a field (e.g. `Vec3`, `Quat`, `u32`, `enum`, `Handle`). Two modes:

    · `kind`  → resolves to a curated palette (vec/quat/numeric/bool/enum/
                handle/entity/option/string/array/struct/unknown). Keeps the
                visual language consistent across all plugins that want to
                show typed values.
    · `tone`  → explicit semantic tone (`accent`, `info`, `success`,
                `warning`, `error`, `muted`). Fallback for custom labels
                outside the curated palette.

  Use this in compact field rows, IntelliJ-style component cards, or
  anywhere you want a one-word type hint without taking real estate.
-->
<script lang="ts">
  type Kind =
    | 'vec' | 'vec2' | 'vec3' | 'vec4' | 'quat' | 'mat'
    | 'i8'  | 'i16'  | 'i32'  | 'i64'  | 'isize'
    | 'u8'  | 'u16'  | 'u32'  | 'u64'  | 'usize'
    | 'f32' | 'f64'
    | 'bool' | 'enum' | 'flags' | 'option' | 'array' | 'map'
    | 'string' | 'char'
    | 'entity' | 'handle' | 'asset' | 'color'
    | 'struct' | 'tuple' | 'unit'
    | 'unknown';

  type Tone = 'accent' | 'info' | 'success' | 'warning' | 'error' | 'muted';

  interface Props {
    /** Visible text. When omitted the resolved kind is shown as-is. */
    label?: string;
    /** Curated kind — picks a palette. Case-insensitive. */
    kind?:  Kind | string;
    /** Explicit tone override. Wins over `kind`. */
    tone?:  Tone;
    /** Tooltip on hover. */
    tooltip?: string;
  }

  let { label, kind, tone, tooltip }: Props = $props();

  // Resolve a free-form kind string to one of the buckets. Numeric integer
  // widths all map to `int`; floats to `float`. Anything we don't recognise
  // falls through to `unknown` which renders dim/neutral.
  function resolveBucket(k?: string): string {
    if (!k) return 'unknown';
    const s = k.toLowerCase();
    if (/^vec[234]?$/.test(s) || s === 'vector') return 'vec';
    if (s === 'quat' || s === 'quaternion')     return 'quat';
    if (/^mat[234]?$/.test(s) || s.startsWith('mat')) return 'mat';
    if (/^[iu](size|8|16|32|64|128)$/.test(s))  return 'int';
    if (/^f(32|64)$/.test(s) || s === 'float')  return 'float';
    if (s === 'bool' || s === 'boolean')        return 'bool';
    if (s === 'enum' || s === 'variant')        return 'enum';
    if (s === 'flags' || s === 'bitflags')      return 'flags';
    if (s === 'option' || s === 'maybe')        return 'option';
    if (s === 'array' || s === 'list' || s === 'vec<>') return 'array';
    if (s === 'map' || s === 'hashmap' || s === 'btreemap') return 'map';
    if (s === 'string' || s === 'str' || s === 'cow<str>')   return 'string';
    if (s === 'char')                           return 'char';
    if (s === 'entity' || s === 'entity_ref')   return 'entity';
    if (s === 'handle' || s === 'asset_handle') return 'handle';
    if (s === 'asset')                          return 'asset';
    if (s === 'color' || s === 'srgba' || s === 'linearrgba' || s === 'hsla') return 'color';
    if (s === 'struct')                         return 'struct';
    if (s === 'tuple')                          return 'tuple';
    if (s === 'unit' || s === '()')             return 'unit';
    return 'unknown';
  }

  const bucket = $derived(tone ? null : resolveBucket(kind));
  const shown  = $derived(label ?? kind ?? '');
</script>

{#if shown}
  <span
    class="type-pill"
    class:tone-accent={tone === 'accent'}
    class:tone-info={tone === 'info'}
    class:tone-success={tone === 'success'}
    class:tone-warning={tone === 'warning'}
    class:tone-error={tone === 'error'}
    class:tone-muted={tone === 'muted'}
    class:b-vec={bucket === 'vec'}
    class:b-quat={bucket === 'quat'}
    class:b-mat={bucket === 'mat'}
    class:b-int={bucket === 'int'}
    class:b-float={bucket === 'float'}
    class:b-bool={bucket === 'bool'}
    class:b-enum={bucket === 'enum'}
    class:b-flags={bucket === 'flags'}
    class:b-option={bucket === 'option'}
    class:b-array={bucket === 'array'}
    class:b-map={bucket === 'map'}
    class:b-string={bucket === 'string'}
    class:b-char={bucket === 'char'}
    class:b-entity={bucket === 'entity'}
    class:b-handle={bucket === 'handle'}
    class:b-asset={bucket === 'asset'}
    class:b-color={bucket === 'color'}
    class:b-struct={bucket === 'struct'}
    class:b-tuple={bucket === 'tuple'}
    class:b-unit={bucket === 'unit'}
    class:b-unknown={bucket === 'unknown'}
    title={tooltip ?? undefined}
  >{shown}</span>
{/if}

<style>
  .type-pill {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    padding: 1px 6px;
    height: 15px;
    border-radius: 3px;
    border: 1px solid transparent;
    font-family: var(--font-ui-sans);
    white-space: nowrap;
    flex-shrink: 0;
  }

  /* Vector / math types — soft greens/teals */
  .b-vec   { color: #74c69d; background: color-mix(in srgb, #74c69d 12%, transparent); border-color: color-mix(in srgb, #74c69d 28%, transparent); }
  .b-quat  { color: #b288f0; background: color-mix(in srgb, #b288f0 12%, transparent); border-color: color-mix(in srgb, #b288f0 28%, transparent); }
  .b-mat   { color: #8ad0c9; background: color-mix(in srgb, #8ad0c9 12%, transparent); border-color: color-mix(in srgb, #8ad0c9 28%, transparent); }

  /* Numeric primitives — yellows / oranges */
  .b-int   { color: #e0b86e; background: color-mix(in srgb, #e0b86e 12%, transparent); border-color: color-mix(in srgb, #e0b86e 28%, transparent); }
  .b-float { color: #f0a55a; background: color-mix(in srgb, #f0a55a 12%, transparent); border-color: color-mix(in srgb, #f0a55a 28%, transparent); }

  /* Boolean — clear blue */
  .b-bool  { color: #62a0ea; background: color-mix(in srgb, #62a0ea 12%, transparent); border-color: color-mix(in srgb, #62a0ea 28%, transparent); }

  /* Enum / flags / option — pinks */
  .b-enum   { color: #e58fb1; background: color-mix(in srgb, #e58fb1 12%, transparent); border-color: color-mix(in srgb, #e58fb1 28%, transparent); }
  .b-flags  { color: #d883c4; background: color-mix(in srgb, #d883c4 12%, transparent); border-color: color-mix(in srgb, #d883c4 28%, transparent); }
  .b-option { color: #c9a0dc; background: color-mix(in srgb, #c9a0dc 12%, transparent); border-color: color-mix(in srgb, #c9a0dc 28%, transparent); }

  /* Containers — neutral cool greys */
  .b-array { color: #9bb0c4; background: color-mix(in srgb, #9bb0c4 12%, transparent); border-color: color-mix(in srgb, #9bb0c4 28%, transparent); }
  .b-map   { color: #88a5b8; background: color-mix(in srgb, #88a5b8 12%, transparent); border-color: color-mix(in srgb, #88a5b8 28%, transparent); }
  .b-tuple { color: #adc1d6; background: color-mix(in srgb, #adc1d6 12%, transparent); border-color: color-mix(in srgb, #adc1d6 28%, transparent); }

  /* Text */
  .b-string { color: #6dcdb8; background: color-mix(in srgb, #6dcdb8 12%, transparent); border-color: color-mix(in srgb, #6dcdb8 28%, transparent); }
  .b-char   { color: #9ad6b2; background: color-mix(in srgb, #9ad6b2 12%, transparent); border-color: color-mix(in srgb, #9ad6b2 28%, transparent); }

  /* Entity / asset / handle — bright accent */
  .b-entity { color: #f08c54; background: color-mix(in srgb, #f08c54 14%, transparent); border-color: color-mix(in srgb, #f08c54 32%, transparent); }
  .b-handle { color: #c98fe5; background: color-mix(in srgb, #c98fe5 14%, transparent); border-color: color-mix(in srgb, #c98fe5 32%, transparent); }
  .b-asset  { color: #e5b078; background: color-mix(in srgb, #e5b078 12%, transparent); border-color: color-mix(in srgb, #e5b078 28%, transparent); }
  .b-color  { color: #f278a0; background: color-mix(in srgb, #f278a0 12%, transparent); border-color: color-mix(in srgb, #f278a0 28%, transparent); }

  /* Composite */
  .b-struct  { color: #8ec4f0; background: color-mix(in srgb, #8ec4f0 12%, transparent); border-color: color-mix(in srgb, #8ec4f0 28%, transparent); }
  .b-unit    { color: var(--text-disabled); background: var(--bg-overlay); border-color: var(--border-subtle); }
  .b-unknown { color: var(--text-disabled); background: var(--bg-overlay); border-color: var(--border-subtle); }

  /* Semantic tone overrides (explicit `tone` prop wins over kind) */
  .tone-accent  { color: var(--accent);  background: color-mix(in srgb, var(--accent) 16%, transparent);  border-color: color-mix(in srgb, var(--accent)  32%, transparent); }
  .tone-info    { color: var(--info);    background: color-mix(in srgb, var(--info) 14%, transparent);    border-color: color-mix(in srgb, var(--info)    30%, transparent); }
  .tone-success { color: var(--success); background: color-mix(in srgb, var(--success) 14%, transparent); border-color: color-mix(in srgb, var(--success) 30%, transparent); }
  .tone-warning { color: var(--warning); background: color-mix(in srgb, var(--warning) 14%, transparent); border-color: color-mix(in srgb, var(--warning) 30%, transparent); }
  .tone-error   { color: var(--error);   background: color-mix(in srgb, var(--error) 14%, transparent);   border-color: color-mix(in srgb, var(--error)   30%, transparent); }
  .tone-muted   { color: var(--text-disabled); background: var(--bg-overlay); border-color: var(--border-subtle); }
</style>
