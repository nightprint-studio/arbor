<script lang="ts">
  /**
   * The Canopy's backdrop — a fixed starry night, no
   * `backdrop-filter` (WebView2 compositor). `starShadow` is the precomputed
   * `box-shadow` starfield; `arbTwinkle` keyframes live globally in
   * `LauncherShell`.
   */
  interface Props { starShadow: string; }
  let { starShadow }: Props = $props();
</script>

<div class="atm">
  <div class="layer" style="background:radial-gradient(46% 38% at 10% 6%, rgba(143,206,106,.16), transparent 70%)"></div>
  <div class="layer" style="background:radial-gradient(44% 36% at 92% 8%, rgba(181,140,240,.16), transparent 70%)"></div>
  <div class="layer" style="background:radial-gradient(60% 50% at 82% 104%, rgba(69,196,176,.10), transparent 70%)"></div>
  <div class="stars" style="box-shadow:{starShadow}"></div>
</div>

<style>
  .atm {
    position: absolute; inset: 0;
    background: radial-gradient(130% 100% at 50% -5%, #111a2c 0%, #0a0f1c 46%, #04060c 100%);
  }
  .layer { position: absolute; inset: 0; pointer-events: none; }
  .stars {
    position: absolute; top: 0; left: 0; width: 1px; height: 1px;
    border-radius: 50%; animation: arbTwinkle 7s ease-in-out infinite;
  }
</style>
