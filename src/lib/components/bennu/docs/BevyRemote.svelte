<script lang="ts">
  /**
   * Bevy Remote: the live inspector over the Bevy Remote Protocol — connecting, the entity hierarchy, the
   * component editors, resources and the type registry. Keep in step with `bennu-be`'s `brp` module and the
   * `arbor-brp` crate (method-name dialects, snapshot, registry shapes).
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Rust</span>
<h1>Bevy Remote</h1>

<p class="doc-lead">
  Look at a running Bevy game from the editor: its entities and how they nest, every component with its current value, its resources, and the types it registered —
  and change them while it plays.
</p>

<h2>Connecting</h2>
<p>
  The game has to serve the Bevy Remote Protocol. Add both plugins to its <code>App</code>:
</p>
<pre><code>app.add_plugins((RemotePlugin::default(), RemoteHttpPlugin::default()));</code></pre>
<p>
  Then open the <strong>Bevy Remote</strong> panel — <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>A</kbd>, the radar button in the right activity bar, or the command palette — and press
  <kbd>Enter</kbd> on the endpoint (<code>http://127.0.0.1:15702</code> unless the game changed it). It is offered on projects that depend on Bevy.
</p>
<p>
  Bennu asks the game what it can do rather than guessing from a Bevy version: the verbs it offers are the ones the game lists, so an older game that cannot spawn or
  watch simply does not show those controls.
</p>
<Callout variant="warning" title="Edits reach a running game">
  Mutating, spawning and despawning act on the game at the endpoint. A game on another machine is flagged in the panel's header.
</Callout>

<h2>When the game restarts</h2>
<p>
  Rebuilding and running the game drops the connection. Bennu notices, retries quietly in the background and picks the new process up by itself, re-reading its
  types: the old registry described a binary that no longer exists. A banner shows the retries; a notification appears only if the game stays away after the first retry,
  once when it is lost and once when it is back, or once more if Bennu eventually gives up.
</p>

<h2>Entities</h2>
<p>
  The left list is the game's hierarchy. A node opens one level at a time, so a parent with thousands of children costs one page. The world is re-read only while the
  panel is open, once a second while it is changing and progressively less often while it is quiet.
</p>
<p>The search box takes plain text — a name, or digits for an id — and two filters, which can be combined:</p>
<table>
  <thead><tr><th>Write</th><th>Shows</th></tr></thead>
  <tbody>
    <tr><td><code>goblin</code></td><td>Entities whose <code>Name</code> contains it</td></tr>
    <tr><td><code>has:Health</code></td><td>Entities with that component (short name or full path)</td></tr>
    <tr><td><code>no:Dead</code></td><td>Entities without it</td></tr>
  </tbody>
</table>
<p>
  Results are a flat list; <strong>Reveal in hierarchy</strong>, in the right-click menu, takes one back to its place. <kbd>Delete</kbd> on a row despawns it, after a
  confirmation. Ids are written the way Bevy prints them: <code>3v1</code> is slot 3, generation 1.
</p>

<h2>Components</h2>
<p>
  Select an entity and the right pane shows one card per component. The values are pushed by the game rather than polled — the <strong>Live</strong> badge says the
  stream is on — so a field that moves in the game moves here.
</p>
<ul>
  <li>Numbers, booleans, text, enums and nested structs have their own controls; short number lists such as positions are on one line, labelled by axis.</li>
  <li><strong>Edit as JSON</strong>, on each card, edits the value exactly as it is sent. It is always right, where a typed control can only be as good as what the game's type registry says.</li>
  <li><strong>Add component</strong> searches the registered types and starts from a well-formed placeholder value; <kbd>Ctrl</kbd> + <kbd>Enter</kbd> inserts it.</li>
  <li>A component the game could not serialise is shown with the game's own message instead of a value.</li>
</ul>

<h2>Resources and types</h2>
<p>
  <strong>Resources</strong> lists the game's singleton data and edits it with the same cards. <strong>Types</strong> is the registry itself: every component and resource the game
  registered, with its exact path — the string the protocol wants — one click from the clipboard, and a shortcut to add the type.
</p>
<Callout variant="info" title="What the editors can and cannot know">
  The game sends each value through the type's own serialisation, which sometimes differs from the type's description: a position is described as a struct of
  <code>x</code>, <code>y</code>, <code>z</code> but arrives as <code>[x, y, z]</code>. The editors follow the value and use the description only to refine it, so an unusual type
  can fall back to the JSON editor, never to a wrong control.
</Callout>

<h2>Limits</h2>
<ul>
  <li>The protocol cannot filter or page on the game's side, so each refresh reads every entity's name and parent. A world of hundreds of thousands of entities is read
    less often, not more cheaply.</li>
  <li>Entity ids are exact up to 2<sup>53</sup>; a slot recycled millions of times is shown by its raw digits.</li>
</ul>
