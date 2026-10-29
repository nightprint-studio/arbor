<script lang="ts">
  /**
   * Dependency security: the OSV.dev check — what is sent, when it runs, how findings are shown in the pom, the
   * Dependencies panel and the Vulnerabilities window, and what the check does not do.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Projects</span>
<h1>Dependency security</h1>

<p class="doc-lead">
  Whether a library you depend on has a published vulnerability is not something a machine can know by itself. Bennu asks
  OSV.dev, tells you where in the pom the affected dependency is written, and which version fixes it.
</p>

<h2>What is sent, and to whom</h2>
<p>
  The data source is <strong>OSV.dev</strong>, the open vulnerability database that aggregates GitHub advisories and other
  sources. For each dependency Bennu sends the Maven <code>groupId:artifactId</code> and the version, and nothing else about
  the project, to <code>api.osv.dev</code>.
</p>
<Callout variant="warning" title="Different from the version check">
  The newer-version check reads a public file and says nothing about your project. This one sends your dependency
  coordinates to a third party. The two have separate switches.
</Callout>
<p>
  <strong>Settings → Java → Check dependencies for known vulnerabilities</strong> controls it, and it is on by default.
  Off, nothing is sent: the pom shows no warnings, the Dependencies panel no badges, and the Vulnerabilities window explains
  the check is off and offers <em>Turn on</em>.
</p>

<h2>When it is asked</h2>
<ul>
  <li><strong>Editing a pom.xml</strong> — shortly after you stop typing, in the background. A cold lookup can take a few seconds and typing never waits for it.</li>
  <li><strong>The Dependencies tool window</strong> — once the dependency list has loaded, to draw the badges. Maven projects only.</li>
  <li><strong>The Vulnerabilities window</strong> — on opening, and again on <em>Refresh</em>.</li>
</ul>
<p>
  Cargo projects are not checked: the check is for Maven coordinates, and the palette entry is offered on Maven projects only.
</p>

<h3>Caching, limits and offline</h3>
<ul>
  <li>An answer is cached on disk, one small file per dependency and version, and reused for the same freshness window as the version check (a day by default).</li>
  <li>A request looks up a bounded number of dependencies and advisory records. What it does not reach stays uncached, so the next pass continues: a large pom fills in over a few passes. The Vulnerabilities window checks up to 400 distinct artifacts at a time and says so when a classpath is larger.</li>
  <li>With no network, or when OSV cannot be reached, the check is silent — no error, no dialog. Fresh cached answers are still shown; an expired cached answer is <em>not</em> reused offline (unlike the newer-version check), so after a day of being offline the warnings go quiet until the next successful lookup.</li>
  <li>A dependency whose advisory details could only be partly fetched is shown with what is known and is not cached, so the next pass retries it.</li>
</ul>

<h2>Which version is checked</h2>
<p>
  A dependency with a literal version is checked at that version. One with no literal version — managed by a BOM or a parent, or
  written as a property — is checked at the version the build <em>resolved</em>, which needs the classpath to be resolved and
  happens as the project indexes. Coordinates containing an unexpanded property are skipped.
</p>
<p>
  The pom only marks what it declares. The Vulnerabilities window and the panel badges also cover everything those
  dependencies drag in, at the version actually on the classpath.
</p>

<h2>How findings are shown</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">In the pom</div>
    <div class="fc-title">A warning on the dependency</div>
    <div class="fc-desc">
      The version is underlined (or the <code>&lt;dependency&gt;</code> tag, when there is no version text). Hover for up to
      four advisories — the CVE, or the advisory id when there is none, its severity, a one-line summary and the nearest
      version that fixes it — then "…and N more". The warning is drawn in the editor; the Problems panel lists only the
      synchronous checks.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">In the Dependencies tool window</div>
    <div class="fc-title">A shield with a count</div>
    <div class="fc-desc">
      A declared or transitive dependency with known vulnerabilities carries a shield with the number, coloured by the
      worst severity. It appears only while the check is on, and the tooltip points to the Vulnerabilities window.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">The whole project</div>
    <div class="fc-title">Vulnerabilities…</div>
    <div class="fc-desc">
      The shield button in the Dependencies panel's toolbar, or <strong>Vulnerabilities…</strong> in the Command Palette
      (also reachable by asking the assistant to show them). The same window the Corvus security dashboard uses: severity
      tabs with counts, worst first, one row per advisory, and a detail view with the summary, whether the dependency is declared
      (and in which modules) or pulled in transitively, the fixed version, and links to osv.dev and — for a CVE — NVD.
    </div>
  </div>
</div>

<h3>Severity</h3>
<p>
  GitHub advisories carry a plain severity word, which is used (OSV's <em>moderate</em> reads as <em>medium</em>). Records from other
  databases carry only a CVSS vector, which is not re-derived: they read as <em>unknown</em> and are still reported. Unknown ranks above low — unrated
  is not harmless.
</p>

<h2>Acting on a finding</h2>
<p>
  The finding names the version that fixes it — the lowest fixed version above the one you have, taken from the
  advisory's ranges for that artifact — or says the advisory lists none. Moving there is a manual edit of the version (or the property, parent or
  BOM that decides it): see <strong>Dependency versions</strong> for where a version comes from.
</p>
<Callout variant="tip" title="The newer-version line is not a fix">
  The line above a dependency offers the newest release on Maven Central, not the fixed version of an advisory. It is often
  the same one, but nothing ties them together.
</Callout>

<h2>What it does not do</h2>
<ul>
  <li>There is no one-click "update to the fixed version", and no way to ignore or suppress a finding.</li>
  <li>It does not check Cargo or npm dependencies.</li>
  <li>It does not say whether your code reaches the vulnerable code — only that the version is affected.</li>
  <li>The freshness window follows the shared metadata setting; there is no separate control for it.</li>
</ul>
