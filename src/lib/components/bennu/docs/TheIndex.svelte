<script lang="ts">
  /**
   * The index: what answers from it, the inspector, what the scan walks, and how it keeps up with edits.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Reference</span>
<h1>The index</h1>

<p class="doc-lead">
  Nearly everything else in this manual is a question asked of the index. This is what it is, when it exists, and what an empty answer means while it is still building.
</p>

<h2>What it is</h2>
<p>
  Completion, go-to declaration, find usages, rename, hover and go to class all answer from a <strong>semantic index</strong> Bennu builds in the background when a Java
  project opens. The footer shows its progress, and reads <em>Indexed · N types</em> once it is warm.
</p>
<Callout variant="info" title="An empty answer while it builds">
  Means "not yet", not "nothing there". The footer says which state the index is in.
</Callout>

<h2>The index inspector</h2>
<p>
  <em>Index inspector…</em> in the command palette browses what the index holds — types, members, jars, the JDK, beans, actions and relations — with a filter and jump-to.
</p>
<dl class="meta-grid">
  <dt>Type names</dt>
  <dd>How many distinct class names completion can offer, counting the JDK's, every resolved jar's and your own. It tells <em>completion is not offering my library classes</em> from <em>the library classes were never loaded</em> — the same from the popup. A few thousand means the JDK alone answered; a project with jars runs to tens of thousands.</dd>
  <dt>Rebuild</dt>
  <dd>When something looks stale or a class you know exists does not turn up: <strong>Rebuild</strong> there, or <em>Rebuild index</em> in the palette, invalidates the index and recomputes it. It re-scans the sources on disk — it does not compile; that is <kbd>Ctrl</kbd> + <kbd>F9</kbd>.</dd>
</dl>

<h2>What it walks</h2>
<p>
  Every <code>.java</code> under the project root except what nothing should index: hidden directories, and Maven's <code>target/</code> — apart from
  <code>target/generated-sources</code>, where an annotation processor writes source that is yours to navigate.
</p>
<p>
  <strong>Settings → Java → Excluded directories</strong> adds to that list. Entries are folder <em>names</em> matched at any depth, so <code>build</code> excludes every
  <code>build</code> folder in a multi-module tree. A change applies to the next build of the index, which <em>Rebuild index</em> gives you at once.
</p>

<h2>It keeps up with your edits</h2>
<p>
  The index is not a snapshot of the project as opened. As you type, the file you edit is read into it again — so a method just written has its usages, and one you just
  stopped calling loses them. Nothing has to be saved, rebuilt or reopened for find usages, rename and hover to agree with go-to about what the code says.
</p>
<ul>
  <li>When an edit changes what a file <em>declares</em> — a method added, renamed or removed, a signature or supertype changed — the files resolving against it are read
    again too. Typing inside a method body changes nothing anyone else resolves, so it costs one file.</li>
  <li><strong>Who extends whom</strong> keeps up the same way. A method rename carries its <em>override family</em>, so a class that has just started implementing an
    interface must be known as one — or the rename would move the interface's method and strand the class on the old name. Changing an <code>extends</code> or
    <code>implements</code> clause files the type again immediately.</li>
</ul>
<Callout variant="tip" title="Everything is on the keyboard">
  The <strong>command palette</strong> (<kbd>Ctrl</kbd> + <kbd>K</kbd>) lists the editor and tool-window actions; <strong>Keyboard shortcuts</strong> has every binding.
  In a Cargo project the Java-only tools are hidden, and <kbd>Ctrl</kbd> + <kbd>F9</kbd> runs <code>cargo check</code>.
</Callout>
