<script lang="ts">
  /**
   * What is in scope at the selected frame — the second column of the console while the
   * program is stopped: the variables, and under them the watches.
   *
   * One column rather than two because they answer the same question from opposite ends. The
   * variables are *everything* that happens to be here; a watch is the two or three things you
   * keep re-checking, and it is worth its own strip precisely because it stays put while the
   * variables change under it.
   */
  import { Plus, X, PanelLeftClose, Braces } from 'lucide-svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import BennuVarTree from './BennuVarTree.svelte';
  import BennuDebugValueModal from './BennuDebugValueModal.svelte';
  import { bennuDebugStore } from '$lib/stores/bennu/debug.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuDebugLayout } from './debug-layout.svelte';

  let watchDraft = $state('');

  /**
   * Which debugger is underneath. A JVM watch and a native one are both paths, but not the same
   * path: Rust adds a leading `*`, and behind a `/nat` or `/py` prefix the adapter's own evaluators
   * are reachable. Explaining the wrong one is worse than explaining nothing, so the prose follows
   * the session rather than being written once for Java.
   */
  const native = $derived(bennuDebugStore.engine === 'native');

  function addWatch() {
    const root = projectStore.project?.root;
    const text = watchDraft.trim();
    if (!root || !text) return;
    watchDraft = '';
    void bennuDebugStore.addWatch(root, text);
  }

  function removeWatch(expression: string) {
    const root = projectStore.project?.root;
    if (root) bennuDebugStore.removeWatch(root, expression);
  }
</script>

<div class="dv">
  <div class="dv-title">
    Variables
    {#if bennuDebugStore.variablesLoading}<Spinner size={11} />{/if}
    <button
      class="dv-btn dv-collapse"
      type="button"
      use:tooltip={'Collapse this column'}
      aria-label="Collapse the variables column"
      onclick={() => bennuDebugLayout.toggleValues()}
    >
      <PanelLeftClose size={12} />
    </button>
  </div>
  <div class="dv-scroll">
    <!-- The one thing worth saying before anything else: an adapter with no Rust formatters shows a
         `Vec` as a pointer and a length, and from inside the tree that is indistinguishable from a
         broken debugger. The sentence says what to install. -->
    {#if bennuDebugStore.note}
      <div class="dv-caveat">
        <Alert variant="warning" compact text={bennuDebugStore.note} />
      </div>
    {/if}
    {#if bennuDebugStore.variables.length}
      <BennuVarTree nodes={bennuDebugStore.variables} />
    {:else if !bennuDebugStore.variablesLoading}
      <!-- Not "this method has no variables": in either language an empty tree usually means the
           build threw the names away, and the fix is a build flag rather than anything here. -->
      <div class="dv-note">
        {#if native}
          Nothing in scope here — or this binary was built without debug info
          (<code>[profile.dev] debug = true</code>, or the <code>dev</code> profile).
        {:else}
          Nothing in scope here — or this class was compiled without variable names
          (<code>-g:vars</code>).
        {/if}
      </div>
    {/if}
  </div>

  <div class="dv-title dv-watch-title">
    Watches
    <span class="dv-add">
      <Input
        bind:value={watchDraft}
        placeholder={native ? 'self.items[0].name' : 'order.customer.name'}
        size="sm"
        onkeydown={(e: KeyboardEvent) => { if (e.key === 'Enter') addWatch(); }}
      />
      <button
        class="dv-btn"
        type="button"
        use:tooltip={'Add a watch'}
        aria-label="Add a watch"
        disabled={!watchDraft.trim()}
        onclick={addWatch}
      >
        <Plus size={12} />
      </button>
    </span>
  </div>
  <div class="dv-watches">
    {#each bennuDebugStore.watches as watch (watch.expression)}
      <div class="dv-watch">
        <span class="dv-expr">{watch.expression}</span>
        {#if watch.error}
          <span class="dv-error" title={watch.error}>{watch.error}</span>
        {:else if watch.value}
          <span class="dv-value" title={watch.value.value}>{watch.value.value}</span>
          {#if watch.value.type_name}<span class="dv-type">{watch.value.type_name}</span>{/if}
          {#if watch.value.object}
            <button
              class="dv-open"
              type="button"
              use:tooltip={'Read the whole value'}
              aria-label="Read the whole value of {watch.expression}"
              onclick={() => void bennuDebugStore.inspectValue(watch.value!)}
            >
              <Braces size={11} />
            </button>
          {/if}
        {/if}
        <button
          class="dv-x"
          type="button"
          aria-label="Remove this watch"
          onclick={() => removeWatch(watch.expression)}
        >
          <X size={11} />
        </button>
      </div>
    {:else}
      <!-- Said plainly, because the shape of a watch is the one thing that surprises people: it is
           a path, not the language. On a native session the escape hatch is worth naming too —
           anything that is not a path goes to the adapter's evaluator, and `v.len()` never works
           there for a reason the panel would rather state than let you rediscover. -->
      <div class="dv-note">
        {#if native}
          A path — <code>v</code>, <code>self.order.total</code>, <code>items[2]</code>,
          <code>*head</code>. Anything else goes to the debugger's own evaluator; method calls
          (<code>v.len()</code>) cannot be evaluated in Rust at all.
        {:else}
          A path — <code>order</code>, <code>order.customer.name</code>, <code>items[2]</code>.
        {/if}
      </div>
    {/each}
  </div>
</div>

{#if bennuDebugStore.inspect}
  <BennuDebugValueModal inspect={bennuDebugStore.inspect} />
{/if}

<style>
  /* The width belongs to the enclosing ResizablePanel; this only has to fill it. */
  .dv {
    display: flex; flex-direction: column; height: 100%; min-height: 0; min-width: 0;
  }
  .dv-title {
    display: flex; align-items: center; gap: 6px;
    padding: 3px 10px; flex-shrink: 0;
    font-size: 10px; text-transform: uppercase; letter-spacing: 0.06em;
    color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle);
  }
  .dv-watch-title { border-top: 1px solid var(--border-subtle); }
  .dv-collapse { margin-left: auto; }
  .dv-add { display: flex; align-items: center; gap: 3px; margin-left: auto; min-width: 0; }
  .dv-add :global(.input-wrap) { width: 130px; }
  .dv-btn {
    display: inline-flex; align-items: center; justify-content: center;
    width: 18px; height: 18px; padding: 0;
    border: 0; border-radius: var(--radius-sm); background: none;
    color: var(--text-muted); cursor: pointer;
  }
  .dv-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .dv-btn:disabled { color: var(--text-disabled); cursor: default; }

  .dv-scroll { flex: 1; overflow: auto; min-height: 0; padding: 2px 0; }
  .dv-caveat { padding: 4px 8px 6px; }
  /* Capped: the watches are a short list you keep an eye on, and letting them take half the
     column would push the variables — which are the thing that changes — off screen. */
  .dv-watches { flex: 0 1 auto; max-height: 40%; overflow: auto; padding: 2px 0; }

  .dv-watch {
    display: flex; align-items: center; gap: 6px;
    padding: 1px 10px;
    font-family: var(--font-code); font-size: 11.5px; line-height: 1.6;
    white-space: nowrap;
  }
  .dv-watch:hover { background: var(--bg-hover); }
  .dv-expr { color: var(--syntax-field, #9876aa); }
  .dv-value { color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .dv-type { color: var(--text-muted); font-size: 10.5px; }
  .dv-error {
    color: var(--text-muted); font-style: italic; font-size: 10.5px;
    overflow: hidden; text-overflow: ellipsis; min-width: 0;
  }
  .dv-x {
    margin-left: auto; flex: 0 0 auto;
    display: inline-flex; align-items: center;
    padding: 0; border: 0; background: none; cursor: pointer;
    color: var(--text-muted); opacity: 0;
    transition: opacity var(--transition-fast), color var(--transition-fast);
  }
  .dv-watch:hover .dv-x { opacity: 1; }
  .dv-x:hover { color: var(--error); }

  /* The same affordance the tree rows carry, on a watch — a watch of a struct is exactly the case
     where reading it whole beats expanding it. */
  .dv-open {
    flex: 0 0 auto;
    display: inline-flex; align-items: center; justify-content: center;
    width: 16px; height: 16px; padding: 0;
    border: 0; border-radius: var(--radius-sm); background: none;
    color: var(--text-muted); cursor: pointer; opacity: 0;
    transition: opacity var(--transition-fast), background var(--transition-fast),
      color var(--transition-fast);
  }
  .dv-watch:hover .dv-open, .dv-open:focus-visible { opacity: 1; }
  .dv-open:hover { background: var(--bg-hover); color: var(--text-primary); }

  .dv-note { padding: 6px 10px; font-size: 11px; color: var(--text-muted); line-height: 1.6; }
  .dv-note code {
    font-family: var(--font-code); font-size: 10.5px;
    padding: 0 3px; border-radius: var(--radius-sm); background: var(--bg-elevated);
  }
</style>
