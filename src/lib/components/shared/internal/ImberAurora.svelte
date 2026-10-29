<script lang="ts">
  /**
   * ImberAurora — the slow aurora behind Imber's surfaces: three soft colour fields drifting.
   *
   * Fills its positioned parent (`position: absolute; inset: 0`), sits behind the content and never takes
   * the pointer. Gradients and transforms only — no blur filter, no `backdrop-filter`, which would keep the
   * WebView2 compositor busy for as long as the surface is open. Durations come from `--anim-dur-slow`, so
   * the app's animation setting (and "off", which is 0ms) applies.
   */
  let {
    hue = 'var(--accent)',
    scale = 1,
    intensity = 0.5,
  }: {
    /** Colour of the first field; the other two are the info and success accents. */
    hue?: string;
    /** Size multiplier of the fields, for a surface larger than a side panel. */
    scale?: number;
    /** Opacity of the fields, 0..1. */
    intensity?: number;
  } = $props();
</script>

<div class="aurora" aria-hidden="true" style="--aur-hue: {hue}; --sc: {scale}; --int: {intensity}">
  <i class="b1"></i><i class="b2"></i><i class="b3"></i>
</div>

<style>
  .aurora { position: absolute; inset: 0; z-index: -1; overflow: hidden; pointer-events: none; }
  i { position: absolute; border-radius: 50%; opacity: var(--int); will-change: transform; }
  .b1 { width: calc(260px * var(--sc)); height: calc(260px * var(--sc)); top: calc(-90px * var(--sc)); left: calc(-70px * var(--sc));
        background: radial-gradient(circle, color-mix(in srgb, var(--aur-hue) 38%, transparent), transparent 68%);
        animation: drift-a calc(var(--anim-dur-slow, 240ms) * 60) ease-in-out infinite alternate; }
  .b2 { width: calc(300px * var(--sc)); height: calc(300px * var(--sc)); top: calc(30px * var(--sc)); right: calc(-120px * var(--sc));
        background: radial-gradient(circle, color-mix(in srgb, var(--info) 30%, transparent), transparent 68%);
        animation: drift-b calc(var(--anim-dur-slow, 240ms) * 75) ease-in-out infinite alternate; }
  .b3 { width: calc(240px * var(--sc)); height: calc(240px * var(--sc)); bottom: calc(-110px * var(--sc)); left: 20%;
        background: radial-gradient(circle, color-mix(in srgb, var(--success) 20%, transparent), transparent 68%);
        animation: drift-a calc(var(--anim-dur-slow, 240ms) * 90) ease-in-out infinite alternate-reverse; }

  @keyframes drift-a {
    from { transform: translate3d(0, 0, 0) scale(1); }
    to   { transform: translate3d(46px, 38px, 0) scale(1.18); }
  }
  @keyframes drift-b {
    from { transform: translate3d(0, 0, 0) scale(1.05); }
    to   { transform: translate3d(-52px, 30px, 0) scale(0.9); }
  }

  @media (prefers-reduced-motion: reduce) {
    i { animation: none; }
  }
</style>
