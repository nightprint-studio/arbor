<script lang="ts">
  /**
   * StreamPanel — the live view of an event stream: every event with its time, id, name and data, newest at
   * the bottom, with pause, clear, a filter by event name, "continue after the last event" and the log as text.
   * Esc stops the stream. The stream itself runs in the backend; nothing here can block the window.
   */
  import { Pause, Play, Square, Trash2, Copy, RotateCw, Save } from 'lucide-svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { textSave } from '$lib/ipc/hirundo';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import ToggleButton from '$lib/components/shared/ui/ToggleButton.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { hirundoSse } from '$lib/stores/hirundo/sse.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';

  let listEl = $state<HTMLElement | undefined>(undefined);
  let saving = $state(false);
  const tone = (s: string) => (s === 'open' ? 'success' : s === 'connecting' || s === 'reconnecting' ? 'warning' : 'neutral');
  const time = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });

  // Stay at the bottom while events arrive, unless paused.
  $effect(() => {
    void hirundoSse.items.length;
    if (!hirundoSse.paused) queueMicrotask(() => listEl?.scrollTo({ top: listEl.scrollHeight }));
  });

  async function saveLog(file: string) {
    saving = false;
    try { await textSave(file, hirundoSse.log() + '\n'); toastStore.show(`Log saved to ${file}.`, 'success'); } catch (e) { toastStore.show(String(e), 'error'); }
  }

  async function copyLog() {
    try { await navigator.clipboard.writeText(hirundoSse.log()); toastStore.show('Log copied.', 'success'); } catch { toastStore.show('Could not reach the clipboard.', 'error'); }
  }
</script>

<div class="sp">
  <div class="sp-top">
    <Badge variant="status" size="sm" tone={tone(hirundoSse.state)} label={hirundoSse.state} />
    {#if hirundoSse.running}
      <Button size="sm" variant="danger" tooltip={{ content: 'Stop the stream', shortcut: 'Esc' }} onclick={() => void hirundoSse.stop()}>{#snippet iconStart()}<Square size={12} />{/snippet}Stop</Button>
    {:else}
      <Button size="sm" variant="primary" disabled={!hirundoStore.active} tooltip={{ content: 'Listen to the open request', shortcut: 'Ctrl+Shift+L' }} onclick={() => hirundoStore.active && void hirundoSse.start(hirundoStore.active)}>{#snippet iconStart()}<Play size={12} />{/snippet}Listen</Button>
      {#if hirundoSse.lastEventId && hirundoStore.active}
        <IconButton tooltip={`Continue after event ${hirundoSse.lastEventId}`} size={26} bordered onclick={() => hirundoStore.active && void hirundoSse.start(hirundoStore.active, { resume: true })}><RotateCw size={13} /></IconButton>
      {/if}
    {/if}
    <span class="sp-spacer"></span>
    <IconButton tooltip={hirundoSse.paused ? `Resume (${hirundoSse.buffered} waiting)` : 'Pause the view'} size={26} bordered active={hirundoSse.paused} onclick={() => hirundoSse.setPaused(!hirundoSse.paused)}><Pause size={13} /></IconButton>
    <IconButton tooltip="Clear" size={26} bordered onclick={() => hirundoSse.clear()}><Trash2 size={13} /></IconButton>
    <IconButton tooltip="Save the log to a file (one JSON event per line)" size={26} bordered disabled={hirundoSse.events === 0} onclick={() => (saving = true)}><Save size={13} /></IconButton>
    <IconButton tooltip="Copy the log (one JSON event per line)" size={26} bordered disabled={hirundoSse.events === 0} onclick={() => void copyLog()}><Copy size={13} /></IconButton>
  </div>

  {#if hirundoSse.title}<p class="sp-title" title={hirundoSse.title}>{hirundoSse.title} · {hirundoSse.events} events</p>{/if}

  {#if hirundoSse.names.length > 1}
    <div class="sp-filter">
      {#each hirundoSse.names as n (n)}<ToggleButton pressed={!hirundoSse.hidden.includes(n)} label={n} title={`Show ${n} events`} onclick={() => hirundoSse.toggleName(n)} />{/each}
    </div>
  {/if}

  {#if hirundoSse.items.length === 0}
    <div class="sp-empty">
      <EmptyState message="No events yet" description="Listen to the open request: the events of a text/event-stream response show here as they arrive." compact />
      {#if !hirundoSse.running}<Button variant="primary" disabled={!hirundoStore.active} onclick={() => hirundoStore.active && void hirundoSse.start(hirundoStore.active)}>Listen</Button>{/if}
    </div>
  {:else}
    <ul class="sp-list" bind:this={listEl}>
      {#each hirundoSse.items as m, i (i)}
        <li class={m.kind}>
          <span class="sp-t">{time(m.at_ms)}</span>
          {#if m.kind === 'event'}
            <Badge variant="chip" size="sm" label={m.event ?? 'message'} />
            {#if m.id}<span class="sp-id">#{m.id}</span>{/if}
            <code class="sp-data">{m.data}</code>
          {:else}
            <span class="sp-sys">{m.kind}{m.message ? ` — ${m.message}` : ''}{m.reason ? ` — ${m.reason}` : ''}{m.in_ms ? ` in ${m.in_ms} ms` : ''}{m.status ? ` (${m.status})` : ''}</span>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

{#if saving}
  <FileExplorerModal mode="save" title="Save the event log" extensions={['jsonl', 'log', 'txt']} initialFilename="events.jsonl" onConfirm={(f: string) => void saveLog(f)} onCancel={() => (saving = false)} onClose={() => (saving = false)} />
{/if}

{#if hirundoSse.confirm}
  {@const c = hirundoSse.confirm}
  <ConfirmModal title="Listen in a protected environment?" message={`${c.method} ${c.url}`} variant="warning" confirmLabel="Listen" onConfirm={() => { hirundoSse.dismissConfirm(); void hirundoSse.start(c.tab, { confirmed: true }); }} onCancel={() => hirundoSse.dismissConfirm()} />
{/if}

<style>
  .sp { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .sp-empty { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; padding: 16px; }
  .sp-top { display: flex; align-items: center; gap: 6px; padding: 8px 10px; }
  .sp-spacer { flex: 1; }
  .sp-title { margin: 0; padding: 0 12px 4px; font-size: var(--font-size-xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sp-filter { display: flex; gap: 4px; flex-wrap: wrap; padding: 0 10px 6px; }
  .sp-list { list-style: none; margin: 0; padding: 0 8px 8px; overflow: auto; flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 2px; font-size: var(--font-size-xs); }
  .sp-list li { display: flex; align-items: baseline; gap: 8px; padding: 2px 4px; border-bottom: 1px solid var(--border-subtle); }
  .sp-t { color: var(--text-muted); font-variant-numeric: tabular-nums; flex: none; }
  .sp-id { color: var(--text-muted); flex: none; }
  .sp-data { font-family: var(--font-mono); word-break: break-all; white-space: pre-wrap; min-width: 0; }
  .sp-sys { color: var(--text-secondary); font-style: italic; }
  li.error .sp-sys { color: var(--error); }
</style>
