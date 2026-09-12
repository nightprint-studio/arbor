<script lang="ts">
  /**
   * Scheduled jobs: cron expressions in words, and the ways a job silently never runs.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>Scheduled jobs</h1>

<p class="doc-lead">
  Every <code>@Scheduled</code> method the project declares, what its cron expression says <em>in words</em>, and the ways a job never runs without anything failing.
</p>

<h2>Nobody reads cron</h2>
<p>
  <code>0 0 2 * * ?</code> is four seconds of counting fields every time — and the counting is where the mistake happens, because the field that moved is the one you did
  not count. Hovering a cron expression says it in words, and the <strong>Scheduled</strong> panel shows the words rather than the expression in its second column.
</p>
<table>
  <thead><tr><th>Expression</th><th>Hover says</th></tr></thead>
  <tbody>
    <tr><td><code>0 0 2 * * ?</code></td><td><em>every day at 02:00</em></td></tr>
  </tbody>
</table>
<p>An expression Bennu cannot phrase confidently says nothing at all, rather than something almost right.</p>

<h2>The silent failures</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">Refused at startup</div>
    <div class="fc-title">The expression is not one</div>
    <div class="fc-desc">A five-field Unix crontab line pasted into <code>@Scheduled(cron = …)</code> — what people paste — an hour of <code>25</code>, a misspelled <code>@dayly</code>. Spring and Quartz refuse it in a stack trace nobody reads until the report has not arrived for a week. The five-field case is named for what it is: it needs a leading seconds field.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Never called</div>
    <div class="fc-title">Scheduling was never switched on</div>
    <div class="fc-desc"><code>@Scheduled</code> in a project with no <code>@EnableScheduling</code> is an annotation with nothing behind it. Nothing fails, nothing logs, and the class looks completely correct.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Refused by Spring</div>
    <div class="fc-title">No "when" at all</div>
    <div class="fc-desc">Exactly one of <code>cron</code>, <code>fixedRate</code> or <code>fixedDelay</code> is required.</div>
  </div>
</div>
<pre><code>{@html highlightCode(`@Scheduled(cron = "0 2 * * *")      // five fields: a Unix crontab, not a Spring cron
public void nightlyReport() { … }`, 'java')}</code></pre>
<Callout variant="warning" title="@SpringBootApplication alone is not enough">
  That is the trap: Boot's autoconfiguration needs <code>@EnableScheduling</code> to be somewhere. The warning is shown on <em>every</em> <code>@Scheduled</code> in the
  file rather than once somewhere central — there is no central place to look, and the person who needs to know is the one reading the method that is not being called.
</Callout>

<h2>What it will not judge</h2>
<ul>
  <li>A cron written as a placeholder — <code>&#36;&#123;report.cron&#125;</code> — or as a constant is resolved from something Bennu cannot see, so nothing is claimed about it
    either way; the alternative reports every externalised schedule as broken.</li>
  <li>Spring's <code>-</code> marker, meaning "declared and switched off", is left alone.</li>
  <li>Nothing is said about scheduling being off until the project has actually been scanned: reporting a cold start as the project's defect is the worst kind of noise.</li>
</ul>
