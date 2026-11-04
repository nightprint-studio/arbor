<script lang="ts">
  /**
   * The conversation you are in, and the way to another — Bennu's data poured into the shared Imber
   * conversation bar: each entry says which model it used and when it was last touched, with its cost at
   * the edge.
   */
  import ImberConversationBar from '$lib/components/shared/internal/imber/ImberConversationBar.svelte';
  import type { ConversationEntry } from '$lib/components/shared/internal/imber/imber-types';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { formatEur, relativeTime } from './assistant-format';

  const store = bennuAssistantStore;

  // The clock "5 min ago" is read against. The panel is not a ticking one, so it is taken again only
  // when the conversations change — which is when a time could have.
  const now = $derived.by(() => {
    void store.visible;
    return Date.now();
  });

  const entries = $derived<ConversationEntry[]>(
    store.visible.map((c) => ({
      id: c.id,
      title: c.title,
      updatedAt: c.updatedAt,
      subtitle: `${store.models.find((m) => m.id === c.model)?.name ?? c.model} · ${relativeTime(c.updatedAt, now)}`,
      meta: formatEur(store.eur(c.cost)),
    })),
  );
</script>

<ImberConversationBar
  title={store.active?.title ?? null}
  {entries}
  activeId={store.active?.id ?? null}
  savedAt={store.active?.updatedAt ?? null}
  {now}
  onSelect={(id) => store.select(id)}
  onRename={(id, title) => store.rename(id, title)}
  onDelete={(id) => store.askRemove(id)}
/>
