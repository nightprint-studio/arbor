<script lang="ts">
  /**
   * CookiesPanel — the cookie jar of the active environment: what answers have set, with domain, path and
   * expiry, and a way to delete one or empty the jar. The jar lives in your profile, not in the project, because
   * a session cookie is a credential; each environment has its own, and a request opts out in its settings.
   */
  import { Pencil, Plus, Trash2 } from 'lucide-svelte';
  import CookieModal from '../modals/CookieModal.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import RowList from '$lib/components/shared/ui/RowList.svelte';
  import { cookieDelete, cookiesClear, cookiesList } from '$lib/ipc/hirundo';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import type { Cookie } from '$lib/types/hirundo';
  import { hirundoCommands } from '../hirundo-commands';

  let cookies = $state<Cookie[]>([]);
  /** The cookie being edited; `'new'` for a blank one. */
  let editing = $state<Cookie | 'new' | null>(null);
  const envId = $derived(hirundoStore.activeEnv?.id ?? '');

  async function load() {
    const project = hirundoStore.project;
    cookies = project ? await cookiesList(project, envId).catch(() => []) : [];
  }

  $effect(() => {
    void hirundoStore.project;
    void envId;
    void load();
  });

  const expiry = (ms: number | null) => (ms === null ? 'session' : new Date(ms).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' }));
  const key = (c: Cookie) => `${c.domain}|${c.path}|${c.name}`;

  async function remove(c: Cookie) {
    if (hirundoStore.project) await cookieDelete(hirundoStore.project, envId, c).catch(() => {});
    await load();
  }

  async function clear() {
    if (hirundoStore.project) await cookiesClear(hirundoStore.project, envId).catch(() => {});
    await load();
  }
</script>

<div class="cp">
  {#if !hirundoStore.project}
    <div class="cp-empty"><EmptyState message="No project open" description="The cookie jar belongs to an environment of a project." compact /><Button variant="primary" onclick={hirundoCommands.openProject}>Open project…</Button></div>
  {:else}
    <div class="cp-top">
      <span class="cp-env">Jar of <b>{hirundoStore.activeEnv?.name ?? 'no environment'}</b></span>
      <span class="cp-spacer"></span>
      <Button size="sm" variant="outline" onclick={() => (editing = 'new')}>{#snippet iconStart()}<Plus size={12} />{/snippet}Add</Button>
      <Button size="sm" variant="outline" disabled={cookies.length === 0} onclick={() => void clear()}>Clear all</Button>
    </div>
    {#if cookies.length === 0}
      <div class="cp-empty">
        <EmptyState message="The jar is empty" description="Cookies an answer sets are kept here and sent back to the same site." compact />
        <Button variant="outline" onclick={() => (editing = 'new')}>Add a cookie…</Button>
      </div>
    {:else}
      <div class="cp-list">
        <RowList items={cookies} key={key}>
          {#snippet row(c)}
            <div class="cp-main">
              <b>{c.name}</b>
              <span class="cp-sub">{c.domain}{c.path} · {expiry(c.expires_ms)}</span>
              <code class="cp-value">{c.value}</code>
            </div>
            {#if c.secure}<Badge variant="chip" size="sm" label="Secure" />{/if}
            {#if c.http_only}<Badge variant="chip" size="sm" label="HttpOnly" />{/if}
            <span class="rl-keep"><IconButton tooltip="Edit this cookie" size={22} onclick={() => (editing = c)}><Pencil size={12} /></IconButton><IconButton tooltip="Delete this cookie" size={22} variant="danger" onclick={() => void remove(c)}><Trash2 size={12} /></IconButton></span>
          {/snippet}
        </RowList>
      </div>
    {/if}
  {/if}
</div>

{#if editing}
  <CookieModal cookie={editing === 'new' ? null : editing} {envId} onClose={() => (editing = null)} onSaved={() => void load()} />
{/if}

<style>
  .cp { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .cp-empty { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; padding: 16px; }
  .cp-top { display: flex; align-items: center; gap: 8px; padding: 8px 12px; }
  .cp-spacer { flex: 1; }
  .cp-env { font-size: var(--font-size-xs); color: var(--text-secondary); }
  .cp-list { padding: 0 12px 8px; overflow: auto; flex: 1; min-height: 0; }
  .cp-main { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .cp-sub { font-size: var(--font-size-xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cp-value { font-family: var(--font-mono); font-size: var(--font-size-xs); color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
