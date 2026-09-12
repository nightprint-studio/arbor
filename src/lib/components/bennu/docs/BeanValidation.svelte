<script lang="ts">
  /**
   * Bean Validation: which bundle a constraint message really resolves against, the three ways it is named, what counts as a
   * key, the editor checks, and the Constraints panel.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>Bean Validation</h1>

<p class="doc-lead">
  A constraint's <code>message</code> is resolved against a bundle the <em>validator</em> chooses — almost never the bundle the rest of the application reads, and sometimes
  named in a <code>@Bean</code>. Bennu finds it, and checks the keys against it.
</p>

<h2>The assumption that is wrong</h2>
<pre><code>{@html highlightCode(`@NotBlank(message = "{order.name.required}")
private String name;`, 'java')}</code></pre>
<p>
  The application has <code>messages.properties</code>, the field has that message, and it is entirely natural to assume the two go together. They do not.
</p>
<p>
  Bean Validation resolves a message key against <strong><code>ValidationMessages</code></strong> — that exact base name, at the classpath root — plus the provider's own
  bundle inside its jar, where the defaults of <code>@NotNull</code> and friends live.
</p>
<Callout variant="warning" title="The key shows up on screen">
  A key that exists in <code>messages_it.properties</code> and nowhere else is one the validator cannot find, and it renders the key itself — braces included — into the field
  the user is looking at.
</Callout>
<p>
  That is why constraint messages are checked apart from the other message bundles. The bundle tooling answers <em>does any bundle declare this key</em> — true, and the wrong
  question. Here the question is whether <em>the validator</em> can find it.
</p>

<h2>Where the bundle is named</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">The specification's</div>
    <div class="fc-title"><code>ValidationMessages.properties</code></div>
    <div class="fc-desc">On a resource root, in every locale written. Nothing declares it — being called that <em>is</em> the declaration.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Named in code</div>
    <div class="fc-title">A resource bundle locator</div>
    <div class="fc-desc">A base name in a string literal in a method body — in no configuration file, nothing to grep for — redirecting every custom message. An <code>AggregateResourceBundleLocator</code> naming several is read the same way.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Spring's</div>
    <div class="fc-title"><code>setValidationMessageSource(…)</code></div>
    <div class="fc-desc">Points the validator at the application's own <code>MessageSource</code> — the one wiring that makes the natural assumption true. Recorded as such, and checked against those bundles.</div>
  </div>
</div>
<pre><code>{@html highlightCode(`@Bean
public LocalValidatorFactoryBean validator() {
    LocalValidatorFactoryBean factory = new LocalValidatorFactoryBean();
    factory.setMessageInterpolator(new ResourceBundleMessageInterpolator(
            new PlatformResourceBundleLocator("jakarta-validator-bundle")));
    return factory;
}`, 'java')}</code></pre>
<Callout variant="warning" title="A bundle nothing implements">
  A bundle the code names and no file provides is flagged on the line naming it, rather than on the hundred messages it breaks: every custom message resolved through it
  renders as its own key, all at once, and nothing else in the project looks wrong.
</Callout>

<h2>What counts as a key</h2>
<p>A message is interpolated, and only one of the things that can appear in it is a bundle key:</p>
<table>
  <thead><tr><th>Written</th><th>Is</th></tr></thead>
  <tbody>
    <tr><td><code>&#123;min&#125;</code> on a <code>@Size</code></td><td>The <strong>constraint's own attribute</strong>, resolved before any bundle</td></tr>
    <tr><td><code>&#123;order.name.length&#125;</code></td><td>A <strong>bundle key</strong></td></tr>
    <tr><td><code>&#36;&#123;validatedValue&#125;</code></td><td>An <strong>expression</strong>, evaluated at interpolation — never looked up</td></tr>
    <tr><td><code>\&#123;</code></td><td>An <strong>escaped brace</strong>, printed as written</td></tr>
  </tbody>
</table>
<p>
  Not cosmetic: reading every brace as a key would report <code>min</code> and <code>max</code> missing on every custom <code>@Size</code> message. Bennu knows each
  constraint's attributes, so it reports the one genuinely unresolvable. Keys in the <strong>provider's own bundle</strong> —
  <code>jakarta.validation.constraints.NotNull.message</code>, and its <code>javax</code> spelling — are resolvable though nothing in your project declares them: they are in the jar.
</p>

<h2>In the editor</h2>
<dl class="meta-grid">
  <dt>Completion</dt>
  <dd>Inside a message's braces, the keys the validator can resolve, with their text — deliberately not every key in the project: offering one from <code>messages.properties</code> would teach the mistake.</dd>
  <dt>Hover</dt>
  <dd>On a key, what the message will say and which bundle it comes from; on the annotation, what the constraint checks and which attributes its message may interpolate.</dd>
  <dt>Go to declaration</dt>
  <dd>On a key, the bundle line defining it, one target per locale.</dd>
</dl>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">An error</div>
    <div class="fc-title">No validator for the type</div>
    <div class="fc-desc"><code>@NotBlank</code> on an <code>int</code>, <code>@Size</code> on a <code>long</code>: Hibernate Validator refuses it at <em>startup</em> — not a wrong message, an application that does not come up.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">A warning</div>
    <div class="fc-title">A constraint that can never fail</div>
    <div class="fc-desc"><code>@NotNull</code> on a primitive reads as a rule and enforces nothing.</div>
  </div>
</div>
<Callout variant="info" title="Only where the answer is certain">
  A project type, a type variable, or a JDK class beyond those Bean Validation names produce no diagnostic: a custom <code>ConstraintValidator</code> is invisible from here,
  and Bennu will not contradict one it cannot see. On a project with no validation bundle nothing is said about keys at all — the messages are literals or defaults.
</Callout>

<h2>The Constraints panel</h2>
<p>
  Every constraint the project declares, in one list: the member it is on, the message it will show, and its type as a tag. Group by constraint to answer "where do we use
  <code>@Email</code>".
</p>
<p>
  The tag to look for is <strong>missing:</strong> — a row whose message names a key no validation bundle declares. On screen, a constraint whose message does not resolve
  looks exactly like one whose message does, until a user trips it.
</p>
