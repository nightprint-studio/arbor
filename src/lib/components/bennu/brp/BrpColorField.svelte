<script lang="ts">
  /**
   * A colour: a swatch that opens the picker, a hex field, and the alpha as a number. Understands the two
   * shapes a game sends — a `{red, green, blue, alpha}` struct and a `[r, g, b(, a)]` list — and writes the
   * same shape back. `linear` colours are shown through the sRGB curve so the swatch matches the game.
   */
  import ColorSwatch from '$lib/components/shared/ui/ColorSwatch.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import ScrubInput from '$lib/components/shared/ui/ScrubInput.svelte';
  import { hexToRgb, linearToSrgb, rgbaToHex, round, srgbToLinear, type Rgba } from './brp-math';
  import { isColorStruct } from './brp-widgets';

  let {
    value,
    path,
    linear = false,
    readonly = false,
    onCommit,
  }: {
    value: unknown;
    path: string;
    /** The channels are linear light (`LinearRgba`) rather than sRGB. */
    linear?: boolean;
    readonly?: boolean;
    onCommit: (path: string, value: unknown) => void;
  } = $props();

  const asStruct = $derived(isColorStruct(value));
  /** r, g, b, a in the colour's own space. */
  const channels = $derived.by<Rgba>(() => {
    if (isColorStruct(value)) return [value.red, value.green, value.blue, value.alpha ?? 1];
    const a = (value as number[]) ?? [];
    return [a[0] ?? 0, a[1] ?? 0, a[2] ?? 0, a[3] ?? 1];
  });
  const hasAlpha = $derived(asStruct || (value as number[]).length === 4);
  const display = $derived<Rgba>(linear ? [linearToSrgb(channels[0]), linearToSrgb(channels[1]), linearToSrgb(channels[2]), channels[3]] : channels);
  const hex = $derived(rgbaToHex(display));
  let draft = $state<string | null>(null);

  function write(rgba: Rgba) {
    const [r, g, b, a] = rgba.map((c) => round(c, 4));
    if (asStruct) onCommit(path, { red: r, green: g, blue: b, alpha: a });
    else onCommit(path, hasAlpha ? [r, g, b, a] : [r, g, b]);
  }

  function pick(next: string) {
    const rgb = hexToRgb(next);
    if (!rgb) return;
    const own = linear ? rgb.map(srgbToLinear) : rgb;
    write([own[0], own[1], own[2], channels[3]]);
  }
</script>

<span class="cf">
  <ColorSwatch color={hex} chipSize={18} onchange={readonly ? undefined : pick} tooltip="Pick a colour" />
  <span class="cf-hex">
    <Input
      value={draft ?? hex}
      size="sm"
      quiet
      {readonly}
      ariaLabel="Hex colour"
      oninput={(v) => (draft = v)}
      onchange={(v) => { draft = null; pick(v); }}
      onblur={() => (draft = null)}
    />
  </span>
  {#if hasAlpha}
    <span class="cf-alpha">
      <ScrubInput value={channels[3]} label="α" step={0.01} min={0} max={1} digits={2} reset={1} {readonly} onchange={(v) => write([channels[0], channels[1], channels[2], v])} />
    </span>
  {/if}
</span>

<style>
  .cf { display: inline-flex; align-items: center; gap: 6px; }
  .cf-hex { display: inline-flex; width: 84px; }
  .cf-alpha { display: inline-flex; }
</style>
