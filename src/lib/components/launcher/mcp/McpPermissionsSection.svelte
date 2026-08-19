<!--
  What a tool may do, for a project that says nothing of its own.

  Deliberately the *defaults* page rather than "the settings": the per-project rules
  inherit from here, so what this page really controls is every project that never
  disagreed with it — which is a bigger blast radius than a page titled "policy" suggests
  and is worth saying on the page itself.
-->
<script lang="ts">
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import { mcpStore } from '$lib/stores/mcp.svelte';
  import { MCP_SAFETY_TIERS } from '$lib/types/mcp';
  import type { McpDecision } from '$lib/types/mcp';

  let { guard }: { guard: (action: () => Promise<void>) => Promise<void> } = $props();

  const cfg = $derived(mcpStore.config);

  const DECISIONS: { value: McpDecision; label: string; description: string }[] = [
    { value: 'allow', label: 'Allow',  description: 'Runs without asking. Still logged.' },
    { value: 'ask',   label: 'Ask',    description: 'Prompts you each time.' },
    { value: 'deny',  label: 'Refuse', description: 'Never runs.' },
  ];

  /** How many projects have opted out of each default — the honest caveat to this page. */
  const overridden = $derived(
    cfg.projects.filter(
      (p) => p.policy.read !== null || p.policy.write !== null || p.policy.destructive !== null,
    ).length,
  );
</script>

<section>
  <SectionHeader
    title="What tools may do"
    description="Tools declare what class of action they perform. Prompting for everything trains you to approve without reading, so the prompts are spent where they carry information." />

  {#each MCP_SAFETY_TIERS as tier (tier.key)}
    <div class="tier">
      <div class="tier-label">
        <strong>{tier.title}</strong>
        <span>{tier.blurb}</span>
      </div>
      <RadioGroup
        value={cfg.policy[tier.key]}
        size="sm"
        nowrap
        options={DECISIONS}
        onchange={(v: string) => guard(() => mcpStore.setPolicy(tier.key, v as McpDecision))} />
    </div>
  {/each}

  <p class="hint">
    These are the defaults every project inherits.
    {#if overridden > 0}
      {overridden} project{overridden === 1 ? ' has' : 's have'} rules of their own and will not follow a change made here.
    {/if}
  </p>
</section>

<section>
  <SectionHeader
    title="Prompts"
    description="A prompt nobody is at must not hold a tool call — and the model's turn — open forever, so it answers no on your behalf when it runs out." />
  <div class="row">
    <label for="mcp-timeout">Seconds a prompt waits before refusing</label>
    <Input
      id="mcp-timeout"
      type="number"
      block={false}
      narrow
      value={String(cfg.consent_timeout_secs)}
      onchange={(v: string) => guard(() => mcpStore.patch({ consent_timeout_secs: Number(v) || 120 }))} />
  </div>
  <div>
    <Button variant="ghost" onclick={() => guard(() => mcpStore.revokeGrants())}>
      Revoke "allow for this session" grants
    </Button>
  </div>
  <p class="hint">
    Those grants live in memory only, so they are also gone on restart and whenever these
    settings change — a tightened policy that left old grants standing would not be tightened.
  </p>
</section>

<style>
  section   { display: flex; flex-direction: column; gap: 10px; }
  .tier     { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
  .tier-label { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .tier-label strong { font-size: 12.5px; color: var(--text-primary); }
  .tier-label span   { font-size: 11.5px; line-height: 1.45; color: var(--text-tertiary); }
  .row      { display: flex; align-items: center; gap: 10px; }
  .row label{ font-size: 12px; color: var(--text-secondary); }
  .hint     { margin: 0; font-size: 11.5px; line-height: 1.5; color: var(--text-tertiary); }
</style>
