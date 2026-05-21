<!--
  SidebarNodeConsole — `console_input` (text + history + autocomplete)
  and `code` (read-only preformatted text with copy button).

  All draft + history + suggestion state lives in the dispatcher; this
  component only renders + dispatches through `ctx`.
-->
<script lang="ts">
  import { Send, Copy, Check } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { SidebarNodeCtx } from './ctx';

  interface Props {
    node:  any;
    index: number;
    ctx:   SidebarNodeCtx;
  }
  let { node: n, index: i, ctx }: Props = $props();
</script>

{#if n.type === 'console_input'}
  {@const cid     = ctx.fieldKey(n, i)}
  {@const cval    = ctx.consoleValue(cid)}
  {@const cmatches = ctx.consoleMatches(n, cval)}
  {@const cshow   = (ctx.suggestVisible.get(cid) ?? false) && cmatches.length > 0}
  {@const cactive = Math.max(0, Math.min(ctx.suggestActive.get(cid) ?? 0, cmatches.length - 1))}
  <div class="node-console-input">
    {#if n.label}
      <label class="field-label" for={`pf-console-${cid}`}>{n.label}</label>
    {/if}
    <div class="console-row">
      <input
        id={`pf-console-${cid}`}
        class="console-input"
        type="text"
        spellcheck="false"
        autocomplete="off"
        placeholder={n.placeholder ?? 'Type a command…'}
        value={cval}
        oninput={(e) => {
          ctx.setConsoleValue(cid, (e.currentTarget as HTMLInputElement).value);
          ctx.setSuggestVisible(cid, true);
          ctx.setSuggestActive(cid, 0);
        }}
        onfocus={() => ctx.setSuggestVisible(cid, true)}
        onblur={() => { setTimeout(() => ctx.setSuggestVisible(cid, false), 120); }}
        onkeydown={(e) => ctx.onConsoleKey(n, cid, e)}
      />
      <button
        type="button"
        class="console-send"
        disabled={!cval.trim()}
        use:tooltip={'Send (Enter)'}
        onclick={() => ctx.submitConsole(n)}
      >
        <Send size={12} />
        {#if n.submit_label}<span>{n.submit_label}</span>{/if}
      </button>
    </div>
    {#if cshow}
      <ul class="console-suggest" role="listbox">
        {#each cmatches as m, si (m + ':' + si)}
          <li
            role="option"
            aria-selected={si === cactive}
            class:active={si === cactive}
            onmousedown={(e) => { e.preventDefault(); ctx.acceptSuggestion(n, cid, m); }}
          >
            <span class="sug-prefix">{m.slice(0, cval.length)}</span><span class="sug-rest">{m.slice(cval.length)}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

{:else if n.type === 'code'}
  {@const codeKey = (n.id ?? '') + ':' + ctx.nodeKey(n, i)}
  {@const codeText = String(n.text ?? '')}
  {@const codeCopyable = n.copyable !== false}
  <div class="node-code">
    {#if codeCopyable}
      <button
        type="button"
        class="node-code-copy"
        use:tooltip={ctx.copiedKey === codeKey ? 'Copied' : 'Copy'}
        onclick={() => ctx.copyCode(codeKey, codeText)}
      >
        {#if ctx.copiedKey === codeKey}
          <Check size={12} />
        {:else}
          <Copy size={12} />
        {/if}
      </button>
    {/if}
    <pre class="node-code-pre"><code>{codeText}</code></pre>
  </div>
{/if}
