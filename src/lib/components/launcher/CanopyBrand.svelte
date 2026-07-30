<script lang="ts">
  /** The launcher's brand mark — a small sprouting-tree glyph + "Arbor". The
   *  wrapper is a window drag handle (frameless window). */
</script>

<div class="brand" data-tauri-drag-region>
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" style="color:#8fce6a;flex:none">
    <path d="M12 21 L12 12" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
    <path d="M12 14 C12 10 9 9 7 9.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
    <path d="M12 13 C12 8.5 15 7.5 17 8.2" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
    <circle cx="7" cy="9.5" r="1.6" fill="#45c4b0" />
    <circle cx="17" cy="8.2" r="1.6" fill="#b58cf0" />
  </svg>
  <span class="brand-name">Arbor</span>
</div>

<style>
  .brand { display: flex; align-items: center; gap: 8px; flex: none; }
  .brand-name { font-family: var(--canopy-display); font-weight: 600; font-size: var(--font-size-lg); color: #dfe5ee; }
</style>
