<script lang="ts">
  /**
   * TestsEditor — a request's tests: one declarative assertion per line, and below a `--- lua` line a
   * sandboxed Lua script. The results of the last run are listed under the editor.
   */
  import { Check, X } from 'lucide-svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import { languageForPath } from '$lib/components/bennu/languages';
  import { hirundoStore, type RequestTab } from '$lib/stores/hirundo/workspace.svelte';

  let { tab }: { tab: RequestTab } = $props();

  const language = languageForPath('tests.txt');
  let help = $state(false);
</script>

<div class="te">
  <div class="te-code">
    <CodeEditor
      value={tab.request.tests}
      {language}
      lineNumbers={false}
      placeholder={'status == 200\ntime < 500ms\nbody.content[*].stato == "APERTO"'}
      oninput={(text) => { tab.request.tests = text; hirundoStore.touch(tab); }}
    />
  </div>

  <div class="te-help">
    <Collapsible bind:open={help} chevron>
      {#snippet header()}<span class="te-h">Syntax</span>{/snippet}
      <p>One assertion per line: <code>subject operator value</code>. Subjects: <code>status</code>, <code>time</code>, <code>size</code>, <code>header.name</code>, <code>body</code>, <code>body.path[*].field</code>, <code>len(…)</code>. Operators: <code>== != &lt; &lt;= &gt; &gt;= contains startswith endswith exists empty</code>. A <code>[*]</code> path must hold for every element.</p>
      <p>For the rest, a line <code>--- lua</code> starts a sandboxed script: <code>hirundo.response.json()</code>, <code>hirundo.expect(cond, "message")</code>, <code>hirundo.env.set("name", value)</code> — a value set there is a local variable for the next request.</p>
    </Collapsible>
  </div>

  {#if tab.report}
    <ul class="te-results">
      {#each tab.report.checks as c, i (i)}
        <li class:fail={!c.passed}>
          {#if c.passed}<Check size={13} />{:else}<X size={13} />{/if}
          <code>{c.line}</code>
          {#if c.detail}<span class="te-detail">{c.detail}</span>{/if}
        </li>
      {/each}
      {#each tab.report.logs as line, i (i)}<li class="log"><code>{line}</code></li>{/each}
    </ul>
  {/if}
</div>

<style>
  .te { display: flex; flex-direction: column; min-height: 0; flex: 1; }
  .te-code { flex: 1; min-height: 140px; display: flex; }
  .te-code > :global(*) { flex: 1; min-width: 0; min-height: 0; }
  .te-help { padding: 6px 12px; border-top: 1px solid var(--border-subtle); font-size: var(--font-size-xs); color: var(--text-secondary); }
  .te-help code, .te-results code { font-family: var(--font-mono); }
  .te-h { font-size: var(--font-size-xs); color: var(--text-secondary); }
  .te-results { list-style: none; margin: 0; padding: 6px 12px 10px; border-top: 1px solid var(--border-subtle); display: flex; flex-direction: column; gap: 3px; font-size: var(--font-size-xs); }
  .te-results li { display: flex; align-items: center; gap: 8px; color: var(--success); }
  .te-results li.fail { color: var(--error); }
  .te-results li.log { color: var(--text-muted); }
  .te-detail { color: var(--text-muted); }
</style>
