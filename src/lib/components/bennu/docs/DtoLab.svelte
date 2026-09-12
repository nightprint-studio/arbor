<script lang="ts">
  /**
   * DTO Lab: a class tried out as JSON on the project's own JVM — the payload and its two answers, how it runs, the tests
   * it writes and the values it fills in, and the template those tests come from. The template context is listed field by
   * field here: keep it in step with `bennu-dtolab`'s context.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>DTO Lab</h1>

<p class="doc-lead">
  What a class makes of a payload, and what its constraints make of the result — answered by the project's own Jackson and Bean Validation, running on the project's own
  JDK. And, from the same answers, the tests that pin it down, written the way you write tests.
</p>

<h2>Opening it</h2>
<table>
  <thead><tr><th>From</th><th>Opens on</th></tr></thead>
  <tbody>
    <tr><td><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>J</kbd></td><td>The innermost class around the caret — a nested DTO too</td></tr>
    <tr><td><strong>Open in DTO Lab</strong>, in the editor's right-click menu</td><td>The class under the pointer</td></tr>
    <tr><td>The command palette — <strong>DTO Lab: try the class at the caret</strong>, <strong>DTO Lab: generate validation tests for the class at the caret</strong></td><td>The class at the caret</td></tr>
    <tr><td>The flask button in the right activity bar</td><td>The lab</td></tr>
  </tbody>
</table>
<p>
  It is offered on projects using Bean Validation, Jackson, or both. The lab reads the class as it opens, and the refresh button in its header reads it again after you change it.
</p>

<h2>Payload</h2>
<p>
  The left column is the JSON, starting from a sketch of every property the class exposes under the name Jackson gives it — with a test value where one answers the field.
  <strong>Use the project's JSON</strong> replaces the sketch with what the project's own <code>ObjectMapper</code> writes for a new instance: its modules, its naming, its date
  format. <kbd>Ctrl</kbd> + <kbd>Enter</kbd>, or <strong>Run</strong>, sends it, and two answers come back:
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Jackson</div>
    <div class="fc-title">What it binds to</div>
    <div class="fc-desc">The object built, field by field, and the JSON it writes back. A property sent and not written back is <em>lost on the way back</em> — the field with no accessor, the <code>@JsonIgnore</code> that also stops it coming in. A payload that does not bind shows Jackson's error.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Bean Validation</div>
    <div class="fc-title">What the validator says</div>
    <div class="fc-desc">Every violation: its path, the message a user reads, its template and its constraint. Messages are interpolated in the locale typed beside the list, with the bundle the project really configures — one named in a <code>@Bean</code> included — so an unresolved key shows as itself, as a user would see it.</div>
  </div>
</div>
<Callout variant="info" title="Bound the way the application binds">
  On a Spring Boot project an unknown property is ignored and dates are written as text — what Boot changes from Jackson's defaults. A project without Jackson has its payload
  bound by field name, with no JSON written back.
</Callout>

<h2>How it runs</h2>
<p>
  The answers come from the project itself, so it is compiled first when anything changed since its last compile, and its classes are loaded and run — static initialisers
  included.
</p>
<dl class="meta-grid">
  <dt>The JVM</dt>
  <dd>The one the project declares, started the first time the lab needs it and stopped after a while without questions — ten minutes by default, under <strong>Settings ▸ Java ▸ DTO Lab</strong>. A rebuild is picked up without a restart.</dd>
  <dt>The classpath</dt>
  <dd>Classes under <code>src/test</code> are not on it, and an unsaved class has not been compiled yet.</dd>
  <dt>A class that hangs</dt>
  <dd>One whose static initialiser waits on a database or a network service gets no answer within a minute; the lab says so and starts a fresh JVM for the next question.</dd>
</dl>

<h2>Tests</h2>
<p>
  The Tests tab turns the class's constraints into test cases — one per way each constraint can fail, plus one checking that a valid instance really is valid:
</p>
<table>
  <thead><tr><th>Constraint</th><th>Cases</th></tr></thead>
  <tbody>
    <tr><td><code>@Size(min = 2, max = 40)</code></td><td>too short, too long</td></tr>
    <tr><td><code>@NotBlank</code></td><td>null, blank</td></tr>
  </tbody>
</table>
<ol class="step-list">
  <li>Choose the fields, the template, and where the tests go.</li>
  <li><strong>Generate</strong>. Every case is checked on the JVM first: the valid instance is validated, then each case — the valid instance with one field changed — and
    <strong>what the validator reports is what the test expects</strong>, including constraints the source does not show, like a custom or composed <code>@Constraint</code>.</li>
  <li>Read the preview. It lists what needs a look: a valid instance that is not valid, or a value that did not violate the constraint it was chosen for. When the JVM could
    not answer, expectations are predicted from the source and the preview says so.</li>
  <li>Apply. Nothing is written before.</li>
</ol>
<p>
  A new test goes beside the class's tests — <code>src/test/java</code>, same package, <code><em>Class</em>ValidationTest</code>. When that file exists, or you choose another,
  the tests are added to it, and the preview lets you pick the class inside, nested ones included. Added to a file open in a tab, they arrive in the buffer as one undo step,
  with the imports they need.
</p>

<h2>Test values</h2>
<p>
  A constraint says what a value must not be, and rarely what it is. So wherever a rule answers a field — for its <strong>name</strong> or for a <strong>constraint</strong> it
  carries — the field gets that rule's value, in the valid instance every test starts from and in the payload sketch. Every other field keeps the value computed from its constraints.
</p>
<table>
  <thead><tr><th>Field</th><th>Gets</th></tr></thead>
  <tbody>
    <tr><td><code>email</code>, <code>billingEmail</code></td><td><code>mario.rossi@example.com</code></td></tr>
    <tr><td><code>codiceFiscale</code>, <code>codice_fiscale</code>, <code>CODICE_FISCALE</code></td><td>A real tax code</td></tr>
    <tr><td>One carrying <code>@Iban</code></td><td>A real IBAN</td></tr>
  </tbody>
</table>
<ul>
  <li>Names match ignoring case, <code>_</code> and <code>-</code>, and a <code>*</code> at either end matches any prefix or suffix: <code>*email</code> is also
    <code>billingEmail</code>. Constraints match by simple name.</li>
  <li>A rule answering a constraint comes before one answering a name; otherwise the first rule wins, and yours come before the built-ins.</li>
  <li>A value that does not fit the field is passed over — text for a number, too long for its <code>@Size</code>, outside its <code>@Min</code> / <code>@Max</code>. What cannot be
    read from source, a <code>@Pattern</code> or a custom validator, the JVM check of the valid instance reports.</li>
  <li>Values are fixed, so a test comes out the same every time. A rule can give a <strong>Java expression</strong> to write instead — your own factory — while its value is
    still what the JVM checks.</li>
  <li>A rule naming a constraint can give an <strong>invalid value</strong> too: the case written for a constraint Bennu does not know, instead of a <code>TODO</code>.</li>
</ul>
<p>
  Rules ship for the common Italian and English fields: email and PEC, phone, fax, tax code, VAT number, IBAN, BIC, card number, postcode, URL, UUID, IP address, ISBN, username,
  password, first, last and full name, company, address, city, province, country, currency, language, gender, age and number plate. Each can be switched off in
  <strong>Settings › Test Values</strong>, where your own are added, edited and ordered; one named like a built-in replaces it. They live in your profile, so every project uses them.
</p>

<h2>Templates</h2>
<p>
  A test is written from a <strong>template</strong>. The built-in one writes plain Bean Validation tests with the project's own JUnit, and AssertJ when the project has it. To
  write tests your way — your assertion helpers, your factories, your naming — create a template from it with <strong>New template…</strong> and change anything: it opens in the
  editor, and the next generation uses it.
</p>
<dl class="meta-grid">
  <dt>Where they live</dt>
  <dd>In your profile under <code>bennu/templates/validation-tests/</code>, so every project sees them — listed with every other kind in <strong>Settings › Code Templates</strong>. See <strong>Code templates</strong>.</dd>
  <dt>Use for this project</dt>
  <dd>Makes a template the one a project generates with; a single generation can still pick another.</dd>
  <dt>The built-in</dt>
  <dd>Cannot be changed or replaced, so there is always one that works.</dd>
</dl>
<p>
  Templates are Jinja (<code>*.java.jinja</code>). A line holding only a <code>{'{%'} … {'%}'}</code> tag leaves nothing behind, and every template also reads
  <code>project</code>, <code>style</code> and <code>naming</code> — see <strong>Template reference</strong>. A case of the test, in a template that follows your style:
</p>
<pre><code>{@html highlightCode(`{% for field in fields %}
{% for case in field.cases %}
    @Test
    void {{ naming.method(field.name, case.name) }}() {
        {{ style.local(class.name) }} instance = new {{ class.name }}({{ valid | arguments(field.name, case.value_java) }});
        …
    }
{% endfor %}
{% endfor %}`, 'jinja-java')}</code></pre>
<p>
  Bennu adds six filters to Jinja's: <code>snake</code>, <code>camel</code>, <code>pascal</code>, <code>java_string</code>, <code>imported</code> and <code>arguments</code> — the
  last written for these templates: <code>valid | arguments</code> is the valid instance's constructor arguments, and <code>arguments(field.name, case.value_java)</code> the same
  with this case's value in its field.
</p>

<h3>What a template is rendered with</h3>
<ul class="prop-list">
  <li><code>mode</code><code>"file"</code> for a new test class, <code>"members"</code> for what goes inside an existing one — which has its own imports, so spell types in full or put them through <code>imported</code></li>
  <li><code>class</code><code>name</code>, <code>package</code>, <code>fqn</code>, <code>binary</code>, <code>record</code>; <code>test_class</code> is the new test class's name</li>
  <li><code>java</code>, <code>junit</code>, <code>assertj</code>the language level; <code>4</code> or <code>5</code>; whether AssertJ is there</li>
  <li><code>validation</code>, <code>verified</code><code>jakarta.validation</code> or <code>javax.validation</code>; whether the expectations came from the JVM</li>
  <li><code>valid</code>one entry per field: <code>field</code>, <code>setter</code>, <code>setter_chains</code> (the setter returns the object), <code>wither</code> (its <code>withName</code>, when there is one), <code>constrained</code>, <code>type_simple</code>, <code>value</code>, <code>value_java</code></li>
  <li><code>fields</code>the constrained fields: <code>index</code>, <code>name</code>, <code>json_name</code>, <code>type_name</code>, <code>type_simple</code>, <code>setter</code>, <code>setter_chains</code>, <code>getter</code>, <code>wither</code>, <code>cases</code></li>
  <li><code>case</code><code>index</code>, <code>name</code> (<code>size_max</code>), <code>constraint</code>, <code>constraint_fqn</code>, <code>violated</code> (the attribute crossed), <code>attributes</code>, <code>value</code>, <code>value_java</code>, <code>expected</code>, <code>verified</code></li>
  <li><code>expected</code>each violation: <code>path</code>, <code>template</code>, <code>message</code>, <code>constraint</code>, <code>attributes</code>, <code>constant</code></li>
</ul>
<p>
  <code>value</code> is the value as data. Its <code>kind</code> is one of <code>null</code>, <code>empty</code>, <code>blank</code>, <code>string_of_length</code> (with
  <code>length</code>), <code>integer</code> / <code>decimal</code> (with <code>value</code>), <code>bool</code>, <code>empty_collection</code>, <code>collection_of_size</code> (with
  <code>size</code>), <code>past_date</code>, <code>future_date</code>, <code>email</code> (with <code>valid</code>), <code>text</code>, <code>non_matching</code> (with
  <code>regexp</code>), <code>named</code> (a test value, with <code>name</code>, <code>value</code> and <code>java</code>) and <code>unknown</code> — the kind a constraint Bennu does
  not know gets, for the template to fill in by constraint name. <code>value_java</code> is the same value already spelled in Java.
</p>
<p>
  Opened in the editor, a template is coloured as Java with its Jinja tags on top, and it completes inside them: the data above — following the template's own loops, so inside
  <code>{'{%'} for case in field.cases {'%}'}</code> typing <code>case.</code> offers a case's fields — the filters after <code>|</code>, the tests after <code>is</code>, the tag
  names after <code>{'{%'}</code>, and the kinds a value can have after <code>value.kind == "</code>. Hovering any of them says what it is. The eye button in its toolbar previews
  the tests it writes for a class, beside the source, as you type.
</p>
<Callout variant="tip" title="Expected messages as constants">
  A comment line <code>bennu.constants: com.example.Messages</code> asks Bennu to look through those classes for the <code>public static final</code> string holding each expected
  template or message, and hand it over as <code>expected.constant</code> — how a template writes <code>Messages.REQUIRED</code> instead of the text the constant holds.
</Callout>

<h2>For an AI client</h2>
<p>
  The same check is offered to an AI client connected to Arbor as <code>bennu_validate_payload</code>: a payload and a class in, the binding and every violation back. It compiles
  and runs project code, so it asks for permission like any tool that changes something.
</p>
