<script lang="ts">
  /**
   * The Input panel — a virtual keyboard, mouse and gamepad for the running game, the game's registered
   * actions, the presence indicator, and a recorder: everything sent while recording becomes a script
   * that can be replayed with the original timing, copied as JSON, or loaded back. Needs the
   * `arbor-bevy-remote` plugin in the game.
   */
  import { onMount } from 'svelte';
  import { Circle, Copy, Play, Square, Trash2 } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import ColumnPanes from '$lib/components/shared/ui/ColumnPanes.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import RowList from '$lib/components/shared/ui/RowList.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import ToggleButton from '$lib/components/shared/ui/ToggleButton.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { brpDriveStore as drive } from '$lib/stores/bennu/brp-drive.svelte';
  import { describeStep } from './brp-script';

  const MOUSE_ACTIONS = ['click', 'double_click', 'move', 'down', 'up', 'drag', 'wheel'].map((v) => ({ value: v, label: v.replace('_', ' ') }));
  const MODS = ['ctrl', 'shift', 'alt'] as const;

  let key = $state('');
  let mods = $state<string[]>([]);
  let text = $state('');
  let mouseAction = $state('click');
  let x = $state('');
  let y = $state('');
  let toX = $state('');
  let toY = $state('');
  let pad = $state('South');
  let presenceLabel = $state('Arbor');
  let presenceOn = $state(true);
  let importText = $state('');
  let importError = $state<string | null>(null);

  onMount(() => void drive.loadInfo());

  const padButtons = $derived((drive.info?.gamepad_buttons ?? ['South']).map((b) => ({ value: b, label: b })));
  const num = (s: string) => (s.trim() === '' || Number.isNaN(Number(s)) ? undefined : Number(s));

  function toggleMod(m: string) {
    mods = mods.includes(m) ? mods.filter((k) => k !== m) : [...mods, m];
  }

  const sendKey = () => key.trim() && void drive.send('key', { key: key.trim(), ...(mods.length ? { modifiers: mods } : {}) });
  const sendText = () => text && void drive.send('text', { text });
  const sendMouse = () =>
    void drive.send('mouse', {
      action: mouseAction,
      ...(num(x) !== undefined ? { x: num(x) } : {}), ...(num(y) !== undefined ? { y: num(y) } : {}),
      ...(num(toX) !== undefined ? { to_x: num(toX) } : {}), ...(num(toY) !== undefined ? { to_y: num(toY) } : {}),
    });
  const sendPad = () => void drive.send('gamepad', { button: pad });

  /** Enter sends, from any single-line field of its section. */
  const onEnter = (send: () => void) => (e: KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      send();
    }
  };

  async function applyPresence() {
    try {
      await drive.setPresence({ enabled: presenceOn, label: presenceLabel, state: 'connected' });
    } catch (e) {
      toastStore.show(e instanceof Error ? e.message : String(e), 'error');
    }
  }

  async function runAction(name: string) {
    try {
      await drive.runAction(name);
      toastStore.show(`Ran “${name}”.`, 'success');
    } catch (e) {
      toastStore.show(e instanceof Error ? e.message : String(e), 'error');
    }
  }

  function importScript() {
    importError = drive.loadScript(importText);
    if (!importError) importText = '';
  }
</script>

<ColumnPanes leftWidth="420px">
  {#snippet left()}
    <PaneHeader title="Send" />
    <div class="in-body">
      {#if drive.unavailable}
        <Alert variant="warning" compact text={drive.unavailable} />
      {/if}

      <div class="in-line">
        <span class="in-lab">Key</span>
        <div class="in-ctl">
          <Input bind:value={key} size="sm" placeholder="a · Enter · Escape · F5 · ArrowUp" ariaLabel="Key" onkeydown={onEnter(sendKey)} />
          {#each MODS as m (m)}
            <ToggleButton pressed={mods.includes(m)} label={m} title={`Hold ${m} during the press`} compact onclick={() => toggleMod(m)} />
          {/each}
          <Button size="sm" variant="primary" disabled={!key.trim()} onclick={sendKey}>Press</Button>
        </div>
      </div>

      <div class="in-line">
        <span class="in-lab">Text</span>
        <div class="in-ctl">
          <Input bind:value={text} size="sm" placeholder="Typed into whatever has the focus" ariaLabel="Text" onkeydown={onEnter(sendText)} />
          <Button size="sm" variant="primary" disabled={!text} onclick={sendText}>Type</Button>
        </div>
      </div>

      <div class="in-line">
        <span class="in-lab" title="Logical window pixels, origin top-left. The Screen tab's “Click through” fills these in for you.">Mouse</span>
        <div class="in-ctl">
          <Select value={mouseAction} options={MOUSE_ACTIONS} size="sm" onchange={(v) => (mouseAction = v)} />
          <Input bind:value={x} size="sm" narrow placeholder="x" ariaLabel="x" onkeydown={onEnter(sendMouse)} />
          <Input bind:value={y} size="sm" narrow placeholder="y" ariaLabel="y" onkeydown={onEnter(sendMouse)} />
          {#if mouseAction === 'drag'}
            <Input bind:value={toX} size="sm" narrow placeholder="to x" ariaLabel="to x" onkeydown={onEnter(sendMouse)} />
            <Input bind:value={toY} size="sm" narrow placeholder="to y" ariaLabel="to y" onkeydown={onEnter(sendMouse)} />
          {/if}
          <Button size="sm" variant="primary" onclick={sendMouse}>Send</Button>
        </div>
      </div>

      <div class="in-line">
        <span class="in-lab">Gamepad</span>
        <div class="in-ctl">
          <Select value={pad} options={padButtons} size="sm" onchange={(v) => (pad = v)} />
          <Button size="sm" variant="primary" onclick={sendPad}>Press</Button>
        </div>
      </div>

      <div class="in-line">
        <span class="in-lab" title="The coloured border and label in the game while a client is attached.">Indicator</span>
        <div class="in-ctl">
          <Toggle checked={presenceOn} size="sm" label="Show" onchange={(v) => { presenceOn = v; void applyPresence(); }} />
          <Input bind:value={presenceLabel} size="sm" placeholder="Label" ariaLabel="Indicator label" onchange={() => void applyPresence()} />
        </div>
      </div>

      <FormField label="Game actions">
        {#if (drive.info?.actions ?? []).length === 0}
          <span class="in-muted">The game registered no actions.</span>
        {:else}
          <RowList items={drive.info?.actions ?? []} key={(a) => a.name}>
            {#snippet row(a)}
              <span class="in-name">{a.name}</span>
              <span class="in-desc">{a.description}</span>
              <Button size="sm" variant="ghost" onclick={() => void runAction(a.name)}>Run</Button>
            {/snippet}
          </RowList>
        {/if}
      </FormField>
    </div>
  {/snippet}

  {#snippet right()}
    <PaneHeader title="Script">
      {#snippet end()}
        <span class="in-muted">{drive.steps.length} step{drive.steps.length === 1 ? '' : 's'}</span>
        {#if drive.recording}
          <Button size="sm" variant="danger" onclick={() => drive.stopRecording()}>
            {#snippet iconStart()}<Square size={12} />{/snippet}
            Stop
          </Button>
        {:else}
          <Button size="sm" variant="outline" onclick={() => drive.startRecording()}>
            {#snippet iconStart()}<Circle size={12} />{/snippet}
            Record
          </Button>
        {/if}
        {#if drive.replaying}
          <Button size="sm" variant="primary" onclick={() => void drive.stopReplay()}>
            {#snippet iconStart()}<Square size={12} />{/snippet}
            Stop replay
          </Button>
        {:else}
          <Button size="sm" variant="primary" disabled={drive.steps.length === 0 || drive.recording} onclick={() => void drive.replay()}>
            {#snippet iconStart()}<Play size={12} />{/snippet}
            Replay
          </Button>
        {/if}
        <IconButton tooltip="Copy the script as JSON" size={22} disabled={drive.steps.length === 0} onclick={() => void copyToClipboard(drive.scriptJson(), { successToast: 'Copied the script', errorToast: true })}>
          <Copy size={13} />
        </IconButton>
        <IconButton tooltip="Clear the script" size={22} disabled={drive.steps.length === 0} onclick={() => drive.clearScript()}>
          <Trash2 size={13} />
        </IconButton>
      {/snippet}
    </PaneHeader>
    <div class="in-body">
      {#if drive.steps.length === 0}
        <EmptyState message={drive.recording ? 'Recording — send some input' : 'No script'} description="Press Record, send input on the left, then Replay. Or paste a script below." />
      {:else}
        <RowList items={drive.steps.map((s, i) => ({ s, i }))} key={(r) => String(r.i)}>
          {#snippet row({ s, i })}
            <span class="in-time">{(s.at / 1000).toFixed(2)} s</span>
            <span class="in-step" class:now={drive.replayAt === i}>{describeStep(s)}</span>
          {/snippet}
        </RowList>
      {/if}
      <Collapsible chevron>
        {#snippet header()}<span class="in-lab">Load a script</span>{/snippet}
        <div class="in-load">
          <Textarea bind:value={importText} framed minRows={3} maxRows={8} placeholder={'{"steps":[{"at":0,"call":"key","args":{"key":"Enter"}}]}'} ariaLabel="Script JSON" onsubmit={importScript} />
          {#if importError}<Alert variant="error" compact text={importError} />{/if}
          <Button size="sm" variant="outline" disabled={!importText.trim()} onclick={importScript}>Load</Button>
        </div>
      </Collapsible>
    </div>
  {/snippet}
</ColumnPanes>

<style>
  .in-body { flex: 1; min-height: 0; overflow-y: auto; padding: 8px 10px; display: flex; flex-direction: column; gap: 6px; }
  .in-line { display: grid; grid-template-columns: 64px minmax(0, 1fr); align-items: center; gap: 8px; }
  .in-lab { font-size: var(--font-size-xs); font-weight: 600; color: var(--text-secondary); }
  .in-ctl { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; min-width: 0; }
  .in-ctl > :global(:first-child) { flex: 1; min-width: 90px; }
  .in-load { display: flex; flex-direction: column; gap: 6px; padding: 4px 0; }
  .in-muted { font-size: var(--font-size-xs); color: var(--text-muted); }
  .in-name { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); }
  .in-desc { flex: 1; min-width: 0; font-size: var(--font-size-xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .in-time { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .in-step { font-size: var(--font-size-sm); color: var(--text-primary); }
  .in-step.now { color: var(--accent); font-weight: 600; }
</style>
