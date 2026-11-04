<script lang="ts">
  /**
   * The console — call any method the connected game offers, with raw JSON params.
   *
   * Rendered twice by the panel (`part="list"` the history, `part="detail"` the composer and the
   * answer), like Resources. Keyboard: type a method (↑/↓ + Tab or Enter pick a suggestion), Tab to
   * the params, Ctrl+Enter runs; Enter on a history row loads it back into the fields.
   */
  import { History, Play, Trash2 } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import SuggestionList from '$lib/components/shared/ui/SuggestionList.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import { bennuBrpStore as brp } from '$lib/stores/bennu/brp.svelte';
  import { brpConsoleStore as con } from '$lib/stores/bennu/brp-console.svelte';
  import { suggestMethods } from './brp-console';

  let { part }: { part: 'list' | 'detail' } = $props();

  // The method and params fields live in the store: the history (one instance) loads them into the
  // composer (the other), and they survive a tab switch.
  const fields = con;

  let methodField = $state<HTMLInputElement | undefined>();
  let active = $state(0);
  let picking = $state(true);

  const suggestions = $derived(
    picking
      ? suggestMethods(con.methods, fields.method).map((m) => ({ id: m, label: m }))
      : [],
  );

  $effect(() => {
    void con.loadMethods(brp.status.generation);
  });
  $effect(() => {
    if (part === 'detail') methodField?.focus();
  });
  $effect(() => {
    // A new suggestion list starts at its top.
    void suggestions.length;
    active = 0;
  });

  function run() {
    picking = false;
    void con.run(fields.method, fields.params);
  }

  function pick(id: string) {
    fields.method = id;
    picking = false;
  }

  function onMethodKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      run();
    } else if (suggestions.length > 0 && e.key === 'ArrowDown') {
      e.preventDefault();
      active = (active + 1) % suggestions.length;
    } else if (suggestions.length > 0 && e.key === 'ArrowUp') {
      e.preventDefault();
      active = (active - 1 + suggestions.length) % suggestions.length;
    } else if (suggestions.length > 0 && (e.key === 'Tab' || e.key === 'Enter')) {
      e.preventDefault();
      pick(suggestions[active].id);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      run();
    } else if (e.key === 'Escape' && suggestions.length > 0) {
      e.stopPropagation();
      picking = false;
    }
  }

  const historyNodes = $derived(
    con.history.map((h, i) => ({ id: String(i), entry: h, children: [] as never[] })),
  );

  function load(i: number) {
    const h = con.history[i];
    if (!h) return;
    fields.method = h.method;
    fields.params = h.params;
    picking = false;
  }
</script>

{#if part === 'list'}
  <PaneHeader title="History">
    {#snippet end()}
      <span class="bc-count">{con.history.length}</span>
      <IconButton tooltip="Clear history" size={22} disabled={con.history.length === 0} onclick={() => con.clearHistory()}>
        <Trash2 size={13} />
      </IconButton>
    {/snippet}
  </PaneHeader>
  <div class="bc-hist">
    {#if historyNodes.length === 0}
      <EmptyState message="No calls yet" description="Calls you run appear here; pick one to load it again." />
    {:else}
      <Tree nodes={historyNodes} selectedId={null} onSelect={(n) => load(Number(n.id))} ariaLabel="Call history">
        {#snippet row({ node })}
          <History size={12} />
          <span class="bc-method">{node.entry.method}</span>
          {#if node.entry.params}<span class="bc-params">{node.entry.params}</span>{/if}
        {/snippet}
      </Tree>
    {/if}
  </div>
{:else}
  <PaneHeader title="Call a method">
    {#snippet end()}
      <Button variant="primary" size="sm" loading={con.running} disabled={!fields.method.trim()} onclick={run}>
        {#snippet iconStart()}<Play size={12} />{/snippet}
        Run
      </Button>
    {/snippet}
  </PaneHeader>
  <div class="bc-form">
    <div class="bc-method-field">
      <Input
        bind:value={fields.method}
        bind:element={methodField}
        size="sm"
        placeholder="world.query · rpc.discover · my_game.spawn_wave"
        ariaLabel="Method"
        attrs={{ role: 'combobox', 'aria-expanded': suggestions.length > 0, 'aria-controls': 'brp-method-suggestions' }}
        oninput={() => (picking = true)}
        onkeydown={onMethodKey}
      />
      {#if suggestions.length > 0}
        <div class="bc-suggest">
          <SuggestionList items={suggestions} {active} id="brp-method-suggestions" ariaLabel="Methods" onpick={(s) => pick(s.id)} />
        </div>
      {/if}
    </div>
    <Textarea
      bind:value={fields.params}
      framed
      minRows={4}
      maxRows={12}
      placeholder={'Params as JSON — e.g. {"data": {"components": []}}'}
      ariaLabel="Params"
      onsubmit={run}
    />
  </div>
  <div class="bc-out">
    {#if con.output}
      <div class="bc-out-head">
        <span class:bc-err={con.output.error}>{con.output.error ? 'Error' : 'Result'} · {con.output.method}</span>
        <CopyButton value={con.output.text} title="Copy the answer" />
      </div>
      <pre class="bc-pre" class:bc-err={con.output.error}>{con.output.text}</pre>
    {:else}
      <EmptyState message="Nothing run yet" description="Ctrl+Enter runs the call. The method list is the game's own rpc.discover." />
    {/if}
  </div>
{/if}

<style>
  .bc-count { font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .bc-hist { flex: 1; min-height: 0; overflow-y: auto; }
  .bc-method { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); white-space: nowrap; }
  .bc-params { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .bc-form { flex: none; display: flex; flex-direction: column; gap: 8px; padding: 8px 10px; }
  .bc-method-field { position: relative; }
  .bc-suggest { position: absolute; left: 0; right: 0; top: calc(100% + 2px); z-index: 20; }
  .bc-out { flex: 1; min-height: 0; display: flex; flex-direction: column; padding: 0 10px 10px; }
  .bc-out-head { display: flex; align-items: center; justify-content: space-between; font-size: var(--font-size-xs); color: var(--text-secondary); padding-bottom: 4px; }
  .bc-pre {
    flex: 1; min-height: 0; margin: 0; padding: 8px 10px; overflow: auto;
    background: var(--bg-overlay); border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary);
    white-space: pre-wrap; word-break: break-word;
  }
  .bc-err { color: var(--danger); }
</style>
