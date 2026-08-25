<script lang="ts">
  /**
   * One module, in words — what the picture cannot say in a 96-pixel box.
   *
   * Two halves, and the split is the point: the **numbers** answer "should I be careful with this
   * one", and the **two lists** answer "who, exactly". The lists are the reason this is not a
   * tooltip: every row is somewhere to go, either to another node of the graph or into the manifest.
   */
  import { ArrowRight, ArrowLeft, FileCode2, TriangleAlert } from 'lucide-svelte';
  import type { GraphNode } from '$lib/ipc/bennu/deps';
  import { moduleWord } from '$lib/ipc/bennu/deps';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    node,
    ecosystem,
    /** Direct dependencies and dependents, as `(index, node)` pairs the parent resolved. */
    dependencies,
    dependents,
    onPick,
    onOpenManifest,
  }: {
    node: GraphNode;
    ecosystem: string;
    dependencies: { index: number; node: GraphNode }[];
    dependents: { index: number; node: GraphNode }[];
    onPick: (index: number) => void;
    onOpenManifest: () => void;
  } = $props();

  const word = $derived(moduleWord(ecosystem, true));
</script>

<div class="gd">
  <div class="gd-head">
    <span class="gd-name" use:tooltip={node.id}>{node.name || node.id}</span>
    {#if node.kind}<span class="gd-kind">{node.kind}</span>{/if}
  </div>

  {#if node.in_cycle}
    <!-- The one state that is a defect rather than a measurement. -->
    <div class="gd-cycle">
      <TriangleAlert size={12} />
      <span>In a dependency cycle — see the ring in red.</span>
    </div>
  {/if}

  <dl class="gd-facts">
    <div>
      <dt>Layer</dt>
      <dd>{node.layer}</dd>
    </div>
    <div>
      <!-- The number worth opening this window for. -->
      <dt use:tooltip={`${word} that rebuild when this one changes`}>Rebuilds</dt>
      <dd class:gd-hot={node.impact > 0}>{node.impact}</dd>
    </div>
    <div>
      <dt use:tooltip={`${word} this one is built on, transitively`}>Built on</dt>
      <dd>{node.reach}</dd>
    </div>
    <div>
      <dt use:tooltip={'Third-party dependencies it declares'}>Third-party</dt>
      <dd>{node.external}</dd>
    </div>
  </dl>

  <div class="gd-list">
    <div class="gd-list-head">
      <ArrowRight size={11} />
      <span>Depends on</span>
      <span class="gd-n">{dependencies.length}</span>
    </div>
    {#if dependencies.length}
      <ul>
        {#each dependencies as d (d.index)}
          <li>
            <button type="button" onclick={() => onPick(d.index)}>{d.node.name || d.node.id}</button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="gd-none">Nothing in this project — it is at the bottom.</p>
    {/if}
  </div>

  <div class="gd-list">
    <div class="gd-list-head">
      <ArrowLeft size={11} />
      <span>Used by</span>
      <span class="gd-n">{dependents.length}</span>
    </div>
    {#if dependents.length}
      <ul>
        {#each dependents as d (d.index)}
          <li>
            <button type="button" onclick={() => onPick(d.index)}>{d.node.name || d.node.id}</button>
          </li>
        {/each}
      </ul>
    {:else}
      <!-- Stated as the fact and not as a verdict: a published library and a deployable both have no
           internal dependents, and only the reader knows which this is. -->
      <p class="gd-none">Nothing here depends on it.</p>
    {/if}
  </div>

  <button class="gd-open" type="button" onclick={onOpenManifest}>
    <FileCode2 size={12} />
    <span>Open manifest</span>
  </button>
</div>

<style>
  .gd {
    display: flex; flex-direction: column; gap: 8px;
    padding: 8px;
    border-top: 1px solid var(--border-subtle);
    overflow: auto;
    flex-shrink: 0;
    max-height: 46%;
  }
  .gd-head { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .gd-name {
    flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary);
  }
  .gd-kind {
    flex-shrink: 0; padding: 0 5px; border-radius: var(--radius-sm);
    background: var(--bg-overlay); color: var(--text-muted);
    font-family: var(--font-code); font-size: var(--font-size-3xs);
  }

  .gd-cycle {
    display: flex; align-items: center; gap: 6px;
    padding: 4px 6px; border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--error) 14%, transparent);
    color: var(--error); font-size: var(--font-size-2xs);
  }

  .gd-facts {
    display: grid; grid-template-columns: repeat(4, 1fr); gap: 4px;
    margin: 0;
  }
  .gd-facts > div {
    display: flex; flex-direction: column; align-items: center; gap: 1px;
    padding: 4px 2px; border-radius: var(--radius-sm);
    background: var(--bg-base);
  }
  .gd-facts dt {
    font-size: var(--font-size-3xs); color: var(--text-disabled);
    text-transform: uppercase; letter-spacing: 0.03em;
  }
  .gd-facts dd {
    margin: 0;
    font-family: var(--font-code); font-size: var(--font-size-sm); color: var(--text-primary);
  }
  .gd-hot { color: var(--warning); }

  .gd-list { display: flex; flex-direction: column; gap: 2px; }
  .gd-list-head {
    display: flex; align-items: center; gap: 5px;
    color: var(--text-muted); font-size: var(--font-size-2xs);
  }
  .gd-n { margin-left: auto; font-family: var(--font-code); color: var(--text-disabled); }
  .gd-list ul { list-style: none; margin: 0; padding: 0 0 0 16px; display: flex; flex-direction: column; }
  .gd-list li { min-width: 0; }
  .gd-list button {
    width: 100%; padding: 1px 4px; border: none; border-radius: var(--radius-sm);
    background: none; text-align: left; cursor: pointer;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    color: var(--text-secondary); font-family: var(--font-code); font-size: var(--font-size-2xs);
  }
  .gd-list button:hover { background: var(--bg-hover); color: var(--accent); }
  .gd-none {
    margin: 0 0 0 16px;
    color: var(--text-disabled); font-size: var(--font-size-2xs); font-style: italic;
  }

  .gd-open {
    display: flex; align-items: center; gap: 6px; justify-content: center;
    padding: 4px; border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    background: none; color: var(--text-secondary); cursor: pointer;
    font-size: var(--font-size-2xs);
  }
  .gd-open:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>
