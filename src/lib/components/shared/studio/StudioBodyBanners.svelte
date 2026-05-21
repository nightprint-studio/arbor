<!--
  StudioBodyBanners — the standard `saveError` + `actionError` banner pair
  every per-format Studio wrapper draws on top of its main view body.

  Drops into `<StudioModal>`'s `bodyBanners` snippet. Wrappers that have
  format-specific extra banners (JSON's streaming-mode notice,
  Properties' parse-error in-body banner) supply them via the `extras`
  snippet which renders AFTER the two standard banners.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import Alert from '../ui/Alert.svelte';

  interface Props {
    saveError:    string | null | undefined;
    actionError:  string | null | undefined;
    /** Extras rendered after the two standard banners (JSON streamMode
     *  notice, Properties parse-error inline alert). */
    extras?:      Snippet<[]>;
  }

  let { saveError, actionError, extras }: Props = $props();
</script>

{#if saveError}
  <div class="sbb-wrap"><Alert variant="error" compact text={`Save failed: ${saveError}`} /></div>
{/if}
{#if actionError}
  <div class="sbb-wrap"><Alert variant="error" compact text={actionError} /></div>
{/if}
{@render extras?.()}

<style>
  .sbb-wrap { padding: 6px 12px 0 12px; }
</style>
