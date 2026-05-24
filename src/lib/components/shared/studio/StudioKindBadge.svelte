<script lang="ts" module>
  export type StudioKindTone =
    | 'type'        // object/array (blue)
    | 'string'      // string/char (green)
    | 'number'      // int/float/bool (purple)
    | 'keyword'     // option/enum-variant / TOML containers (orange)
    | 'muted'       // null/unit/none (text-muted)
    | 'accent';     // named struct/named tuple/unit variant (accent)
</script>

<script lang="ts">
  import { tooltip as tip } from '$lib/actions/tooltip';

  interface Props {
    label:    string;
    tone:     StudioKindTone;
    /** JSON-style tinted background. Default = false (text color only). */
    tinted?:  boolean;
    italic?:  boolean;
    tooltip?: string;
    /** Optional override class for one-off color tweaks (e.g. RON `none`). */
    extraClass?: string;
  }

  let { label, tone, tinted = false, italic = false, tooltip, extraClass }: Props = $props();
</script>

<span class="skb skb-{tone} {tinted ? 'skb-tinted' : ''} {italic ? 'skb-italic' : ''} {extraClass ?? ''}"
      use:tip={tooltip ?? ''}
>{label}</span>

<style>
  .skb {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 3px;
    background: var(--bg-overlay);
    color: var(--text-muted);
    font-family: var(--font-code);
    font-size: 10px;
    font-weight: 700;
    flex-shrink: 0;
  }

  .skb-type    { color: var(--syntax-type,    #4d78cc); }
  .skb-string  { color: var(--syntax-string,  #6a9956); }
  .skb-number  { color: var(--syntax-number,  #9876aa); }
  .skb-keyword { color: var(--syntax-keyword, #cc7832); }
  .skb-muted   { color: var(--text-muted); }
  .skb-accent {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
  }

  /* Tinted variant (JSON style) — adds a soft bg derived from the tone color. */
  .skb-tinted.skb-type    { background: color-mix(in srgb, var(--syntax-type,    #4d78cc) 18%, transparent); }
  .skb-tinted.skb-string  { background: color-mix(in srgb, var(--syntax-string,  #6a9956) 18%, transparent); }
  .skb-tinted.skb-number  { background: color-mix(in srgb, var(--syntax-number,  #9876aa) 18%, transparent); }
  .skb-tinted.skb-keyword { background: color-mix(in srgb, var(--syntax-keyword, #cc7832) 18%, transparent); }
  .skb-tinted.skb-muted   { background: var(--bg-overlay); }

  .skb-italic { font-style: italic; }
</style>
