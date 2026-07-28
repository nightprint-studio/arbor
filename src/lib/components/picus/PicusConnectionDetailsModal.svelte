<script lang="ts">
  /**
   * Connection details — what a connection *is*, without opening the editor.
   *
   * Reading a setting and changing it are different intentions, and a form is a
   * bad answer to the first: fields invite typing, and the question "which
   * database does production actually point at, and is it read-only" deserves an
   * answer you cannot accidentally edit. So this is flat text, in the same
   * sections the editor uses — identity, server, session, credentials — so the
   * two dialogs describe the connection in the same order.
   *
   * It knows the password only as a yes or a no. `hasSecret` is the single fact
   * the backend reports about it; the secret itself never leaves Arbor's keychain,
   * and there is nothing here that could show it.
   */
  import { Database, Pencil, Trash2, Lock, ShieldCheck, ShieldOff } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import PicusDialectChip from './PicusDialectChip.svelte';
  import { connectionsStore, connectionColorVar } from '$lib/stores/picus/connections.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { DIALECTS, type ConnectionState } from '$lib/types/picus';

  interface Props {
    connectionId: string;
    onClose: () => void;
  }

  let { connectionId, onClose }: Props = $props();

  /** The configured row — the same object the editor edits, read here. */
  const row = $derived(connectionsStore.specById(connectionId));
  /** The projected view, for the one thing only it carries: the version stamp. */
  const view = $derived(connectionsStore.byId(connectionId));

  const STATE_LABELS: Record<ConnectionState, string> = {
    connected: 'Connected',
    'read-only': 'Connected, read-only',
    connecting: 'Opening…',
    disconnected: 'Not connected',
  };

  const STATE_TONES: Record<ConnectionState, 'success' | 'warning' | 'info' | 'neutral'> = {
    connected: 'success',
    'read-only': 'warning',
    connecting: 'info',
    disconnected: 'neutral',
  };

  /** The address, spelled the way the engine spells it. */
  const address = $derived(
    row ? `${row.host}${row.port ? `:${row.port}` : ''}${row.database ? `/${row.database}` : ''}` : '',
  );

  const params = $derived(Object.entries(row?.params ?? {}));
</script>

<Modal {onClose} width="560px" ariaLabel="Connection details">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Database size={14} />
      <span class="cd-title">{row?.name ?? 'Connection'}</span>
      {#if row}<PicusDialectChip dialect={row.engine} />{/if}
    </ModalHeader>
  {/snippet}

  {#if !row}
    <p class="cd-gone">This connection no longer exists.</p>
  {:else}
    <div class="cd">
      <section>
        <h2>Identity</h2>
        <dl>
          <dt>Name</dt>
          <dd>
            <span class="cd-swatch" style:background={connectionColorVar(row)}></span>
            {row.name}
          </dd>
          <dt>Alias</dt>
          <dd>{row.alias || '—'}</dd>
          <dt>State</dt>
          <dd>
            <Badge variant="tone" tone={STATE_TONES[row.state]} label={STATE_LABELS[row.state]} />
          </dd>
          {#if row.serverVersion}
            <dt>Server</dt>
            <dd>{row.serverVersion}</dd>
          {/if}
          {#if view?.dbVersion}
            <dt>Application version</dt>
            <dd>{view.dbVersion}</dd>
          {/if}
        </dl>
      </section>

      <section>
        <h2>Server</h2>
        <dl>
          <dt>Engine</dt>
          <dd>{DIALECTS[row.engine].label}</dd>
          <dt>Address</dt>
          <dd><code>{address || '—'}</code></dd>
          <dt>Transport</dt>
          <dd>{row.tls ? 'TLS required' : 'Plaintext'}</dd>
        </dl>
      </section>

      <section>
        <h2>Session</h2>
        <dl>
          <dt>Schema</dt>
          <dd>{row.schema || '—'}</dd>
          <dt>Username</dt>
          <dd>{row.user || '—'}</dd>
          <dt>Writes</dt>
          <dd>
            {#if row.readOnly}
              <span class="cd-flag cd-warn"><Lock size={12} /> Refused by the backend</span>
            {:else}
              Allowed
            {/if}
          </dd>
          {#if params.length}
            <dt>Extra parameters</dt>
            <dd>
              <ul class="cd-params">
                {#each params as [key, value] (key)}
                  <li><code>{key}={value}</code></li>
                {/each}
              </ul>
            </dd>
          {/if}
        </dl>
      </section>

      <section>
        <h2>Credentials</h2>
        <dl>
          <dt>Password</dt>
          <dd>
            {#if row.hasSecret}
              <span class="cd-flag cd-ok"><ShieldCheck size={12} /> Stored in Arbor's keychain</span>
            {:else}
              <span class="cd-flag"><ShieldOff size={12} /> None stored</span>
            {/if}
          </dd>
        </dl>
        <p class="cd-note">
          Picus never holds the password itself — it asks the keychain for it at the moment a
          session opens, so it stays out of project files, configuration and logs.
        </p>
      </section>
    </div>
  {/if}

  {#snippet footer()}
    <Button
      variant="danger"
      size="sm"
      disabled={!row}
      tooltip={{ content: 'Delete this connection' }}
      onclick={() => row && picusUiStore.requestConnectionDelete(row.id)}
    >
      {#snippet iconStart()}<Trash2 size={13} />{/snippet}
      Delete…
    </Button>
    <span class="cd-spacer"></span>
    <Button variant="ghost" size="sm" onclick={onClose}>Close</Button>
    <Button
      variant="primary"
      size="sm"
      disabled={!row}
      onclick={() => row && picusUiStore.openConnectionEditor(row.id)}
    >
      {#snippet iconStart()}<Pencil size={13} />{/snippet}
      Edit…
    </Button>
  {/snippet}
</Modal>

<style>
  .cd-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }

  .cd { display: flex; flex-direction: column; gap: 16px; }

  .cd section { display: flex; flex-direction: column; gap: 8px; }
  .cd h2 {
    margin: 0;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  /* Label / value pairs on one grid so every value lines up down the dialog. */
  .cd dl {
    display: grid;
    grid-template-columns: 150px minmax(0, 1fr);
    gap: 6px 12px;
    margin: 0;
    font-size: var(--font-size-xs);
  }
  .cd dt { color: var(--text-muted); }
  .cd dd {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    min-width: 0;
    color: var(--text-primary);
    overflow-wrap: anywhere;
  }
  .cd code {
    font-family: var(--font-code);
    font-size: 11.5px;
    color: var(--text-secondary);
  }

  .cd-swatch {
    width: 9px;
    height: 9px;
    border-radius: 2px;
    flex-shrink: 0;
  }

  .cd-flag { display: inline-flex; align-items: center; gap: 5px; color: var(--text-secondary); }
  .cd-flag.cd-warn { color: var(--warning); }
  .cd-flag.cd-ok { color: var(--success); }

  .cd-params { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }

  .cd-note {
    margin: 0;
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-muted);
  }

  .cd-gone { font-size: var(--font-size-xs); color: var(--text-muted); }

  .cd-spacer { flex: 1; }
</style>
