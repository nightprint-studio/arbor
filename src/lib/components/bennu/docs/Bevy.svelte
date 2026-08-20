<!-- Bennu docs — Bevy ECS support. -->
<h1>Bevy ECS</h1>
<p class="doc-lead">
  In an ECS the architecture <em>is</em> the data: a component and the systems that read and write
  it say more about how a game works than any file layout does. Bennu reads that out of the source
  — the declarations, the system signatures, and the pairs of systems whose accesses cannot run at
  the same time. No build, and no running game.
</p>

<h2>When it turns on</h2>
<p>
  On a project whose Cargo manifest declares <code>bevy</code> (or any <code>bevy_*</code> crate),
  corroborated by the sources — a <code>#[derive(Component)]</code>, an <code>add_systems</code>
  call. Which signal convinced Bennu is listed under <strong>Projects &amp; capabilities</strong>.
  A Maven project never carries this tooling at all.
</p>

<h2>Components</h2>
<p>
  <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>B</kbd>, or the rail button. One row per declared
  <code>Component</code>, <code>Resource</code>, <code>Message</code>, <code>Event</code>,
  <code>Bundle</code> and <code>States</code>; the badge says which. Expand a row for the systems
  that touch it — each sub-row naming the system, whether it reads or writes, and the parameter it
  was read from:
</p>
<pre><code>{`Health          Component    read by 2 · written by 1
  damage         write        mut q: Query<&mut Health>          Update
  draw_health    read         q: Query<&Health, With<Player>>    Update
  PlayerBundle   in bundle    carries Health`}</code></pre>
<p>
  This is find-usages asked properly. <em>Who writes <code>Health</code></em> is a question about
  signatures, and a text search answers it with every comment that mentions the word.
</p>
<p>
  A <strong>marker</strong> — a component nothing reads, whose whole job is
  <code>With&lt;Player&gt;</code> — is counted separately, as <em>filtered on by</em>. A filter is
  not an access (two systems filtering on the same marker do not contend, and counting them as if
  they did would invent conflicts), but a marker twenty queries depend on is not a component nobody
  uses either, and the row says which of the two it is.
</p>

<p>
  A <strong>message</strong> and an <strong>event</strong> are two roles, not two names for one
  thing: a message is buffered — written with a <code>MessageWriter</code>, drained by whoever reads
  it — while an event is triggered and delivered to observers. Both are found by their readers and
  writers: a message through the queue it is posted to, an observer event through the
  <code>On&lt;…&gt;</code> parameter of whoever handles it.
</p>

<h2>Systems</h2>
<p>
  One row per system, badged by the schedule it was registered in — so grouping by badge groups by
  schedule. The row summarises what it touches; expanding it lists each access with its query
  filters. A system Bennu found no <code>add_systems</code> call for is tagged as such rather than
  hidden: on a project that registers systems through a helper, that tag is the honest answer.
</p>

<h2>Access conflicts</h2>
<p>
  Two systems in one schedule that want the same data — one of them mutably — can never run in
  parallel. Bevy derives that from the same signatures at schedule-build time; so does this panel:
</p>
<pre><code>{`damage ⇄ tick        Update    contend over Score      unordered
  Score              write/write — mut score: ResMut<Score>
  tick                            mut score: ResMut<Score>`}</code></pre>
<p>
  <strong>Not a bug list.</strong> Two conflicting systems that are explicitly ordered are working
  as intended — that is what <code>.before</code>, <code>.after</code> and <code>.chain</code> are
  for, and the row is tagged <code>ordered</code>. The tag worth looking at is
  <code>unordered</code>: those two are serialised in whichever order the schedule happens to pick,
  which is a frame-order dependency nobody wrote down.
</p>
<p>
  An exclusive system — one taking <code>&amp;mut World</code> — contends with everything in its
  schedule, and gets one row saying so rather than one per component.
</p>
<p>
  An <code>unordered</code> pair is also <strong>warned about in the editor</strong>, on the name of
  each of the two systems. Only that case: a conflict is not a defect, and marking the ordered ones
  would put a permanent squiggle under half the systems in the project. Two narrowings keep the
  warning honest — a system that is in a <strong>set</strong> is never accused, because a set's
  ordering is declared by <code>configure_sets</code> and Bennu does not read those, and neither is
  an <strong>exclusive</strong> system, which contends with everything by construction. Both still
  appear in the panel.
</p>

<h2>In the editor</h2>
<p>
  Every ECS declaration carries a gutter mark: <code>◈</code> a component, <code>▣</code> a
  resource, <code>✉</code> a message, <code>✳</code> an observer event, <code>▦</code> a bundle.
  Clicking it opens the systems that touch it — one jumps, several ask.
</p>

<h2>Engines built on Bevy</h2>
<p>
  A project on an engine of its own often never writes <code>Res</code> or
  <code>MessageReader</code> at all: it declares <code>#[derive(DomainResource)]</code> and takes a
  <code>DomainResMutParam&lt;Board&gt;</code>, and the engine's parameter does the
  <code>Res</code> underneath. Bennu reads the wrapper as what it wraps, so the declaration still
  lists the systems that touch it and the pair that contends over it still pairs.
</p>
<p>
  The wrappers it knows are <strong>fulcrum</strong>'s per-domain layer —
  <code>DomainResParam</code>, <code>DomainResMutParam</code>, <code>DomainStateParam</code>,
  <code>DomainStateMutParam</code>, <code>DomainMessageReader</code>,
  <code>DomainMessageWriter</code>, <code>DomainQuery</code> — plus its derives
  (<code>DomainResource</code>, <code>DomainMessage</code>, <code>DomainState</code>), which name
  the same roles as Bevy's own. A <code>#[derive(SystemParam)]</code> the <em>project itself</em>
  declares is read from its own fields and needs no table.
</p>

<h2>Where a component is created</h2>
<p>
  A signature says who <em>reads</em> a component. Nothing in a signature says who ever
  <strong>makes</strong> one — so a type that six systems read has, on the evidence of its
  parameters alone, no origin at all. Under every declaration is therefore a row per site that
  puts it into the world: a <code>spawn</code>, an <code>insert</code> on an entity that already
  exists, an <code>insert_resource</code>, an <code>add_message</code>, an
  <code>init_state</code>. Each names the function it happens in and the argument it was given.
</p>
<p>
  Read from call sites rather than from signatures, so a <code>spawn((Health(100.0), Player))</code>
  is one row under <code>Health</code> and one under <code>Player</code> — which is what a
  bundle-as-tuple means. A value whose type cannot be named from the expression is skipped rather
  than guessed at, and the call names are a closed list: a method on your own type called
  <code>insert</code> contributes nothing.
</p>

<h2>Materials and shaders</h2>
<p>
  A <code>#[derive(Asset)]</code> type is a row in the components list like any other — the
  question "who touches <code>SpiralHoverMaterial</code>" is answered by the same signatures, so
  splitting it into a panel of its own would split one question in two. An asset is reached
  through the <code>Assets&lt;T&gt;</code> resource that stores it rather than by its own name,
  which is how it is looked up here.
</p>
<p>
  A material — an asset that also runs a shader — carries its shaders as the first rows under it,
  and the whole relationship is described under <em>Shaders (WGSL)</em>: what the two files have
  to agree about, what Bennu checks, and how to get from one to the other.
</p>

<h2>What it does not claim</h2>
<p>
  Bennu reads <strong>this project's own sources</strong> and nothing else. The engine's plugins and
  a dependency's systems are not in the picture, which decides what is safe to say and what is not:
</p>
<ul>
  <li>
    <strong>A conflict stays true.</strong> Two systems contending over <code>Score</code> still
    contend however many systems a plugin adds — so the report is short of pairs rather than full
    of invented ones.
  </li>
  <li>
    <strong>Parallelism is never claimed.</strong> "These two run at the same time" cannot be shown
    from part of a schedule: one unseen system writing the same component would refute it.
  </li>
  <li>
    <strong>There is no ordering graph.</strong> Most of an app's ordering lives in plugins, so a
    picture drawn from this project's <code>add_systems</code> calls would be a fragment presented
    as a whole. Ordering appears only as the tag on a conflict row, where it means "as far as this
    project's own registrations go".
  </li>
  <li>
    <strong>Names are not resolved.</strong> <code>Health</code> is matched by name, so two types
    called <code>Health</code> in two modules look like one — though a generic argument is kept, so
    <code>NextState&lt;GameState&gt;</code> and <code>NextState&lt;MenuPage&gt;</code> stay apart.
    Every row shows the parameter it came from, which is what lets a wrong one be recognised.
  </li>
  <li>
    <strong>Registration is read literally.</strong> A system added behind a <code>cfg</code>, in a
    loop, or by a macro is not seen as registered — it appears in the catalog with no schedule.
  </li>
</ul>
