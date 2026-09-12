<script lang="ts">
  /**
   * Bevy ECS: the components and systems read out of the source, access conflicts between systems, gutter marks, engines
   * built on Bevy, where components are created, materials, and what is deliberately not claimed.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Rust</span>
<h1>Bevy ECS</h1>

<p class="doc-lead">
  In an ECS the architecture <em>is</em> the data: a component and the systems that read and write it say more about how a game works than any file layout. Bennu reads
  that out of the source — the declarations, the system signatures, and the pairs of systems whose accesses cannot run at the same time. No build, no running game.
</p>

<h2>When it turns on</h2>
<p>
  On a project whose Cargo manifest declares <code>bevy</code> or any <code>bevy_*</code> crate, corroborated by the sources — a <code>#[derive(Component)]</code>, an
  <code>add_systems</code> call. Which signal convinced Bennu is listed under <strong>Projects</strong>. A Maven project never carries this tooling.
</p>

<h2>Components</h2>
<p>
  <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>B</kbd>, or the rail button. One row per declared <code>Component</code>, <code>Resource</code>, <code>Message</code>,
  <code>Event</code>, <code>Bundle</code> and <code>States</code>, badged. Expand a row for the systems touching it — each naming the system, whether it reads or writes,
  and the parameter it was read from:
</p>
<pre><code>{`Health          Component    read by 2 · written by 1
  damage         write        mut q: Query<&mut Health>          Update
  draw_health    read         q: Query<&Health, With<Player>>    Update
  PlayerBundle   in bundle    carries Health`}</code></pre>
<p>
  This is find usages asked properly: <em>who writes <code>Health</code></em> is a question about signatures, and a text search answers it with every comment mentioning the word.
</p>
<dl class="meta-grid">
  <dt>A marker</dt>
  <dd>A component nothing reads, whose whole job is <code>With&lt;Player&gt;</code>, is counted apart, as <em>filtered on by</em>. A filter is not an access — two systems filtering on one marker do not contend — but a marker twenty queries depend on is not unused either.</dd>
  <dt>Messages and events</dt>
  <dd>Two roles, not two names: a message is buffered — written with a <code>MessageWriter</code>, drained by whoever reads it — an event is triggered and delivered to observers. A message is found through the queue it is posted to, an observer event through the <code>On&lt;…&gt;</code> parameter handling it.</dd>
</dl>

<h2>Systems</h2>
<p>
  One row per system, badged by the schedule it was registered in, so grouping by badge groups by schedule. The row summarises what it touches; expanding lists each access
  with its query filters. A system with no <code>add_systems</code> call found is tagged so rather than hidden — on a project registering through a helper, that tag is the
  honest answer.
</p>

<h2>Access conflicts</h2>
<p>
  Two systems in one schedule wanting the same data, one of them mutably, can never run in parallel. Bevy derives that from the signatures when it builds the schedule, and so
  does this panel:
</p>
<pre><code>{`damage ⇄ tick        Update    contend over Score      unordered
  Score              write/write — mut score: ResMut<Score>
  tick                            mut score: ResMut<Score>`}</code></pre>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Working as intended</div>
    <div class="fc-title"><code>ordered</code></div>
    <div class="fc-desc">Two conflicting systems explicitly ordered — what <code>.before</code>, <code>.after</code> and <code>.chain</code> are for.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Worth a look</div>
    <div class="fc-title"><code>unordered</code></div>
    <div class="fc-desc">Serialised in whichever order the schedule picks: a frame-order dependency nobody wrote down.</div>
  </div>
</div>
<p>
  An exclusive system — one taking <code>&amp;mut World</code> — contends with everything in its schedule, and gets one row saying so rather than one per component.
</p>
<Callout variant="warning" title="Not a bug list — except the unordered pairs">
  Only an <code>unordered</code> pair is also warned about in the editor, on the name of each system: marking the ordered ones would put a permanent squiggle under half the
  systems. A system in a <strong>set</strong> is never accused — a set's ordering is declared by <code>configure_sets</code>, which Bennu does not read — nor is an
  <strong>exclusive</strong> one, contending with everything by construction. Both still appear in the panel.
</Callout>

<h2>In the editor</h2>
<table>
  <thead><tr><th>Mark</th><th>Beside</th></tr></thead>
  <tbody>
    <tr><td><code>◈</code></td><td>A component</td></tr>
    <tr><td><code>▣</code></td><td>A resource</td></tr>
    <tr><td><code>✉</code></td><td>A message</td></tr>
    <tr><td><code>✳</code></td><td>An observer event</td></tr>
    <tr><td><code>▦</code></td><td>A bundle</td></tr>
  </tbody>
</table>
<p>Clicking one opens the systems that touch it — one jumps, several ask.</p>

<h2>Engines built on Bevy</h2>
<p>
  A project on an engine of its own may never write <code>Res</code> or <code>MessageReader</code>: it declares <code>#[derive(DomainResource)]</code> and takes a
  <code>DomainResMutParam&lt;Board&gt;</code>, and the engine's parameter does the <code>Res</code> underneath. Bennu reads the wrapper as what it wraps, so the declaration still
  lists its systems and a contending pair still pairs.
</p>
<p>
  The wrappers known are <strong>fulcrum</strong>'s per-domain layer — <code>DomainResParam</code>, <code>DomainResMutParam</code>, <code>DomainStateParam</code>,
  <code>DomainStateMutParam</code>, <code>DomainMessageReader</code>, <code>DomainMessageWriter</code>, <code>DomainQuery</code> — and its derives <code>DomainResource</code>,
  <code>DomainMessage</code>, <code>DomainState</code>. A <code>#[derive(SystemParam)]</code> the <em>project itself</em> declares is read from its own fields and needs no table.
</p>

<h2>Where a component is created</h2>
<p>
  A signature says who <em>reads</em> a component; nothing in one says who <strong>makes</strong> it — so a type six systems read has, by its parameters alone, no origin. Under
  every declaration is a row per site putting it into the world: a <code>spawn</code>, an <code>insert</code> on an existing entity, an <code>insert_resource</code>, an
  <code>add_message</code>, an <code>init_state</code> — each naming its function and the argument given.
</p>
<p>
  Read from call sites, so <code>spawn((Health(100.0), Player))</code> is a row under <code>Health</code> and one under <code>Player</code> — what a bundle-as-tuple means. A value
  whose type cannot be named from the expression is skipped, and the call names are a closed list: an <code>insert</code> method on your own type contributes nothing.
</p>

<h2>Materials and shaders</h2>
<p>
  A <code>#[derive(Asset)]</code> type is a row in the components list like any other — "who touches <code>SpiralHoverMaterial</code>" is answered by the same signatures. An asset
  is reached through the <code>Assets&lt;T&gt;</code> resource storing it, which is how it is looked up here. A material carries its shaders as the first rows under it, and the
  relationship is described in <strong>Shaders (WGSL)</strong>.
</p>

<h2>What it does not claim</h2>
<p>
  Bennu reads <strong>this project's own sources</strong> and nothing else — not the engine's plugins, not a dependency's systems — which decides what is safe to say:
</p>
<dl class="meta-grid">
  <dt>A conflict stays true</dt>
  <dd>Two systems contending over <code>Score</code> contend however many systems a plugin adds — so the report is short of pairs rather than full of invented ones.</dd>
  <dt>Parallelism is never claimed</dt>
  <dd>"These two run at the same time" cannot be shown from part of a schedule: one unseen system writing the same component would refute it.</dd>
  <dt>No ordering graph</dt>
  <dd>Most of an app's ordering lives in plugins, so a picture from this project's <code>add_systems</code> calls would be a fragment presented as a whole. Ordering appears only as the tag on a conflict row.</dd>
  <dt>Names are not resolved</dt>
  <dd><code>Health</code> is matched by name, so two <code>Health</code> types in two modules look like one — though generic arguments are kept, so <code>NextState&lt;GameState&gt;</code> and <code>NextState&lt;MenuPage&gt;</code> stay apart. Every row shows its parameter, so a wrong one can be recognised.</dd>
  <dt>Registration is read literally</dt>
  <dd>A system added behind a <code>cfg</code>, in a loop or by a macro is not seen as registered — it appears with no schedule.</dd>
</dl>
