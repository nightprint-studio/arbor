<!-- Merula docs — Libraries. Plain semantic HTML; DocsShell supplies the typography. -->
<script lang="ts">
  import MerulaCode from '../editor/MerulaCode.svelte';
</script>

<h1>Libraries</h1>
<p class="doc-lead">
  Reuse <code>.merula</code> modules across projects — your own helpers, or shared kits and
  instruments — pulled from GitHub and pinned for reproducible builds.
</p>

<h2>Declaring a library</h2>
<pre><code># merula.toml
[libraries]
drums = "github:owner/repo@v1"</code></pre>
<p>
  Each entry under <code>[libraries]</code> names a public GitHub source (a tag, branch or
  commit). Then import its <code>let</code> / <code>fn</code> declarations through the
  <code>$lib/</code> prefix:
</p>
<MerulaCode code={`import { kick, snare } from "$lib/drums/groove.merula"`} />

<h2>Syncing</h2>
<p>
  <strong>Sync libraries</strong> (Command Palette) downloads each one — pinned to a commit
  SHA — into a shared cache and records it in <code>merula.lock</code>, so the project builds the
  same way everywhere. Opening a project auto-fetches any declared library that's missing. Each
  sync is a background job in the <strong>Downloads &amp; Exports</strong> overlay.
</p>

<div class="callout">
  Local imports need none of this: <code>import &lbrace; … &rbrace; from "lib/drums.merula"</code> pulls
  from a file in your own project. The <code>$lib/</code> prefix is only for declared GitHub
  libraries.
</div>
