<script lang="ts">
  /**
   * Mockito: the misuse that throws somewhere else — unfinished stubbings and verifications, matchers mixed with plain values,
   * @Mock fields nothing initialises — the Alt+Enter fixes, and what is deliberately not judged.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Build, run &amp; test</span>
<h1>Mockito</h1>

<p class="doc-lead">
  Mockito used wrongly, caught in the test that is wrong — instead of in the stack trace of the test that ran after it.
</p>

<h2>Why the exception points at the wrong line</h2>
<p>
  Mockito remembers a half-built stubbing or verification between calls, and notices it only when it is next used — by the <strong>next</strong> Mockito call. That call is often the
  first line of another test, so the stack trace names a line that is fine. The broken line compiles and reads naturally; it is only found by reading it.
</p>

<h2>What is checked</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">Error · UnfinishedStubbingException</div>
    <div class="fc-title">A stubbing never finished</div>
    <div class="fc-desc"><code>when(…)</code> or <code>given(…)</code> as a statement of its own, with no <code>thenReturn</code>; a <code>doReturn(…).when(mock)</code> that never names the method.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Error · UnfinishedVerificationException</div>
    <div class="fc-title">A verification never finished</div>
    <div class="fc-desc"><code>verify(mock)</code> or <code>then(mock).should()</code> with no method called on it. The line verifies nothing.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Error · InvalidUseOfMatchersException</div>
    <div class="fc-title">Matchers beside plain values</div>
    <div class="fc-desc">When one argument of a stubbed or verified call is a matcher, all of them must be. Each plain value is marked.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Warning · NullPointerException</div>
    <div class="fc-title">@Mock fields nothing initialises</div>
    <div class="fc-desc">No <code>MockitoExtension</code>, no <code>MockitoJUnitRunner</code>, no <code>openMocks(this)</code>: the fields stay null, and the first stubbing fails as if the code under test were broken.</div>
  </div>
</div>
<pre><code>{@html highlightCode(`when(repo.find(1));                  // unfinished: what does find(1) return?
doReturn(order).when(repo);          // unfinished: which method?
verify(repo);                        // unfinished: verifies nothing
verify(repo).save(any(), 5);         // 5 is a plain value beside a matcher`, 'java')}</code></pre>

<h2>Alt+Enter</h2>
<table>
  <thead><tr><th>On</th><th>Offers</th></tr></thead>
  <tbody>
    <tr><td>A plain value beside matchers</td><td><em>Wrap plain arguments in eq(…)</em> — every one in the call, not just the one under the caret. <code>eq</code> is written the way the file already reaches it: bare, qualified like the call's own matchers (<code>ArgumentMatchers.eq</code>), or bare with <code>import static org.mockito.ArgumentMatchers.eq</code> added.</td></tr>
    <tr><td>An uninitialised <code>@Mock</code></td><td><em>Add @ExtendWith(MockitoExtension.class)</em> on JUnit 5, <em>Add @RunWith(MockitoJUnitRunner.class)</em> on JUnit 4 — on the class, at its indentation, with the imports.</td></tr>
  </tbody>
</table>
<pre><code>{@html highlightCode(`verify(repo).save(any(), 5);     →     verify(repo).save(any(), eq(5));`, 'java')}</code></pre>
<p>
  No fix is offered for <code>null</code> beside a matcher: <code>isNull()</code> is what is usually meant, and that is a choice rather than a rewrite. Nor when <code>eq</code> cannot be spelled
  so that it certainly compiles — a file declaring an <code>eq</code> of its own, for one.
</p>

<h2>How it stays quiet</h2>
<p>
  <code>when</code>, <code>verify</code>, <code>any</code> and <code>not</code> are ordinary names — a test helper, AssertJ and Hamcrest declare some of them. Every call is resolved through the
  file's imports, and a bare name reached only through an on-demand import is trusted only when no other static on-demand import could declare it too.
</p>
<Callout variant="info" title="A plain value has to be certainly plain">
  Mockito counts the matchers <em>registered while the arguments were evaluated</em>, not the ones it can see. So an argument that calls anything — <code>orderWith(id)</code>,
  <code>captor.capture()</code>, even <code>order.getId()</code> — a parameter of a helper method, or a local assigned from a call silences the whole call: any of them could be carrying a matcher.
</Callout>
<p>The uninitialised-mock warning appears only where the class says plainly that nothing initialises its mocks:</p>
<ul>
  <li>a JUnit 4 or JUnit 5 test — not TestNG — with tests of its own, so a base class run by its subclasses is left alone;</li>
  <li>no <code>extends</code> and no <code>implements</code>, on the class or around it: an interface can carry <code>@ExtendWith</code> too;</li>
  <li>no class-level annotation beyond the inert ones — <code>@DisplayName</code>, <code>@Tag</code>, <code>@Nested</code>, <code>@TestInstance</code>, <code>@Disabled</code> and the like. Any
    <code>@ExtendWith</code>, any <code>@RunWith</code>, <code>@SpringBootTest</code> or a composed annotation silences it;</li>
  <li>nothing in the file that looks like initialisation: <code>openMocks</code>, a Mockito rule or session, a <code>@RegisterExtension</code>, a call handed <code>this</code>, or the field assigned by hand.</li>
</ul>

<h2>What it will not judge</h2>
<ul>
  <li>Stubbing a <code>void</code> or <code>final</code> method, a matcher used outside a stubbing, and Mockito's other run-time complaints.</li>
  <li>A stubbing whose result is kept — <code>var s = when(…)</code> may be finished elsewhere — deep stubs, <code>lenient().when(…)</code> and <code>willReturn(…).given(…)</code>.</li>
  <li>Initialisation inherited from a superclass, or registered globally through JUnit's extension auto-detection.</li>
  <li>TestNG, and files that mix JUnit 4 and JUnit 5.</li>
  <li>A <code>when</code> inherited from a test base class: imports cannot see it.</li>
</ul>
