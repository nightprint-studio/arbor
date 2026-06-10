<script lang="ts">
  /**
   * Docs — a compact grove language reference (like Arbor's DocsPanel). Mocked
   * content grouped into collapsible sections with a search, an example, and
   * symbol/description rows. The real version mirrors design/grove/*.md.
   */
  import { BookOpen, Search, Hash, Braces, WandSparkles, Music } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';

  let query = $state('');

  const SECTIONS = [
    { id: 'host', label: 'Host language', icon: Braces, color: 'var(--accent)', rows: [
      ['cps(n)', 'cycles per second (tempo)'],
      ['let / fn', 'bindings & expression functions'],
      ['par · seq · cat', 'compose patterns (stack / sequence / alternate)'],
      ['arrange · cycles', 'absolute-timeline sections'],
      ['tracks · track', 'output: named channels'],
      ['(0..8).par(i => …)', 'range map + combine'],
    ] },
    { id: 'mini', label: 'Mini-notation', icon: Hash, color: 'var(--info)', rows: [
      ['~', 'a silent slot (rest)'],
      ['_', 'extend the previous term by a slot'],
      ['[ ]', 'group events into one slot'],
      ['< >', 'alternate — one element per cycle'],
      ['&', 'parallel (stack) — loosest precedence'],
      ['*n  /n', 'fast / slow inside the slot'],
      ['!n  @n', 'replicate / weight'],
      ['(n,k)', 'euclidean — n hits over k steps'],
      [":n  'chord", 'sample variant / chord (n only)'],
      ['$ident', 'splice a variable as a leaf'],
    ] },
    { id: 'xform', label: 'Transforms', icon: WandSparkles, color: 'var(--color-tag, #c792ea)', rows: [
      ['fast · slow · rev', 'time & structure'],
      ['every · off', 'periodic / echo'],
      ['degrade · sometimes · jux', 'probability & stereo'],
      ['gain · pan · room · lpf', 'voice & mix'],
      ['inst · scale', 'instrument / degree mapping'],
      ['rand(lo,hi) · choose(…)', 'generative values'],
    ] },
  ];

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return SECTIONS;
    return SECTIONS
      .map(s => ({ ...s, rows: s.rows.filter(r => r[0].toLowerCase().includes(q) || r[1].toLowerCase().includes(q)) }))
      .filter(s => s.rows.length > 0);
  });

  // Section open state (default open).
  let open = $state<Record<string, boolean>>({});
  const isOpen = (id: string) => open[id] ?? true;
</script>

<PanelShell title="Docs">
  {#snippet icon()}<BookOpen size={13} />{/snippet}
  {#snippet toolbar()}
    <div class="docs-search">
      <Input bind:value={query} placeholder="Search the language…" size="sm">
        {#snippet iconStart()}<Search size={13} />{/snippet}
      </Input>
    </div>
  {/snippet}

  <div class="docs">
    {#if !query}
      <div class="docs-example">
        <div class="docs-example-head"><Music size={11} /> Example</div>
        <pre class="docs-code">n(c4 e4 g4)<span class="op">.</span><span class="fn">slow</span>(2)<span class="op">.</span><span class="fn">inst</span>(<span class="str">"synth.pad"</span>)</pre>
      </div>
    {/if}

    {#each filtered as sec (sec.id)}
      {@const Si = sec.icon}
      <SidebarSection
        label={sec.label}
        expanded={isOpen(sec.id)}
        onToggle={() => open = { ...open, [sec.id]: !isOpen(sec.id) }}
        badge={sec.rows.length}
        iconColor={sec.color}
      >
        {#snippet icon()}<Si size={13} />{/snippet}
        {#each sec.rows as row}
          <div class="docs-row">
            <code class="docs-sym" style="color: {sec.color}">{row[0]}</code>
            <span class="docs-desc">{row[1]}</span>
          </div>
        {/each}
      </SidebarSection>
    {/each}

    {#if filtered.length === 0}
      <div class="docs-empty">No matches for “{query}”.</div>
    {/if}
  </div>
</PanelShell>

<style>
  .docs-search { padding: 6px 8px; }
  .docs { padding: 4px 0 12px; }

  .docs-example { margin: 6px 10px 8px; }
  .docs-example-head {
    display: flex; align-items: center; gap: 5px;
    font-size: 9.5px; text-transform: uppercase; letter-spacing: 0.4px;
    color: var(--text-muted); margin-bottom: 4px;
  }
  .docs-code {
    margin: 0; padding: 8px 10px;
    background: var(--bg-input); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font-family: var(--font-code); font-size: 11.5px; line-height: 1.5;
    color: var(--color-stash, #82aaff); white-space: pre-wrap;
  }
  .docs-code .op { color: var(--text-secondary); }
  .docs-code .fn { color: #61afef; }
  .docs-code .str { color: #98c379; }

  .docs-row { display: flex; gap: 10px; padding: 3px 4px; align-items: baseline; }
  .docs-sym { flex-shrink: 0; min-width: 96px; font-family: var(--font-code); font-size: 11px; }
  .docs-desc { font-size: 11.5px; color: var(--text-secondary); line-height: 1.45; }

  .docs-empty { padding: 14px 12px; font-size: 11.5px; color: var(--text-muted); font-style: italic; }
</style>
