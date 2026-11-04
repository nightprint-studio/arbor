<script lang="ts">
  /**
   * SuggestInput — a text field that offers completions while you type, built from `Input` and
   * `SuggestionList`. The owner decides *what* to offer (`suggest`, a pure function of the text and the
   * caret); this draws the list under the field, keeps the focus in the field, and does the replacing.
   *
   * Keys: ↓ ↑ move, Enter or Tab pick, Esc closes the list (and only the list). The list is `position: fixed`,
   * so a field inside a scrolling table is not clipped by it.
   */
  import Input from './Input.svelte';
  import SuggestionList, { type Suggestion } from './SuggestionList.svelte';

  /** What to offer: the range of the text the pick replaces, and the rows (`id` is the text inserted). */
  export interface SuggestResult {
    from: number;
    to: number;
    items: Suggestion[];
  }

  interface Props {
    value: string;
    suggest: (text: string, caret: number) => SuggestResult | null;
    oninput: (value: string) => void;
    placeholder?: string;
    size?: 'sm' | 'md';
    quiet?: boolean;
    ariaLabel?: string;
  }

  let { value, suggest, oninput, placeholder, size = 'sm', quiet = false, ariaLabel }: Props = $props();

  const uid = `sg-${Math.random().toString(36).slice(2, 8)}`;
  let host = $state<HTMLElement | undefined>(undefined);
  let element = $state<HTMLInputElement | undefined>(undefined);
  let result = $state<SuggestResult | null>(null);
  let active = $state(0);
  let dismissed = $state(false);
  let box = $state({ left: 0, top: 0, width: 240 });

  function refresh() {
    const el = element;
    if (!el || document.activeElement !== el) { result = null; return; }
    const r = suggest(el.value, el.selectionStart ?? el.value.length);
    result = r && r.items.length ? r : null;
    active = 0;
    if (result && host) {
      const rect = host.getBoundingClientRect();
      box = { left: rect.left, top: rect.bottom + 2, width: Math.max(rect.width, 260) };
    }
  }

  function pick(item: Suggestion) {
    if (!result) return;
    const next = value.slice(0, result.from) + item.id + value.slice(result.to);
    const caret = result.from + item.id.length;
    result = null;
    oninput(next);
    queueMicrotask(() => element?.setSelectionRange(caret, caret));
  }

  function onkeydown(e: KeyboardEvent) {
    if (!result) return;
    const n = result.items.length;
    if (e.key === 'ArrowDown') { active = (active + 1) % n; e.preventDefault(); }
    else if (e.key === 'ArrowUp') { active = (active - 1 + n) % n; e.preventDefault(); }
    else if (e.key === 'Enter' || e.key === 'Tab') { pick(result.items[active]); e.preventDefault(); }
    else if (e.key === 'Escape') { result = null; dismissed = true; e.preventDefault(); e.stopPropagation(); }
  }
</script>

<div class="si" bind:this={host}>
  <Input
    {value}
    bind:element
    {placeholder}
    {size}
    {quiet}
    {ariaLabel}
    oninput={(v) => { dismissed = false; oninput(v); queueMicrotask(refresh); }}
    onkeydown={onkeydown}
    onfocus={() => { dismissed = false; refresh(); }}
    onblur={() => { result = null; }}
  />
  {#if result && !dismissed}
    <div class="si-pop" style:left="{box.left}px" style:top="{box.top}px" style:width="{box.width}px">
      <SuggestionList items={result.items} {active} id={uid} ariaLabel="Suggestions" onpick={pick} />
    </div>
  {/if}
</div>

<style>
  .si { min-width: 0; }
  .si-pop { position: fixed; z-index: var(--z-menu, 1000); }
</style>
