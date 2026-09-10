<!-- Bennu docs — Jakarta / Java Bean Validation: which bundle a constraint message really uses. -->
<h1>Bean Validation</h1>
<p class="doc-lead">
  A constraint's <code>message</code> is resolved against a bundle the <em>validator</em> chooses —
  which is almost never the bundle the rest of the application reads, and is sometimes named in a
  <code>@Bean</code>. Bennu finds it, and checks the keys against it.
</p>

<h2>The assumption that is wrong</h2>
<p>
  An application has <code>messages.properties</code>. A field has
  <code>@NotBlank(message = "&#123;order.name.required&#125;")</code>. It is entirely natural to
  assume the two go together. They do not.
</p>
<p>
  Bean Validation resolves a message key against <strong><code>ValidationMessages</code></strong> —
  that exact base name, at the classpath root — plus the provider's own bundle inside the jar, which
  is where the default messages of <code>@NotNull</code> and friends live. A key that exists in
  <code>messages_it.properties</code> and nowhere else is a key the validator cannot find, and what
  it does then is render the key itself, braces included, into the field the user is looking at.
</p>
<p>
  This is why constraint messages are checked separately from the rest of the message bundles. The
  general bundle tooling answers <em>does any bundle declare this key</em> — which is true, and is
  the wrong question. Here the question is whether <em>the validator</em> can find it.
</p>

<h2>Where the bundle is named</h2>
<p>
  Three ways, and Bennu looks for all three:
</p>
<ul>
  <li>
    <strong>The specification's.</strong> A <code>ValidationMessages.properties</code> on a resource
    root, in every locale it is written in. Nothing declares it — being called that <em>is</em> the
    declaration.
  </li>
  <li>
    <strong>One named in code.</strong>
    <code>new PlatformResourceBundleLocator("jakarta-validator-bundle")</code>, usually inside a
    <code>@Bean</code> that builds a <code>ResourceBundleMessageInterpolator</code>. That is a bundle
    base name written as a string literal in a method body — in no configuration file, nothing to
    grep for unless you already know — and it redirects every custom constraint message in the
    application. An <code>AggregateResourceBundleLocator</code> naming several is read the same way.
  </li>
  <li>
    <strong>Spring's.</strong> <code>LocalValidatorFactoryBean.setValidationMessageSource(…)</code>
    points the validator at the application's own <code>MessageSource</code> — the one wiring that
    makes the natural assumption above true. Bennu records it as such and checks against those
    bundles instead.
  </li>
</ul>
<p>
  <strong>A bundle named by the code that nothing implements</strong> is flagged on the line that
  names it, rather than on the hundred messages it breaks. It is worth its own warning: every custom
  message resolved through that bundle renders as its own key, all at once, and nothing else in the
  project looks wrong.
</p>

<h2>What counts as a key, and what does not</h2>
<p>
  A message is interpolated, and only one of the things that can appear in it is a bundle key:
</p>
<ul>
  <li><code>&#123;min&#125;</code> on a <code>@Size</code> — the <strong>constraint's own
    attribute</strong>. Resolved before any bundle is consulted.</li>
  <li><code>&#123;order.name.length&#125;</code> — a <strong>bundle key</strong>.</li>
  <li><code>&#36;&#123;validatedValue&#125;</code> — an <strong>expression</strong>, evaluated at
    interpolation time. Never looked up.</li>
  <li><code>\&#123;</code> — an <strong>escaped brace</strong>, printed as written.</li>
</ul>
<p>
  The distinction is not cosmetic: a check that read every brace run as a key would report
  <code>min</code> and <code>max</code> missing on every custom <code>@Size</code> message in the
  project. Bennu knows each constraint's attributes, so it reports the one that is genuinely
  unresolvable and stays quiet about the three that are not.
</p>
<p>
  Keys that resolve to the <strong>provider's own bundle</strong> —
  <code>jakarta.validation.constraints.NotNull.message</code>, and its <code>javax</code> spelling —
  are recognised as resolvable even though nothing in your project declares them. They are in the
  jar.
</p>

<h2>In the editor</h2>
<ul>
  <li>
    <strong>Completion</strong> inside a message's braces offers the keys the validator can actually
    resolve, with the text each one resolves to. Deliberately not every key in the project: offering
    one from <code>messages.properties</code> would be offering a key that resolves to nothing at
    runtime — the popup would be teaching the mistake.
  </li>
  <li><strong>Hover</strong> on a key shows what the message will say and which bundle it comes
    from; hover on the annotation itself shows what the constraint checks and which attributes its
    message may interpolate.</li>
  <li><strong>Go to declaration</strong> on a key opens the bundle line that defines it, one target
    per locale.</li>
  <li>
    <strong>A constraint the engine has no validator for</strong> — <code>@NotBlank</code> on an
    <code>int</code>, <code>@Size</code> on a <code>long</code> — is an error rather than a warning,
    because Hibernate Validator refuses it at <em>startup</em>: it is not a wrong message, it is an
    application that does not come up.
  </li>
  <li><strong>A constraint that can never fail</strong> — <code>@NotNull</code> on a primitive — is
    a warning. It reads as a rule and enforces nothing.</li>
</ul>
<p>
  Type checks are only made where the answer is certain. A project type, a type variable or a JDK
  class outside the ones Bean Validation itself names produce no diagnostic at all — a custom
  <code>ConstraintValidator</code> is invisible from here, and Bennu will not contradict one it
  cannot see.
</p>
<p>
  Nothing is said about message keys at all on a project with no validation bundle. That means the
  messages are literals or defaults, not that every key in the project is wrong.
</p>

<h2>The Constraints panel</h2>
<p>
  Every constraint the project declares, in one list: the member it is on, the message it will show,
  and its type as a tag. Group it by constraint to answer "where do we use <code>@Email</code>".
</p>
<p>
  The tag worth looking for is <strong>missing:</strong> — a row whose message names a key no
  validation bundle declares. On screen a constraint whose message does not resolve looks exactly
  like one whose message does, right up until a user trips it.
</p>
