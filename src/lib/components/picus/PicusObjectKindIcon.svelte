<script lang="ts" module>
  import { Table2, Eye, Hash, Package, Cog, FunctionSquare, Zap } from 'lucide-svelte';
  import type { IconComponent } from '$lib/types/icon';
  import type { ObjectKind } from '$lib/types/picus';

  /**
   * One icon per kind of database object.
   *
   * In one place because the alternative is what this replaced: a two-branch
   * `{#if kind === 'table'} … {:else} …` that drew a table for tables and a
   * package for *everything else* — so views, sequences and triggers all wore the
   * same wrong badge, in a view whose entire purpose is telling objects apart.
   */
  export const OBJECT_KIND_ICONS: Record<ObjectKind, IconComponent> = {
    table: Table2 as unknown as IconComponent,
    view: Eye as unknown as IconComponent,
    sequence: Hash as unknown as IconComponent,
    package: Package as unknown as IconComponent,
    procedure: Cog as unknown as IconComponent,
    function: FunctionSquare as unknown as IconComponent,
    trigger: Zap as unknown as IconComponent,
  };

  /** How each kind is written when it is a heading rather than a badge. */
  export const OBJECT_KIND_LABELS: Record<ObjectKind, string> = {
    table: 'Tables',
    view: 'Views',
    sequence: 'Sequences',
    package: 'Packages',
    procedure: 'Procedures',
    function: 'Functions',
    trigger: 'Triggers',
  };
</script>

<script lang="ts">
  let { kind, size = 13 }: { kind: ObjectKind; size?: number } = $props();

  const Icon = $derived(OBJECT_KIND_ICONS[kind] ?? OBJECT_KIND_ICONS.table);
</script>

<Icon {size} />
