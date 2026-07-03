<script lang="ts">
  /** Structure (left tool window) — the active file's symbol tree (Java hierarchy
   *  or XML/JSP element tree, picked by extension). Thin wrapper over the shared
   *  BennuSymbolList so Structure and Outline stay identical. */
  import BennuSymbolList from './BennuSymbolList.svelte';
</script>

<BennuSymbolList title="Structure" />
