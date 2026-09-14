<script lang="ts">
  /**
   * AssertJ: the assertion that asserts nothing, soft assertions nobody reports, dedicated assertions, the JUnit → AssertJ
   * rewrites on Alt+Enter, and what is deliberately not judged.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Build, run &amp; test</span>
<h1>AssertJ</h1>

<p class="doc-lead">
  The assertions that cannot fail — found in the test while you write it, rather than on the day the bug they were meant to catch reaches production with a green build.
</p>

<h2>The assertion that asserts nothing</h2>
<p>
  <code>assertThat(x)</code> builds an assertion <em>object</em>. Nothing is compared until a check such as <code>isEqualTo</code> is called on it, and the methods that only
  configure it — <code>as</code>, <code>describedAs</code>, <code>extracting</code>, <code>filteredOn</code>, <code>usingComparator</code> — are not checks.
</p>
<pre><code>{@html highlightCode(`assertThat(order.total());                   // checks nothing
assertThat(order.total()).as("the total");    // still nothing
assertThat(order.total()).isEqualTo(3);       // an assertion`, 'java')}</code></pre>
<p>
  A statement that stops before a check is a warning: it compiles, runs and passes whatever the value is. The same goes for <code>assertThatObject</code>,
  <code>assertThatCode</code>, the BDD <code>then(…)</code>, and a soft-assertions object's <code>softly.assertThat(…)</code>.
  <code>assertThatThrownBy(…)</code> on its own is fine — it asserts that something was thrown.
</p>

<h2>Soft assertions nobody reports</h2>
<p>
  A <code>SoftAssertions</code> object does not throw: it collects, and throws everything at once when <code>assertAll()</code> is called. Forget that call and every failure
  is collected into nothing.
</p>
<pre><code>{@html highlightCode(`SoftAssertions softly = new SoftAssertions();
softly.assertThat(order.total()).isEqualTo(3);
softly.assertThat(order.lines()).hasSize(2);
// no softly.assertAll() — this test cannot fail`, 'java')}</code></pre>
<p>
  The variable is flagged, and <kbd>Alt</kbd> + <kbd>Enter</kbd> on it offers <strong>Add softly.assertAll()</strong> as the method's last statement. The fix is offered only
  where the end of the method is certainly the end of the test: no <code>return</code> in it, and no lambda or anonymous class using the variable.
</p>
<Callout variant="info" title="Only the shape nobody else can report">
  The moment the variable goes anywhere — passed to a helper, returned, assigned — whoever receives it may call <code>assertAll()</code>, and nothing is said.
  <code>AutoCloseableSoftAssertions</code> in a try-with-resources, <code>SoftAssertions.assertSoftly(…)</code> and the JUnit 5 extension report on their own and are never flagged.
</Callout>

<h2>Dedicated assertions</h2>
<p>A boolean assertion throws away the value it was computed from — the one thing you need when it fails in CI:</p>
<table>
  <thead><tr><th>Written</th><th>Fails with</th><th>Alt+Enter rewrites to</th></tr></thead>
  <tbody>
    <tr><td><code>assertThat(names.isEmpty()).isTrue()</code></td><td><em>expected true</em></td><td><code>assertThat(names).isEmpty()</code></td></tr>
    <tr><td><code>assertThat(names.size()).isEqualTo(2)</code></td><td><em>expected 2 but was 3</em></td><td><code>assertThat(names).hasSize(2)</code></td></tr>
    <tr><td><code>assertThat(a.equals(b)).isTrue()</code></td><td><em>expected true</em></td><td><code>assertThat(a).isEqualTo(b)</code></td></tr>
    <tr><td><code>assertThat(x == null).isFalse()</code></td><td><em>expected false</em></td><td><code>assertThat(x).isNotNull()</code></td></tr>
    <tr><td><code>assertThat(x instanceof Order).isTrue()</code></td><td><em>expected true</em></td><td><code>assertThat(x).isInstanceOf(Order.class)</code></td></tr>
  </tbody>
</table>
<p>
  Also <code>contains</code> / <code>doesNotContain</code>, <code>startsWith</code>, <code>endsWith</code>, <code>isPresent</code>, and <code>hasSize</code> from a string's
  <code>length()</code> or an array's <code>length</code>. The finding is a faint style note, and the rewrite is offered anywhere in the statement even when those notes are hidden.
</p>
<p>
  The rewrites that depend on a type — <code>isEmpty</code>, <code>hasSize</code>, <code>contains</code> — are offered only when the variable's type is <strong>written</strong>
  in the file as a JDK string, collection, map, <code>Optional</code> or array. On anything else there may be no such assertion, and the edit would not compile.
</p>

<h2>From JUnit to AssertJ</h2>
<p>
  With the caret in a JUnit 4 or JUnit 5 assertion, <kbd>Alt</kbd> + <kbd>Enter</kbd> offers <strong>Replace with AssertJ</strong>. In a file with two or more,
  <strong>Replace every JUnit assertion in this file with AssertJ</strong> does them all in one edit.
</p>
<pre><code>{@html highlightCode(`assertEquals("the total", 3, order.total());   // JUnit 4: message first
assertEquals(3, order.total(), "the total");   // JUnit 5: message last
// both become
assertThat(order.total()).as("the total").isEqualTo(3);`, 'java')}</code></pre>
<table>
  <thead><tr><th>JUnit</th><th>AssertJ</th></tr></thead>
  <tbody>
    <tr><td><code>assertEquals</code> / <code>assertNotEquals</code></td><td><code>isEqualTo</code> / <code>isNotEqualTo</code></td></tr>
    <tr><td><code>assertSame</code> / <code>assertNotSame</code></td><td><code>isSameAs</code> / <code>isNotSameAs</code></td></tr>
    <tr><td><code>assertTrue</code> / <code>assertFalse</code></td><td><code>isTrue</code> / <code>isFalse</code></td></tr>
    <tr><td><code>assertNull</code> / <code>assertNotNull</code></td><td><code>isNull</code> / <code>isNotNull</code></td></tr>
    <tr><td><code>assertArrayEquals</code></td><td><code>containsExactly</code></td></tr>
  </tbody>
</table>
<p>
  The static import of AssertJ's <code>assertThat</code> is added with the edit when the file does not have it. The JUnit imports are left where they are.
</p>
<Callout variant="warning" title="Not offered where the result would not be the same assertion">
  A delta overload (<code>assertEquals(1.0, ratio, 0.01)</code>), a <code>Supplier</code> message (<code>() -&gt; "…"</code>), a JUnit 4 <code>assertTrue(label, flag)</code>
  whose first argument is not a literal, and <code>assertArrayEquals</code> over arrays whose types are not visibly the same. Nor in a file where a bare
  <code>assertThat</code> already belongs to Hamcrest or a helper — the import would take the name away from it.
</Callout>

<h2>What it will not judge</h2>
<ul>
  <li>Every call is resolved through the file's imports, never matched by name: Hamcrest's <code>assertThat(x, is(1))</code>, JUnit 4's, and a project's own helper are not
    AssertJ's, and are left alone. A class implementing <code>WithAssertions</code> is recognised.</li>
  <li><code>assertThat(x instanceof T).isFalse()</code> is not rewritten: <code>isNotInstanceOf</code> fails on <code>null</code>, where the original holds.</li>
  <li>A chain configured further than Bennu knows — <code>usingRecursiveComparison().ignoringFields(…)</code> with nothing after — is read as a check. That misses one, and
    never reports a real assertion as broken.</li>
  <li>A test class extending JUnit 3's <code>TestCase</code> inherits its own <code>assertEquals</code>, so nothing in it is converted.</li>
</ul>
