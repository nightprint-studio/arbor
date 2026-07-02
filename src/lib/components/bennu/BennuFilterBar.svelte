<script lang="ts">
  /**
   * BennuFilterBar — the filter <SearchBar> row a left tool window puts below its
   * header. Wraps the shared SearchBar with the one consistent padding so Project,
   * Structure and Dependencies share an identical filter rhythm (single fix
   * surface). Intended to be rendered in PanelShell's `toolbar` snippet, which
   * supplies the divider below it.
   *
   * Bennu-local (leans on the Bennu panels' shared look), so it lives in
   * components/bennu/, not shared/internal/.
   */
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';

  let {
    query = $bindable(''),
    placeholder = 'Filter…',
  }: { query?: string; placeholder?: string } = $props();
</script>

<div class="filter-bar">
  <SearchBar bind:query {placeholder} showRegex={false} showCounter={false} />
</div>

<style>
  .filter-bar { padding: 6px 8px; }
</style>
