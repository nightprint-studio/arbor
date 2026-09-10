<!-- Bennu docs — scheduled work: what a job says, and the two ways it silently never runs. -->
<h1>Scheduled jobs</h1>
<p class="doc-lead">
  Every <code>@Scheduled</code> method the project declares, what its cron expression says
  <em>in words</em>, and the two ways a job never runs without anything failing.
</p>

<h2>Nobody reads cron</h2>
<p>
  <code>0 0 2 * * ?</code> is four seconds of counting fields, every single time — and the counting
  is where the mistake happens, because the field that moved is the one you did not count. Hovering
  a cron expression says <em>every day at 02:00</em>, and the Scheduled panel shows the words rather
  than the expression in its second column.
</p>
<p>
  An expression Bennu cannot phrase confidently says nothing at all, rather than something almost
  right.
</p>

<h2>The two silent failures</h2>
<ul>
  <li>
    <strong>The expression is not one.</strong> A five-field Unix crontab line pasted into
    <code>@Scheduled(cron = …)</code> — which is what people paste — an hour of <code>25</code>, a
    misspelled <code>@dayly</code>. Spring and Quartz both refuse it, at startup, in a stack trace
    nobody reads until the report has not arrived for a week. The five-field case is named for what
    it is: it needs a leading seconds field here.
  </li>
  <li>
    <strong>Scheduling was never switched on.</strong> <code>@Scheduled</code> on a bean of a project
    with no <code>@EnableScheduling</code> is an annotation with nothing behind it. Nothing fails,
    nothing logs, the method is simply never called — and the class looks completely correct.
    <strong><code>@SpringBootApplication</code> alone is not enough</strong>, which is the trap:
    Boot's autoconfiguration needs the annotation to be somewhere.
  </li>
</ul>
<p>
  The second is reported on <em>every</em> <code>@Scheduled</code> in the file rather than once
  somewhere central — there is no central place to go and look, and the person who needs to know is
  the one reading the method that is not being called.
</p>
<p>
  A third, smaller one: an <code>@Scheduled</code> that says nothing about <em>when</em>. Exactly one
  of <code>cron</code>, <code>fixedRate</code> or <code>fixedDelay</code> is required, and Spring
  refuses the bean without one.
</p>

<h2>What it will not judge</h2>
<p>
  A cron written as a property placeholder (<code>&#36;&#123;report.cron&#125;</code>) or as a
  constant is resolved from something Bennu cannot see, so nothing is claimed about it in either
  direction — the alternative is reporting every externalised schedule in the project as broken.
  Spring's <code>-</code> marker, which means "declared and switched off", is left alone too.
</p>
<p>
  And nothing is said about scheduling being off until the project has actually been scanned:
  reporting a cold start as the project's defect is the worst kind of noise.
</p>
