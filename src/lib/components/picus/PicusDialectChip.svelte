<script lang="ts">
  /**
   * The dialect badge — Oracle / PostgreSQL — in one place.
   *
   * The dialect is a property of the folder, so it is shown constantly: on tree
   * branches, tabs, generation targets, findings. Centralising it means the
   * colour and the wording are decided once; the colours themselves come from
   * the theme's workspace ramp, never from a hex literal.
   */
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { DIALECTS, type Dialect } from '$lib/types/picus';

  interface Props {
    dialect: Dialect;
    size?: 'sm' | 'md';
    /** Use the short code (`ORA` / `PG`) — for very tight rows. */
    terse?: boolean;
  }

  let { dialect, size = 'sm', terse = false }: Props = $props();

  const info = $derived(DIALECTS[dialect]);
  const label = $derived(terse ? (dialect === 'oracle' ? 'ORA' : 'PG') : info.short);
</script>

<span use:tooltip={info.label}>
  <Badge variant="chip" {size} color={`var(${info.colorVar})`} label={label} />
</span>
