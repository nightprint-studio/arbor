<!--
  Generic contribution primitive.

  Renders one slot per contribution item registered at `point`, applying:
    • disabled-plugin filter (items from disabled plugins are skipped)
    • `c.disabled === true` filter (Phase 5 — top-level field; consumers
      that want to render disabled items as greyed-out should drop the
      <Contribution> primitive and read `forPoint()` directly)
    • optional prop `filter` for additional caller-side filtering
    • optional `whenContext` — items whose top-level `when` clause does not
      match the context are skipped (tree-kind sidebars use this per node)

  Each item is wrapped in a <svelte:boundary> so a throwing plugin snippet
  degrades to nothing without taking down sibling items or the host UI.

  The `item` snippet receives:
    { payload, plugin, itemId, fire }

  where `fire(extra?)` calls firePluginAction(plugin, payload.action, {
    ...payload.payload,   ← optional nested context object in the contribution
    ...extra,             ← caller-supplied runtime context (e.g. { oid, node_id })
  }).

  Usage:
    <Contribution point="arbor:diff-toolbar">
      {#snippet item({ payload, fire })}
        {@const p = payload as { icon: string; action: string; tooltip?: string }}
        <button title={p.tooltip} onclick={() => fire()}>
          <PluginIcon name={p.icon} size={14} />
        </button>
      {/snippet}
    </Contribution>

    <Contribution point="{ns}:node_action" whenContext={node}>
      {#snippet item({ payload, fire })}
        {@const p = payload as { icon?: string; tooltip?: string }}
        <button onclick={() => fire({ node_id: node.id, data: node.data })}>
          {#if p.icon}<PluginIcon name={p.icon} size={12} />{/if}
        </button>
      {/snippet}
    </Contribution>
-->
<script lang="ts" module>
  import { contributionStore as _store } from '$lib/stores/contribution.svelte';
  import { pluginStore       as _plugins } from '$lib/stores/plugin.svelte';
  import { whenMatches       as _whenMatches } from '$lib/contributions/when';
  import type { PluginContribution as _Contribution } from '$lib/types/contribution';

  /**
   * Bucket a point's contributions by their top-level `group` field. Items
   * without a group fall into the `defaultGroup` bucket (or are dropped if
   * no `defaultGroup` is supplied). Disabled-plugin and `c.disabled` filters
   * are applied here too so callers don't have to repeat them.
   *
   * Returns a `Map` keyed by group label (insertion-ordered as encountered),
   * with the contributions inside each bucket preserved in priority order.
   *
   * Reserved for Phase 6 callers (KeybindingsSection, CommandPalette) that
   * want grouped layouts; the per-item `<Contribution>` render path doesn't
   * need it.
   */
  export function groups(
    point: string,
    opts: { defaultGroup?: string } = {},
  ): Map<string, _Contribution[]> {
    const out = new Map<string, _Contribution[]>();
    const list = _store.forPoint(point)
      .filter(c => !_plugins.disabledPlugins.has(c.plugin_name))
      .filter(c => !c.disabled);
    for (const c of list) {
      const g = c.group ?? opts.defaultGroup;
      if (g === undefined) continue;
      const bucket = out.get(g) ?? [];
      bucket.push(c);
      out.set(g, bucket);
    }
    return out;
  }
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import { pluginStore }       from '$lib/stores/plugin.svelte';
  import { firePluginAction }  from '$lib/ipc/plugin';
  import { whenMatches }       from '$lib/contributions/when';
  import type { PluginContribution } from '$lib/types/contribution';

  interface Props {
    point:        string;
    whenContext?: unknown;
    filter?:      (c: PluginContribution) => boolean;
    item:         Snippet<[{
      payload: unknown;
      plugin:  string;
      itemId:  string;
      fire:    (extra?: Record<string, unknown>) => void;
    }]>;
  }

  let { point, whenContext, filter, item: itemSnippet }: Props = $props();

  const items = $derived.by(() => {
    let list = contributionStore.forPoint(point);
    list = list.filter(c => !pluginStore.disabledPlugins.has(c.plugin_name));
    list = list.filter(c => !c.disabled);
    if (filter) list = list.filter(filter);
    if (whenContext !== undefined) {
      list = list.filter(c => whenMatches(c.when, whenContext));
    }
    return list;
  });
</script>

{#each items as c (`${c.plugin_name}::${c.item_id}`)}
  <svelte:boundary
    onerror={(err) => {
      // eslint-disable-next-line no-console
      console.warn(
        `[plugin:${c.plugin_name}] contribution '${point}' threw while rendering`,
        { itemId: c.item_id, err },
      );
    }}
  >
    {@render itemSnippet({
      payload: c.payload,
      plugin:  c.plugin_name,
      itemId:  c.item_id,
      fire: (extra) => {
        const p = c.payload as Record<string, unknown>;
        firePluginAction(
          c.plugin_name,
          (p.action as string) ?? '',
          JSON.stringify({
            ...((p.payload as Record<string, unknown>) ?? {}),
            ...(extra ?? {}),
          }),
        ).catch(() => {});
      },
    })}
    {#snippet failed()}{/snippet}
  </svelte:boundary>
{/each}
