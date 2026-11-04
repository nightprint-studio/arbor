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
  Then open the <strong>Bevy Remote</strong> sidebar — <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>A</kbd>, the radar button in the right activity bar, or the command palette — and press
  <kbd>Enter</kbd> on the endpoint (<code>http://127.0.0.1:15702</code> unless the game changed it). It is offered on projects that depend on Bevy.
</p>
<p>
  Bennu asks the game what it can do rather than guessing from a Bevy version: the verbs it offers are the ones the game lists, so an older game that cannot spawn or
  watch simply does not show those controls.
</p>
<Callout variant="warning" title="Edits reach a running game">
  Mutating, spawning and despawning act on the game at the endpoint. A game on another machine is flagged in the panel's header.
</Callout>

<h3>Saved games</h3>
<p>
  The bookmark button beside the endpoint saves it; saved games are listed on the connect page, in the server menu of the panel header and in the command palette
  (<em>Bevy Remote: connect to …</em>). The inspector holds one connection at a time, so choosing another game replaces the current one.
</p>

<h3>Driving the game: the companion crate</h3>
<p>
  The stock protocol inspects and edits the world. To let Bennu (and Imber) also <em>drive</em> the game — screenshots, keyboard, mouse and gamepad input, named actions, frame statistics and an on-screen
  indicator that someone is attached — add the <code>arbor-bevy-remote</code> plugin, which registers extra <code>arbor.*</code> methods on the same server (Bevy 0.18):
</p>
<pre><code>app.add_plugins((
    DefaultPlugins,
    with_arbor_methods(RemotePlugin::default()),   // instead of RemotePlugin::default()
    RemoteHttpPlugin::default(),
    ArborRemotePlugin::default(),                  // ::without_indicator() hides the border
))
.register_remote_action("restart", "Start a new game", |world, _arg| Ok(serde_json::json!("ok")));</code></pre>
<p>
  The methods can be called from the <strong>Console</strong> view (<code>arbor.info</code> lists them all), and three views of the bottom panel are built on them.
</p>
<ul>
  <li><strong>Screen</strong> — the game's window as a picture. <em>Live</em> re-captures every second; with <em>Click through</em> on, a click on the picture is a click in the game at the matching spot.</li>
  <li><strong>Input</strong> — a virtual keyboard (with <kbd>Ctrl</kbd> / <kbd>Shift</kbd> / <kbd>Alt</kbd>), text, mouse and gamepad, the game's registered actions, and the indicator switch. <em>Record</em> keeps everything you send as a script with its timing; <em>Replay</em> sends it again, and the script can be copied as JSON or loaded back.</li>
  <li><strong>Stats</strong> — frame-time percentiles (p50, p95, p99, worst), a graph of the last frames against a 60 fps budget, and every diagnostic the game registered.</li>
</ul>
<p>
  While a client is attached the game draws a soft coloured border and a small label: blue when connected, orange while input is being sent, grey when paused. The Input view and the command palette switch it off; <code>ArborRemotePlugin::without_indicator()</code> removes it from the game.
</p>

<h2>When the game restarts</h2>
<p>
  Rebuilding and running the game drops the connection. Bennu notices, retries quietly in the background and picks the new process up by itself, re-reading its
  types: the old registry described a binary that no longer exists. A banner shows the retries; a notification appears only if the game stays away after the first retry,
  once when it is lost and once when it is back, or once more if Bennu eventually gives up.
</p>

<h2>The tree</h2>
<p>
  The sidebar is one tree with three roots, all closed until you open them: <strong>Entities</strong> (the hierarchy), <strong>Resources</strong> and <strong>States</strong>. Selecting a leaf shows its
  detail in the bottom panel, where it can be edited; the buttons in that panel's header switch to the game-wide views — statistics, screen, input, console and the type registry —
  and back to the detail. Expand all / collapse all are in the sidebar's toolbar; <kbd>←</kbd> and <kbd>→</kbd> close and open a node, <kbd>/</kbd> or just typing a letter starts the filter,
  <kbd>Esc</kbd> clears it.
</p>
<h3>Entities</h3>
<p>
  An entity's icon says what it is when a marker component does: a camera, a light, a mesh, a UI node, a sprite, text or a window; any other is a cube, with a branch when it has children. A node opens one level at a time, so a parent with thousands of children costs one page. The world is re-read only while the sidebar or the panel is open, once a second while it is
  changing and progressively less often while it is quiet.
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
  The filter applies to all three roots: an entity that matches stays under the parents it hangs from (names match loosely from three letters — <code>gbch</code> finds <code>Goblin Chief</code>), and resources and states match by name; a root with nothing to show disappears. <strong>Reveal in hierarchy</strong>, in the right-click menu, takes a hit back to its place. <kbd>Delete</kbd> on a row despawns it, after a
  confirmation. Ids are written the way Bevy prints them: <code>3v1</code> is slot 3, generation 1.
</p>

<p>
  The bookmark button beside the search box keeps a search under its own text and runs a saved one again; the command palette lists them too (<em>Bevy Remote: search …</em>).
</p>

<h2>Components</h2>
<p>
  Select an entity and the panel shows its components as flat, collapsible lines — a closed one summarises its value, a lone number, text or flag is edited on the line itself. The values are pushed by the game rather than polled — the <strong>Live</strong> badge says the
  stream is on — so a field that moves in the game moves here.
</p>
<ul>
  <li>Numbers, booleans, text, enums and nested structs have their own controls; short number lists such as positions are on one line, labelled by axis. Drag an axis letter to change the value (<kbd>Shift</kbd> finer, <kbd>Ctrl</kbd> coarser), double-click it to reset.</li>
  <li><code>Transform</code> shows position, rotation as angles in degrees (a toggle shows the quaternion) and scale with a lock that scales the three axes together. Colours show a swatch, a picker, a hex field and alpha. A <code>Duration</code> is seconds; a handle is a small chip; an entity reference jumps to that entity.</li>
  <li>A type's <code>#[derive(RemoteInspect)]</code> hints (below) turn a number into a slider, an angle in degrees or a date, and a list into a colour.</li>
  <li><strong>Edit as JSON</strong>, on each card, edits the value exactly as it is sent. It is always right, where a typed control can only be as good as what the game's type registry says.</li>
  <li><strong>Add component</strong> searches the registered types and starts from a well-formed placeholder value; <kbd>Ctrl</kbd> + <kbd>Enter</kbd> inserts it.</li>
  <li>A component the game could not serialise is shown with the game's own message instead of a value.</li>
</ul>

<h2>Resources, states and types</h2>
<p>
  The panel's detail is deliberately narrow: a few lines of values need a few hundred pixels, not the dock's whole width.
</p>
<p>
  A resource opens in the detail view and is edited with the same lines. <strong>States</strong> are the game's Bevy state machines, found in the type registry as <code>State&lt;T&gt;</code> beside <code>NextState&lt;T&gt;</code>: the detail shows the current variant and the others, and <em>Switch to</em> asks the game to move there (it applies it on its next state transition). A variant whose name says it ends the session — a menu, a quit, a reset — asks first. A state without a registered <code>NextState</code> can be read but not changed. The type registry view is the registry itself: every component and resource the game
  registered, with its exact path — the string the protocol wants — one click from the clipboard, and a shortcut to add the type.
</p>
<p>
  The copy button in an entity's or a resource's header copies it as <strong>JSON</strong>, as the game sent it, or as <strong>RON</strong>: the same data in RON syntax,
  fields named, lists in brackets. The RON follows the shape of the JSON, not the Rust type, so a position stays <code>[x, y, z]</code>. The palette has both for the selected entity.
</p>

<h3>Telling the inspector how to draw a field</h3>
<p>
  A bare number says nothing about being a volume, an angle or a date. <code>#[derive(RemoteInspect)]</code> (re-exported by <code>arbor-bevy-remote</code>) attaches that to a field:
</p>
<pre><code>#[derive(Component, Reflect, RemoteInspect)]
struct Lamp &#123;
    #[inspect(color)]                            tint: [f32; 3],
    #[inspect(range = 0.0..=1.0, step = 0.01)]   intensity: f32,
    #[inspect(angle)]                            beam: f32,     // radians, edited in degrees
    #[inspect(date)]                             installed: f64,
    #[inspect(label = "speed", unit = "m/s", readonly)] measured: f32,
&#125;

app.register_inspect::&lt;Lamp&gt;();</code></pre>
<p>
  A <code>///</code> doc comment on a field is its description — shown on hover and as a quiet line under the field's name — and one on the struct describes the type under its name (<code>description = "…"</code> overrides either). Hints are sent through <code>arbor.inspect_hints</code> and read once per connection. <code>color</code>, <code>range</code>, <code>angle</code> and <code>date</code> each say how to draw the field, so a field takes one; <code>step</code>, <code>readonly</code>,
  <code>label</code> and <code>unit</code> combine with them.
</p>

<h2>Console</h2>
<p>
  The <strong>Console</strong> view calls any method the game lists, with raw JSON params: the game's own remote methods, and anything newer than the inspector. Type a
  method — the suggestions are the game's own <code>rpc.discover</code> — press <kbd>Tab</kbd> to take one, then <kbd>Ctrl</kbd> + <kbd>Enter</kbd> to run it. The answer is shown as
  JSON and can be copied; calls are kept in the list on the left for this session, and picking one loads it back. A call is sent as written: a method that spawns or
  despawns does so.
</p>

<Callout variant="info" title="What the editors can and cannot know">
  The game sends each value through the type's own serialisation, which sometimes differs from the type's description: a position is described as a struct of
  <code>x</code>, <code>y</code>, <code>z</code> but arrives as <code>[x, y, z]</code>. The editors follow the value and use the description only to refine it, so an unusual type
  can fall back to the JSON editor, never to a wrong control.
</Callout>

<h2>Limits</h2>
<ul>
  <li>The protocol has no method for systems, schedules or the game's diagnostics, so there is nothing to show for them; a game that adds its own method for them can be called from the console.</li>
  <li>The protocol cannot filter or page on the game's side, so each refresh reads every entity's name and parent. A world of hundreds of thousands of entities is read
    less often, not more cheaply.</li>
  <li>Entity ids are exact up to 2<sup>53</sup>; a slot recycled millions of times is shown by its raw digits.</li>
</ul>
