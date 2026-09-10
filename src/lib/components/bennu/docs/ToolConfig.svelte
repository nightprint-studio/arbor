<!-- Bennu docs — lombok.config and junit-platform.properties: properties files with a published,
     versioned vocabulary behind them. -->
<h1>Tool configuration files</h1>
<p class="doc-lead">
  Two files in a JVM project look exactly like the hundred other <code>.properties</code> around
  them and are nothing like them: <code>lombok.config</code> and
  <code>junit-platform.properties</code>. Their keys are not names somebody chose — they are an
  API, published per release, with defaults and legal values written down. Bennu knows that
  vocabulary, and knows which half of it <em>your</em> version understands.
</p>

<h2>What you get</h2>
<ul>
  <li><strong>Completion</strong> over the documented keys, with the type and the default on the
    right of each one. Past the <code>=</code>, the legal values — but only where the set is
    genuinely closed. A duration, a class name or a field name has no candidates, because offering
    the current default as the only entry would dress a guess up as a choice.</li>
  <li><strong>Hover</strong> on a key: what it changes, what it defaults to, what it accepts, and
    the release it arrived in.</li>
  <li><strong>Ghost text</strong> where the answer is certain — the documented default for a value
    you have left empty, or the one continuation a key prefix admits. Never a ranking: anything
    less than certain belongs in the popup, where the alternatives are visible.</li>
  <li><strong>Squiggles</strong> for the three things that are true by construction — see below.</li>
</ul>

<h2>The version is the interesting part</h2>
<p>
  Bennu reads the version out of your project's poms — the parent chain, the
  <code>&#36;&#123;…&#125;</code> properties and <code>&lt;dependencyManagement&gt;</code>
  folded in, so a version pinned in a parent or imported from a BOM counts — and then offers only
  the keys that version has. On Lombok 1.16 you are not shown
  <code>lombok.addNullAnnotations</code>; on JUnit 5.2 the whole
  <code>junit.jupiter.execution.parallel.*</code> block is absent, because it arrived in 5.3.
</p>
<p>
  <strong>When the version cannot be resolved, everything is offered.</strong> That is the common
  case on a real tree — Lombok arriving through a starter, a Gradle build with no pom at all, a
  corporate parent this machine has never fetched — and it is the safe direction: a key too many,
  never a key missing from a project that has it. The same rule governs the tables themselves. A
  key is dated only where the release that introduced it is certain; the rest are offered to
  everybody.
</p>

<h2>What is flagged, and what deliberately is not</h2>
<ul>
  <li><strong>A key your version ignores.</strong> It is in the file, it looks right, and the tool
    reads the line and does nothing with it — the failure that otherwise costs an afternoon,
    because nothing anywhere says so.</li>
  <li><strong>A deprecated key</strong>, with the name of what replaced it.</li>
  <li><strong>A value outside a closed set</strong> — <code>MAYBE</code> where the key takes
    <code>CALL</code>, <code>SKIP</code> or <code>WARN</code>. A value that is a
    <code>&#36;&#123;…&#125;</code> placeholder is left alone: what it expands to is somebody
    else's business.</li>
</ul>
<p>
  There is <strong>no “unknown key” warning</strong>, and that is a decision rather than an
  omission. The tables are the keys worth documenting, not a transcription of every constant the
  tool defines — so a key that is not in one means <em>not written down here</em> and never
  <em>not understood</em>. A warning under a line that is perfectly correct is the finding people
  learn to ignore, and it takes the true ones with it.
</p>

<h2>Lombok</h2>
<p>
  <code>lombok.config</code> is properties syntax with two additions, and both are understood:
  <code>+=</code> and <code>-=</code> add to and remove from a list key (the
  <code>lombok.copyableAnnotations</code> and <code>lombok.accessors.prefix</code> shape), and
  <code>clear &lt;key&gt;</code> is a statement whose second word is a key — so it completes there
  too.
</p>
<p>
  The <code>lombok.&lt;feature&gt;.flagUsage</code> family is complete: every feature Lombok can
  report on, each taking <code>WARNING</code>, <code>ERROR</code> or <code>ALLOW</code> — the lever
  for banning a feature a team has decided against. The file itself means nothing before Lombok
  1.14, which is where the version floor sits.
</p>

<h2>JUnit</h2>
<p>
  <code>junit-platform.properties</code> carries two namespaces on two version lines:
  <code>junit.jupiter.*</code> belongs to Jupiter (<code>5.x</code>) and
  <code>junit.platform.*</code> to the Platform (<code>1.x</code>). They are not independent — the
  release train ships Platform 1.<em>N</em> with Jupiter 5.<em>N</em>, always — so every version
  Bennu reports is the <strong>Jupiter</strong> one, including for a <code>junit.platform.*</code>
  key.
</p>
<p>
  The file is found anywhere in the project, which in practice means
  <code>src/test/resources</code>. Both files get the same icon treatment in the project tree, so
  they are distinguishable at a glance from the message bundle sitting beside them.
</p>
