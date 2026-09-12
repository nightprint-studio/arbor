<script lang="ts">
  /**
   * Tool configuration files: lombok.config, junit-platform.properties and struts.properties — properties files with a
   * published, versioned vocabulary, served for the version the project resolves.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Editor</span>
<h1>Tool configuration files</h1>

<p class="doc-lead">
  Three files in a JVM project look exactly like the hundred other <code>.properties</code> around them and are nothing like them. Their
  keys are not names somebody chose — they are an API, published per release, with defaults and legal values written down. Bennu knows
  that vocabulary, and which half of it <em>your</em> version understands.
</p>

<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">Lombok</div>
    <div class="fc-title"><code>lombok.config</code></div>
    <div class="fc-desc">Properties syntax, plus <code>+=</code>, <code>-=</code> and <code>clear</code>.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">JUnit</div>
    <div class="fc-title"><code>junit-platform.properties</code></div>
    <div class="fc-desc">Jupiter's and the Platform's keys, on one version line.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Struts</div>
    <div class="fc-title"><code>struts.properties</code></div>
    <div class="fc-desc">A legacy Struts application's constants — the security ones included.</div>
  </div>
</div>

<h2>What you get</h2>
<dl class="meta-grid">
  <dt>Completion</dt>
  <dd>
    The documented keys, each with its type and default on the right. Past the <code>=</code>, the legal values — but only where the set is
    genuinely closed: a duration, a class name or a field name has no candidates, because offering the default as the only entry would dress
    a guess up as a choice.
  </dd>
  <dt>Hover</dt>
  <dd>On a key: what it changes, what it defaults to, what it accepts, and the release it arrived in.</dd>
  <dt>Ghost text</dt>
  <dd>Only where the answer is certain — the documented default for a value left empty, or the one continuation a key prefix admits. Anything less belongs in the popup, where the alternatives show.</dd>
  <dt>Squiggles</dt>
  <dd>For the three things that are true by construction — below.</dd>
</dl>

<h2>The version is the interesting part</h2>
<p>
  Bennu reads the version out of your poms — the parent chain, the <code>&#36;&#123;…&#125;</code> properties and
  <code>&lt;dependencyManagement&gt;</code> folded in, so a version pinned in a parent or imported from a BOM counts — and offers only the
  keys that version has:
</p>
<ul>
  <li>On Lombok 1.16 you are not shown <code>lombok.addNullAnnotations</code>.</li>
  <li>On JUnit 5.2 the whole <code>junit.jupiter.execution.parallel.*</code> block is absent, because it arrived in 5.3.</li>
</ul>
<Callout variant="info" title="When the version cannot be resolved, everything is offered">
  That is the common case on a real tree — Lombok through a starter, a Gradle build with no pom, a corporate parent this machine never
  fetched — and the safe direction: a key too many, never a key missing from a project that has it. The tables follow the same rule: a key
  is dated only where the release that introduced it is certain.
</Callout>

<h2>What is flagged, and what deliberately is not</h2>
<table>
  <thead><tr><th>Flagged</th><th>Why</th></tr></thead>
  <tbody>
    <tr><td><strong>A key your version ignores</strong></td><td>It is in the file and looks right, and the tool reads the line and does nothing with it — the failure that costs an afternoon, because nothing else says so.</td></tr>
    <tr><td><strong>A deprecated key</strong></td><td>With the name of what replaced it.</td></tr>
    <tr><td><strong>A value outside a closed set</strong></td><td><code>MAYBE</code> where the key takes <code>CALL</code>, <code>SKIP</code> or <code>WARN</code>. A <code>&#36;&#123;…&#125;</code> placeholder is left alone: what it expands to is somebody else's business.</td></tr>
  </tbody>
</table>
<Callout variant="warning" title="There is no “unknown key” warning">
  A decision, not an omission. The tables are the keys worth documenting, not a transcription of every constant a tool defines — so a key
  not in them means <em>not written down here</em>, never <em>not understood</em>. A warning under a correct line is the finding people
  learn to ignore, and it takes the true ones with it.
</Callout>

<h2>Lombok</h2>
<p>
  <code>lombok.config</code> is properties syntax with two additions, both understood:
</p>
<pre><code>{@html highlightCode(`lombok.copyableAnnotations += com.fasterxml.jackson.annotation.JsonProperty
clear lombok.accessors.prefix
lombok.val.flagUsage = ERROR`, 'properties')}</code></pre>
<ul>
  <li><code>+=</code> and <code>-=</code> add to and remove from a list key — the <code>lombok.copyableAnnotations</code> and
    <code>lombok.accessors.prefix</code> shape.</li>
  <li><code>clear &lt;key&gt;</code> is a statement whose second word is a key, so it completes there too.</li>
</ul>
<p>
  The <code>lombok.&lt;feature&gt;.flagUsage</code> family is complete: every feature Lombok can report on, each taking
  <code>WARNING</code>, <code>ERROR</code> or <code>ALLOW</code> — the lever for banning a feature a team has decided against. The file
  means nothing before Lombok 1.14, which is where the version floor sits.
</p>

<h2>JUnit</h2>
<pre><code>{@html highlightCode(`junit.jupiter.execution.parallel.enabled = true`, 'properties')}</code></pre>
<p>
  <code>junit-platform.properties</code> carries two namespaces on two version lines: <code>junit.jupiter.*</code> belongs to Jupiter
  (<code>5.x</code>), <code>junit.platform.*</code> to the Platform (<code>1.x</code>). They are not independent — the release train
  ships Platform 1.<em>N</em> with Jupiter 5.<em>N</em>, always — so every version Bennu reports is the <strong>Jupiter</strong> one,
  even for a <code>junit.platform.*</code> key.
</p>
<p>
  The file is found anywhere in the project, in practice <code>src/test/resources</code>. Each of these three files has an icon of its own
  in the project tree, so it stands apart from the message bundle beside it.
</p>

<h2>Struts</h2>
<pre><code>{@html highlightCode(`struts.enable.DynamicMethodInvocation = false`, 'properties')}</code></pre>
<p>
  <code>struts.properties</code> is where a legacy Struts application is configured, and its keys are documented in a
  <code>default.properties</code> inside <code>struts2-core.jar</code> that nobody opens. Completion, hover and the version gate work as
  above, dated by the <code>struts2-core</code> version the project resolves.
</p>
<p>
  The same constants can be written three ways — here, as <code>&lt;constant name="…"/&gt;</code> in <code>struts.xml</code>, or as an
  <code>&lt;init-param&gt;</code> in <code>web.xml</code>. The <code>.properties</code> spelling is the one served, because the XML one
  already has a DTD behind it.
</p>
<Callout variant="warning" title="The security constants are the point">
  Several are what an advisory was about. <code>struts.enable.DynamicMethodInvocation</code> chooses a method from the URL, and was on by
  default before 2.5. <code>struts.ognl.allowStaticFieldAccess</code> is the half of static access that survived;
  <code>struts.ognl.allowStaticMethodAccess</code> is the half that did not — a value for it does nothing from 2.5, and the hover says so
  where it is written. The <code>struts.excluded*</code> deny lists are the ones every release extends, which is why
  <code>struts.additional.excludedPatterns</code> exists: it <em>adds</em> to the built-in list where the others replace it.
</Callout>
