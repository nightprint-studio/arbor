<script lang="ts">
  /**
   * Settings › Java › Debugger — the packages a step walks straight through.
   *
   * The list on screen is the backend's *effective* one, never assembled here: a copy of the
   * defaults on this side would be a second answer that drifts from the one doing the stepping.
   */
  import { Bug, Plus, Trash2 } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import { bennuConfigStore } from '$lib/stores/bennu/config.svelte';
  import { getStepExcludes } from '$lib/ipc/bennu/debug';

  let stepExcludes = $state<string[]>([]);
  let draft = $state('');

  /** Whether the list on screen is the backend's default rather than one of your own — what makes
   *  "Reset" meaningful and what the hint says. */
  const usingDefaults = $derived((bennuConfigStore.cfg?.step_excludes ?? []).length === 0);

  async function load() {
    stepExcludes = await getStepExcludes().catch(() => []);
  }
  $effect(() => { void load(); });

  /** Whether the VM will accept this pattern: a `*` at one end, or none. One bad entry makes the VM
   *  refuse the whole step request, which shows up as stepping having quietly stopped working — so
   *  it is refused here, where there is somewhere to say so. */
  function validPattern(p: string): boolean {
    const inner = p.trim().replace(/^\*/, '').replace(/\*$/, '');
    return inner.length > 0 && !inner.includes('*');
  }
  const draftValid = $derived(!draft.trim() || validPattern(draft));

  /** Persist a new list and re-read what the backend made of it. Writing the *effective* list is
   *  what turns the defaults into a list of your own — adding one pattern to an empty config would
   *  otherwise mean stepping into the JDK from then on. */
  async function commit(next: string[]) {
    await bennuConfigStore.patch({ step_excludes: next });
    await load();
  }
  async function add() {
    const p = draft.trim();
    if (!p || !validPattern(p) || stepExcludes.includes(p)) return;
    draft = '';
    await commit([...stepExcludes, p]);
  }
</script>

<div class="section-header">
  <h2>Debugger</h2>
  <p>What a step walks through, and what it walks past.</p>
</div>
<div class="card">
  <div class="card-section-title"><Bug size={12} /> Step-through packages</div>
  <p class="set-hint">
    Classes a <strong>step into</strong> passes straight through instead of stopping in. Without
    them, stepping into <code>service.place(order)</code> walks the proxy, then
    <code>ReflectiveMethodInvocation.proceed</code>, then every interceptor in the chain — a dozen
    stops in code you have no source for. Remove <code>org.springframework.*</code> to be able to
    step into Spring itself. A <code>*</code> is allowed at <strong>one end only</strong>.
  </p>
  {#if usingDefaults}
    <p class="set-empty">These are the defaults. Adding or removing one makes the list yours.</p>
  {/if}
  <div class="set-list">
    {#each stepExcludes as p (p)}
      <div class="set-list-row">
        <span class="set-list-text set-mono" use:tooltip={p}>{p}</span>
        <button class="set-list-del" type="button" onclick={() => void commit(stepExcludes.filter((x) => x !== p))} aria-label="Remove pattern">
          <Trash2 size={13} />
        </button>
      </div>
    {/each}
  </div>
  <div class="set-list-add">
    <Input
      bind:value={draft}
      placeholder="com.acme.generated.*"
      size="sm"
      error={draftValid ? null : 'A * is allowed at one end only'}
      onkeydown={(e: KeyboardEvent) => { if (e.key === 'Enter') void add(); }}
    />
    <Button variant="ghost" size="sm" disabled={!draft.trim() || !draftValid} onclick={() => void add()}>
      {#snippet iconStart()}<Plus size={13} />{/snippet}
      Add
    </Button>
    <Button variant="ghost" size="sm" disabled={usingDefaults} onclick={() => void commit([])}>Reset</Button>
  </div>
  {#if !draftValid}
    <p class="set-invalid">A pattern may carry a <code>*</code> at one end only — the VM refuses anything else, and one bad entry stops stepping working at all.</p>
  {/if}
</div>
