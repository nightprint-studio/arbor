<script lang="ts">
  /**
   * Bevy's own bird — the rail button for the Bevy tool window.
   *
   * It used to be lucide's `Box`, which said "a generic thing" on a rail where `Cog` already says
   * Cargo and a gear-with-a-check says tests: three grey outlines that had to be read rather than
   * recognised. A brand is the one case where the literal mark beats a metaphor, and it is the same
   * call `MavenIcon` makes for the same reason.
   *
   * ## Where this path comes from
   *
   * `assets/branding/bevy_bird_simpleicons.svg` in `bevyengine/bevy` — the flock of three, drawn as
   * **one path on a 24 grid with the gaps between the birds cut into the geometry**. That is the
   * form `simple-icons` asks for and exactly what a monochrome rail needs: it takes
   * `currentColor`, so the active state tints it along with every other icon, and it needs no
   * second colour to keep the three birds apart. Bevy is dual MIT / Apache-2.0, and the mark is
   * used here to refer to Bevy itself.
   *
   * Inlined rather than fetched or shipped as an asset, for two reasons: Arbor must work fully
   * offline — which is why `brand-icons.ts` imports every simple-icon as an object instead of
   * hitting the Iconify API — and Bevy has no entry in the `simple-icons` package that file draws
   * from. Several brands there are inlined verbatim for exactly that reason.
   *
   * Takes `size` (not `width` / `height`) so it drops into any icon slot typed for lucide, which is
   * what the activity rail's `icon` is. The mark is wider than it is tall, so at a given `size` it
   * fills the width and leaves room above and below: that is the logo's own proportion.
   */
  let { size = 18 }: { size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="currentColor"
  aria-hidden="true"
>
  <path
    d="
    M18.742 6.287 a3.14 3.14 0 0 0-.762.081 c-.018.004-.033.01-.05.013-.29.287-.614.574-.96.856
    a5.63 5.63 0 0 0 .57 1.53
    c.508.911.927 1.398 1.172 2.03.246.632.289 1.376.06 2.73-.282 1.68-1.304 3.459-2.925 4.772 1.258-.367 2.33-1.044 3.04-1.865.767-.885 1.023-1.386 1.112-1.89.09-.505.006-1.058.041-1.997
    a5.372 5.372 0 0 1 .316-1.599 l.04-.112.117-.015
    c1.049-.136 1.997-.404 2.487-.72-.158-.707-.826-1.257-1.712-1.674 l-.084-.04-.021-.09
    c-.281-1.206-1.223-1.984-2.441-2.01 Z m.61 1.49
    a.615.626 17.73 0 1 .184.028.615.626 17.73 0 1 .395.784.615.626 17.73 0 1-.776.41.615.626 17.73 0 1-.395-.784.615.626 17.73 0 1 .592-.438
    z m-5.676-4.598 c-.761-.005-1.542.3-2.213.95-.881.856-1.155 1.626-1 2.587 l.042.264-.263-.043
    c-1.936-.314-4.332-.155-7.058.947.802.344 2.272.917 3.904 1.7 l.518.25-.563.114
    c-2.155.436-4.712 1.698-7.048 4.452 1.045-.081 3.18-.333 5.408-.329 1.238.002 2.458.088 3.452.345.994.258 1.898.848 2.025 1.28.127.432.055.66-.07.998-.126.338-.511.851-1.045 1.19-.533.34-1.21.636-1.914.888-1.406.503-2.903.825-3.564.887-.14.013-.196.063-.25.161
    a.969.969 0 0 0-.089.415
    c-.006.215.028.328.089.413.06.086.156.12.254.127 4.153.28 7.258-.764 9.513-1.659 2.594-1.189 4.233-3.538 4.589-5.652.223-1.325.175-1.974-.04-2.527-.215-.553-.627-1.044-1.15-1.982
    a6.133 6.133 0 0 1-.64-1.745 l-.024-.116.092-.074
    c.961-.77 1.744-1.611 2.028-2.224-.583-.62-1.59-.751-2.718-.63 l-.092.01-.066-.067
    c-.6-.612-1.346-.926-2.107-.93 Z m.337 1.431
    a.715.715 0 0 1 .702.551.71.71 0 0 1-.516.863.711.711 0 0 1-.853-.532.711.711 0 0 1 .667-.882
    z m6.658 6.65 h-.006 a4.8 4.8 0 0 0-.24 1.3
    c-.034.908.058 1.461-.047 2.05-.104.588-.413 1.167-1.2 2.075-.935 1.081-2.418 1.918-4.132 2.2
    a8.883 8.883 0 0 1-1.009.557
    c1.333.502 2.728.53 3.833.166 1.948-.639 1.797-1.098 2.67-2.435.275-.421.592-.743.91-1.007.909.396 1.803.65 2.36.587.3-.694-.006-1.5-.516-2.262.418-1.3-.217-2.6-1.623-3.06
    a3.035 3.035 0 0 0-1-.171 Z m1.055 1.909
    a.531.541 48.68 0 1 .408.178.531.541 48.68 0 1-.056.756.531.541 48.68 0 1-.757-.042.531.541 48.68 0 1 .055-.756.531.541 48.68 0 1 .35-.136
    z m-8.715 5.783.05.028.05-.02 c-.034-.002-.067-.004-.1-.008 z
    "
  />
</svg>

<style>
  svg {
    display: block;
    flex-shrink: 0;
  }
</style>
