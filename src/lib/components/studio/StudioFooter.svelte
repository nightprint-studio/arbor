<!--
  StudioFooter is split across three sibling components so each can plug
  into the matching <StudioModal> slot:

    · <StudioFooterStatus>  → `footerStatusLeft`
    · <StudioFooterCenter>  → `footerCenter`
    · <StudioFooterRight>   → `footerRight`

  This shim re-exports them so callers can `import { StudioFooterStatus,
  StudioFooterCenter, StudioFooterRight } from './StudioFooter'`. The
  shared `StudioFooterDoc` / `IndentChoice` types live next to them.

  All the dumb-button / pill markup lives in the three child files; this
  file just re-exports to keep import sites tidy.
-->
<script module lang="ts">
  export type { StudioFooterDoc, IndentChoice } from './studio-footer-types';
  export { DEFAULT_INDENT_OPTIONS } from './studio-footer-types';
</script>

<script lang="ts">
  /* This wrapper component is intentionally a no-op — consumers import
     the three slot components by name. Keeping a file under the
     historical name avoids breaking grep / IDE history. */
</script>
