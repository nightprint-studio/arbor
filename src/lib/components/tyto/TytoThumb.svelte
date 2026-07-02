<script lang="ts" module>
  // Per-instance gradient id so multiple thumbnails don't collide.
  let _uid = 0;
</script>

<script lang="ts">
  /**
   * TytoThumb — a stylized, scalable stand-in "preview" for a capture (mock).
   * Draws a little app window on a hue-tinted wallpaper: reads like a screenshot
   * without a real frame (which needs the capture backend). Fills its container;
   * pair with a fixed-size wrapper.
   */
  let { hue = 210, kind = 'record' }: { hue?: number; kind?: 'record' | 'screenshot' } = $props();

  const gid = `tyto-thumb-${_uid++}`;
  const c1 = $derived(`hsl(${hue} 58% 48%)`);
  const c2 = $derived(`hsl(${(hue + 45) % 360} 52% 30%)`);
</script>

<svg class="thumb" viewBox="0 0 100 62" preserveAspectRatio="xMidYMid slice" aria-hidden="true">
  <defs>
    <linearGradient id={gid} x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color={c1} />
      <stop offset="1" stop-color={c2} />
    </linearGradient>
  </defs>

  <!-- wallpaper -->
  <rect width="100" height="62" fill="url(#{gid})" />
  <ellipse cx="28" cy="-6" rx="72" ry="42" fill="#ffffff" opacity="0.12" />

  <!-- window -->
  <rect x="15" y="13" width="70" height="40" rx="4.5" fill="#0c1119" opacity="0.86" />
  <rect x="15" y="13" width="70" height="9.5" rx="4.5" fill="#1b2740" />
  <rect x="15" y="19" width="70" height="3.5" fill="#1b2740" />
  <circle cx="21" cy="18" r="1.5" fill="#ff5f57" />
  <circle cx="26.5" cy="18" r="1.5" fill="#febc2e" />
  <circle cx="32" cy="18" r="1.5" fill="#28c840" />

  <!-- content -->
  <rect x="20" y="28" width="22" height="4.5" rx="2.2" fill={c1} opacity="0.9" />
  <rect x="20" y="36" width="42" height="3" rx="1.5" fill="#2b3a56" />
  <rect x="20" y="42" width="34" height="3" rx="1.5" fill="#243250" />
  <rect x="20" y="48" width="26" height="3" rx="1.5" fill="#243250" />
  <rect x="66" y="28" width="14" height="17" rx="2.5" fill="#243652" />

  {#if kind === 'record'}
    <circle cx="80" cy="18" r="2.1" fill="#ff4d4d" />
  {/if}
</svg>

<style>
  .thumb { display: block; width: 100%; height: 100%; }
</style>
