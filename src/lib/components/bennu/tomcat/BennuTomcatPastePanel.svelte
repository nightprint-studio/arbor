<script lang="ts">
  /**
   * Bring in a context that already exists somewhere else.
   *
   * The usual case is a colleague's `context.xml`, the `<Resource>` copied out of a production
   * `server.xml`, or the `META-INF/context.xml` already in the project. Whatever names a data
   * source, an environment entry or a parameter becomes a resource of this form; the rest of the
   * text is ignored. Passwords found in it go to the secret store rather than into the
   * configuration, so pasting a context that carried one does not put it in a shared file.
   */
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import { parseContextXml, type ContextImport } from './tomcat-context-xml';

  let {
    onimport,
    onclose,
  }: {
    onimport: (result: ContextImport) => void;
    onclose: () => void;
  } = $props();

  let text = $state('');
  let error = $state('');

  function run() {
    const result = parseContextXml(text);
    if (result.error) {
      error = result.error;
      return;
    }
    onimport(result);
  }
</script>

<div class="head">
  <strong>Paste XML</strong>
  <span class="sp"></span>
  <Button variant="primary" size="sm" disabled={!text.trim()} onclick={run}>Import</Button>
  <Button variant="secondary" size="sm" onclick={onclose}>Cancel</Button>
</div>
<p class="lead">
  A <code>context.xml</code>, a <code>&lt;Resource&gt;</code> from <code>server.xml</code>, or a
  <code>GlobalNamingResources</code> block. Passwords are moved out of the text into the secret store.
</p>
<Textarea
  bind:value={text}
  framed
  minRows={12}
  maxRows={18}
  ariaLabel="Context XML to import"
  placeholder={'<Resource name="jdbc/Gestionale" type="javax.sql.DataSource" url="jdbc:…" …/>'}
/>
{#if error}<Alert variant="error" compact text={error} />{/if}

<style>
  .head { display: flex; align-items: center; gap: 8px; margin-bottom: 6px; }
  .sp { flex: 1; }
  .lead { margin: 0 0 10px; color: var(--text-muted); font-size: var(--font-size-xs); line-height: 1.5; }
  code { font-family: var(--font-code); }
</style>
