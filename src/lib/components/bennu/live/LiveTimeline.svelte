<script lang="ts">
  /**
   * The unified timeline: what every process printed, each HTTP request and its answer, and the
   * starts and exits, in one chronological stream. Filter by text or kind, or by a correlation id to
   * see everything one request touched across every service.
   */
  import { Eraser, Route } from 'lucide-svelte';
  import ChipBar from '$lib/components/shared/ui/ChipBar.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import { bennuLiveStore as live } from '$lib/stores/bennu/live.svelte';
  import type { LiveKindFilter } from '$lib/ipc/bennu/live';
  import LiveLogView from './LiveLogView.svelte';
  import { eventClass, eventText } from './live-format';

  const KINDS = [
    { id: 'log', label: 'Logs' },
    { id: 'http', label: 'HTTP' },
    { id: 'marker', label: 'Lifecycle' },
  ];

  let text = $state(live.filterText);
  let corr = $state(live.filterCorr);
  // The store can change the filter (a scenario step's "follow this request"); the boxes mirror it.
  $effect(() => { corr = live.filterCorr; });

  // Debounced: every keystroke would otherwise re-read the whole timeline.
  let timer: ReturnType<typeof setTimeout> | undefined;
  function typed(value: string) {
    text = value;
    clearTimeout(timer);
    timer = setTimeout(() => void live.setFilter({ text: value }), 200);
  }
  $effect(() => () => clearTimeout(timer));

  const lines = $derived(live.events.map(eventText));
  const classes = $derived(live.events.map(eventClass));
  const keys = $derived(live.events.map((e) => e.seq));
</script>

<div class="tl">
  <div class="tl-bar">
    <div class="tl-search">
      <SearchBar query={text} showRegex={false} showCounter={false} placeholder="Filter the timeline…" ariaLabel="Filter the timeline" oninput={typed} onClear={() => typed('')} />
    </div>
    <ChipBar
      items={KINDS}
      selected={live.filterKinds}
      multi
      size="sm"
      ariaLabel="Kinds of event"
      onSelect={(sel) => void live.setFilter({ kinds: sel as LiveKindFilter[] })}
    />
    <div class="tl-corr">
      <Input
        value={corr}
        size="sm"
        placeholder="correlation id"
        ariaLabel="Correlation id"
        clearable
        onchange={(v) => void live.setFilter({ corr: v.trim() })}
        onclear={() => void live.setFilter({ corr: '' })}
      >
        {#snippet iconStart()}<Route size={12} />{/snippet}
      </Input>
    </div>
    <IconButton tooltip="Clear the timeline" size={22} onclick={() => void live.clearTimeline()}><Eraser size={12} /></IconButton>
  </div>
  <div class="tl-body">
    <LiveLogView {lines} {classes} {keys} emptyMessage="Nothing on the timeline yet. Start a process or send a request." />
  </div>
</div>

<style>
  .tl { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .tl-bar { display: flex; align-items: center; gap: 8px; padding: 5px 10px; border-bottom: 1px solid var(--border-subtle); flex: none; }
  .tl-search { flex: 1; min-width: 160px; max-width: 360px; }
  .tl-corr { width: 220px; }
  .tl-body { flex: 1; min-height: 0; }
</style>
