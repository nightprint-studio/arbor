<script lang="ts">
  /**
   * Gallery-only: the shared Imber chat widgets assembled from static data, with no product store behind
   * them — the proof that the panel is props-driven. Shows the conversation view; the home is its own scene.
   */
  import { SquarePen } from 'lucide-svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import ImberActivityLine from '$lib/components/shared/internal/imber/ImberActivityLine.svelte';
  import ImberChatPanel from '$lib/components/shared/internal/imber/ImberChatPanel.svelte';
  import ImberComposerFrame from '$lib/components/shared/internal/imber/ImberComposerFrame.svelte';
  import ImberConversationBar from '$lib/components/shared/internal/imber/ImberConversationBar.svelte';
  import ImberMessageFrame from '$lib/components/shared/internal/imber/ImberMessageFrame.svelte';
  import ImberTextBlock from '$lib/components/shared/internal/imber/ImberTextBlock.svelte';
  import ImberToolsFold from '$lib/components/shared/internal/imber/ImberToolsFold.svelte';
  import type { ConversationEntry, ToolCall } from '$lib/components/shared/internal/imber/imber-types';

  const now = Date.now();
  const entries: ConversationEntry[] = [
    { id: 'a', title: 'Why does the build fail?', updatedAt: now - 120_000, subtitle: 'Sonnet · 2 min ago', meta: '€0.04' },
    { id: 'b', title: 'Rename the service layer', updatedAt: now - 3 * 86_400_000, subtitle: 'Sonnet · 3 days ago', meta: '€0.31' },
  ];
  const calls: ToolCall[] = [
    { id: '1', kind: 'read', label: 'Read BandoService.java', detail: '412 lines', ok: true },
    { id: '2', kind: 'search', label: 'Searched “findOpen”', detail: '3 matches', ok: true },
  ];
  let draft = $state('');
</script>

<ImberChatPanel running followKey={0} resetKey="a">
  {#snippet headerActions()}
    <IconButton tooltip="New conversation" size={24}><SquarePen size={14} /></IconButton>
  {/snippet}
  {#snippet bars()}
    <div style="padding: 8px 12px 4px"><ImberConversationBar title="Why does the build fail?" {entries} activeId="a" savedAt={now - 120_000} {now} onSelect={() => {}} onRename={() => {}} onDelete={() => {}} /></div>
  {/snippet}

  <ImberMessageFrame role="user" userText="Why does the build fail on `BandoService`?" plainText="Why does the build fail?" />
  <ImberMessageFrame role="assistant" name="Sonnet" when="10:42" plainText="The build fails because…">
    <ImberToolsFold summary="Read 1 file · Searched once" {calls} />
    <ImberTextBlock text={'The build fails because `findOpen` now returns an `Optional`.\n\n- Unwrap it in the controller\n- Or keep the old signature'} />
  </ImberMessageFrame>
  <ImberActivityLine label="Reading" meta="0:07 · step 2" />

  {#snippet dock()}
    <ImberComposerFrame bind:value={draft} running onSend={() => {}} onStop={() => {}}>
      {#snippet hint()}You approve every edit before it is written.{/snippet}
    </ImberComposerFrame>
  {/snippet}
</ImberChatPanel>
