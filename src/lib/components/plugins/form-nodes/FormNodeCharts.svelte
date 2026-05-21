<!--
  FormNodeCharts — analytic widgets that wrap a shared chart primitive:
    counter_grid, score_gauge, time_series_chart, data_table, filter_bar.
-->
<script lang="ts">
  import CounterGrid from '$lib/components/shared/charts/CounterGrid.svelte';
  import GaugeChart  from '$lib/components/shared/charts/GaugeChart.svelte';
  import LineChart   from '$lib/components/shared/charts/LineChart.svelte';
  import DataTable   from '$lib/components/shared/charts/DataTable.svelte';
  import FilterBar   from '$lib/components/shared/charts/FilterBar.svelte';

  import type { FormNode } from '$lib/types/plugin';
  import type { FormNodeCtx } from './ctx';

  interface Props {
    node: FormNode;
    ctx:  FormNodeCtx;
  }
  let { node, ctx }: Props = $props();
</script>

{#if node.type === 'counter_grid'}
  {@const cg = node as any}
  <div class={(node as any).class ?? ''} style={(node as any).style}>
    <CounterGrid
      items={ctx.toArr(cg.items)}
      minWidth={cg.min_width}
      gap={cg.gap}
      padding={cg.padding}
      onSelect={cg.actions?.select
        ? (key) => ctx.handleButtonAction(cg.actions.select, false, { key })
        : undefined}
    />
  </div>

{:else if node.type === 'score_gauge'}
  {@const sg = node as any}
  <div class={(node as any).class ?? ''} style={(node as any).style}>
    <GaugeChart
      value={sg.value}
      min={sg.min}
      max={sg.max}
      segments={ctx.toArr(sg.segments)}
      label={sg.label}
      size={sg.size ?? 'md'}
      valueColor={sg.value_color}
    />
  </div>

{:else if node.type === 'time_series_chart'}
  {@const ts = node as any}
  {@const series = ctx.toArr<any>(ts.series).map((s: any) => ({
    id:    s.id,
    label: s.label,
    color: s.color,
    points: ctx.toArr<any>(s.points).map((p: any) => ({
      x: ts.x_kind === 'linear' ? Number(p.x) : new Date(p.x),
      y: Number(p.y),
    })),
  }))}
  <div class={(node as any).class ?? ''} style={(node as any).style}>
    <LineChart
      series={series}
      xAxis={{ kind: ts.x_kind ?? 'time' }}
      yAxis={{ includeZero: ts.y_include_zero ?? true }}
      height={ts.height ?? 220}
      showLegend={ts.show_legend ?? true}
    />
  </div>

{:else if node.type === 'data_table'}
  {@const dt = node as any}
  <div class={(node as any).class ?? ''} style={(node as any).style}>
    <DataTable
      columns={ctx.toArr(dt.columns) as any}
      rows={ctx.toArr(dt.rows) as any}
      rowKey={dt.row_key ?? 'id'}
      height={dt.height}
      initialSort={dt.initial_sort}
      empty={dt.empty}
      onRowClick={dt.actions?.row_click
        ? (row) => ctx.handleButtonAction(dt.actions.row_click, false, {
            row_id: row[dt.row_key ?? 'id'],
            row,
          })
        : undefined}
    />
  </div>

{:else if node.type === 'filter_bar'}
  {@const fb = node as any}
  {@const fbName = fb.name as (string | undefined)}
  {@const fbKey = fbName ?? `_anon:${node.id ?? ''}`}
  {@const fbValue = fbName
    ? (ctx.values[fbName] ?? { search: '', filters: {} })
    : (ctx.filterBarState[fbKey] ??= {
        search:  fb.default?.search ?? '',
        filters: { ...(fb.default?.filters ?? {}) },
      })}
  <div class={(node as any).class ?? ''} style={(node as any).style}>
    <FilterBar
      value={fbValue}
      filters={ctx.toArr(fb.filters)}
      search={fb.search === null ? null : (fb.search ?? { placeholder: 'Search…' })}
      padding={fb.padding ?? '8px'}
      onChange={(v) => {
        if (fbName) {
          ctx.values[fbName] = v;
          ctx.notifyChange(fbName, v);
        } else {
          ctx.filterBarState[fbKey] = v;
        }
        if (fb.actions?.change) {
          ctx.handleButtonAction(fb.actions.change, false, { value: v });
        }
      }}
    />
  </div>
{/if}
