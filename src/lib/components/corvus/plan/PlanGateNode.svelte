<script lang="ts">
  /**
   * The connector between two phases: a node on the wire that says *when the next phase starts*.
   * Waiting gates pulse and offer a way out ("Skip wait"); a click opens the condition menu.
   */
  import { Check, X } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import type { Plan, PlanGate } from '$lib/types/corvus/plan';
  import { GATE_META, gateLabel } from './plan-meta';

  interface Props {
    plan: Plan;
    gate: PlanGate;
    selected?: boolean;
    interactive?: boolean;
    /** Open the condition menu at the node. */
    onEdit: (at: { x: number; y: number }) => void;
    onRelease: () => void;
  }

  let { plan, gate, selected = false, interactive = true, onEdit, onRelease }: Props = $props();

  const meta = $derived(GATE_META[gate.kind]);
  const Icon = $derived(gate.state === 'open' ? Check : gate.state === 'blocked' ? X : meta.icon);

  function open(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    onEdit({ x: r.left, y: r.bottom + 6 });
  }
</script>

<div class="gate st-{gate.state}" class:selected>
  <button
    type="button"
    class="node"
    disabled={!interactive}
    aria-label={`Start condition: ${gateLabel(plan, gate)}`}
    data-gate-node
    onclick={open}
  >
    <Icon size={15} />
  </button>
  <span class="label">{gateLabel(plan, gate)}</span>
  {#if gate.state === 'waiting' || gate.state === 'blocked'}
    <Button size="xs" variant="secondary" onclick={onRelease}>
      {gate.kind === 'manual' ? 'Go ahead' : 'Skip wait'}
    </Button>
  {/if}
</div>

<style>
  .gate {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    width: 116px;
    flex: none;
    padding-top: 6px;
  }
  /* the wire */
  .gate::before {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: 22px;
    border-top: 2px solid var(--border);
  }
  .st-open::before { border-color: color-mix(in srgb, var(--success) 55%, transparent); }
  .st-waiting::before { border-top-style: dashed; border-color: var(--warning); }
  .st-blocked::before { border-top-style: dotted; border-color: var(--danger); }

  .node {
    position: relative;
    z-index: 1;
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 50%;
    border: 2px solid var(--border);
    background: var(--bg-elevated);
    color: var(--text-muted);
    cursor: pointer;
  }
  .node:hover:not(:disabled), .selected .node { border-color: var(--accent); color: var(--accent); }
  .node:disabled { cursor: default; }
  .st-open .node { border-color: var(--success); color: var(--success); }
  .st-waiting .node {
    border-color: var(--warning);
    color: var(--warning);
    background: color-mix(in srgb, var(--warning) 14%, var(--bg-elevated));
  }
  .st-waiting .node::after {
    content: '';
    position: absolute;
    inset: -6px;
    border-radius: 50%;
    border: 2px solid var(--warning);
    opacity: 0;
    animation: ring 1.8s ease-out infinite;
  }
  .st-blocked .node { border-color: var(--danger); color: var(--danger); }
  @keyframes ring {
    from { opacity: 0.55; transform: scale(0.7); }
    to   { opacity: 0;    transform: scale(1.25); }
  }
  @media (prefers-reduced-motion: reduce) { .st-waiting .node::after { animation: none; } }

  .label { max-width: 108px; text-align: center; font-size: 11px; line-height: 1.35; color: var(--text-muted); }
</style>
