<script lang="ts">
  /**
   * Completion, the overview: asking for it, and a map of the pages that follow.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Editor</span>
<h1>Completion</h1>

<p class="doc-lead">
  What the popup knows, where each suggestion comes from, and the members that exist without being written anywhere.
</p>

<h2>Asking for it</h2>
<p>
  Typing <code>.</code> after an expression offers the members. <kbd>Ctrl</kbd> + <kbd>Space</kbd> — or <kbd>Ctrl</kbd> + <kbd>Shift</kbd> +
  <kbd>Space</kbd> — asks explicitly, anywhere. In Java the answers come from the project index and appear once it is warm; edits are indexed
  again in the background as you type, so completion and go-to follow your changes without reopening anything.
</p>
<Callout variant="warning" title="On macOS the chord is Cmd + Shift + Space">
  Not a style choice. macOS claims the whole <kbd>Ctrl</kbd> + <kbd>Space</kbd> family for switching input source, <em>above</em>
  applications: those chords produce no key event in any program. <kbd>Cmd</kbd> + <kbd>Space</kbd> is Spotlight, which is what the
  <kbd>Shift</kbd> is for.
</Callout>
<ul>
  <li>Typing fast does not send a question per key. Within a burst the list is asked for when you pause, and at least every few hundred
    milliseconds while the burst lasts; the first letter, a <code>.</code> and <kbd>Ctrl</kbd> + <kbd>Space</kbd> are answered at once.</li>
  <li>When an explicit request finds nothing, the footer says so for a moment — <em>No suggestions here</em>, or <em>No engine answered for
    this file</em> when no server or index is up. Silence would look like a shortcut that never arrived, and those two have opposite fixes.</li>
  <li>If a shortcut seems to do nothing at all, turn on <strong>Show keyboard inputs</strong> (<kbd>Alt</kbd> + <kbd>Shift</kbd> +
    <kbd>K</kbd>) and press it again: the overlay draws every chord the window receives. Nothing drawn means the key never arrived — a
    conflict outside Bennu.</li>
  <li><strong>Command languages ask on every position.</strong> A <code>.dev</code> scenario or a <code>.dig</code> line is words and
    arguments rather than dotted paths, so with the caret after <code>unlock&nbsp;</code> the server — <strong>nd-dig-lsp</strong>, geode's
    own — is asked too, not only after a trigger character.</li>
</ul>

<h2>The pages</h2>
<dl class="meta-grid">
  <dt><a href="#completion-order">The order things are offered in</a></dt>
  <dd>Eight signals, strongest first: what the position wants, who declares it, what you picked, and the rest.</dd>
  <dt><a href="#completion-smart">Smart suggestions and ghost text</a></dt>
  <dd>The learned and project-aware signals, finishing the line in grey, type names and where the import counts come from.</dd>
  <dt><a href="#completion-writing">Matching and writing</a></dt>
  <dd>How a name is matched, a name with nothing to its left, what accepting writes, and how to read a row.</dd>
  <dt><a href="#completion-templates">Annotations, abbreviations and postfix</a></dt>
  <dd>Names that expand into code.</dd>
  <dt><a href="#completion-positions">Positions with their own rules</a></dt>
  <dd>After a dot, method references, arguments, enums, keywords and a declaration's name.</dd>
  <dt><a href="#completion-missing">Members that do not exist yet</a></dt>
  <dd>The method this class calls and does not declare, missing accessors, and what Lombok and records generate.</dd>
</dl>
