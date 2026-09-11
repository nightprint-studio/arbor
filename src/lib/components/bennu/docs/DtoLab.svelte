<!-- Bennu docs — the DTO Lab: a class tried out as JSON, validated for real, and turned into tests. -->
<h1>DTO Lab</h1>
<p class="doc-lead">
  What a class makes of a payload, and what its constraints make of the result — answered by the
  project's own Jackson and Bean Validation, running on the project's own JDK. And, from the same
  answers, the tests that pin it down, written the way you write tests.
</p>

<h2>Opening it</h2>
<ul>
  <li><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>J</kbd> with the caret in a Java class — the innermost class around the caret, so a nested DTO is reachable too</li>
  <li><strong>Open in DTO Lab</strong> from the editor's right-click menu, on the class under the pointer</li>
  <li>The command palette — <strong>DTO Lab: try the class at the caret</strong> and <strong>DTO Lab: generate validation tests for the class at the caret</strong> — and the flask button in the right activity bar</li>
</ul>
<p>
  It is offered on projects that use Bean Validation, Jackson, or both. The lab reads the class the
  moment it opens; the refresh button in its header reads it again after you change it.
</p>

<h2>Payload</h2>
<p>
  The left column is the JSON, starting from a sketch of every property the class exposes under the
  name Jackson gives it. <strong>Use the project's JSON</strong> replaces the sketch with what the
  project's own <code>ObjectMapper</code> writes for a new instance — its modules, its naming, its
  date format. <kbd>Ctrl</kbd> + <kbd>Enter</kbd> (or <strong>Run</strong>) sends the payload, and two
  answers come back:
</p>
<ul>
  <li><strong>What it binds to.</strong> The object Jackson built, field by field, and the JSON that
    object writes back. A property you sent and the object does not write back is listed as
    <em>lost on the way back</em> — the field with no accessor, the <code>@JsonIgnore</code> that also
    stops it coming in. A payload that does not bind at all shows Jackson's own error.</li>
  <li><strong>What the validator says.</strong> Every violation, with its property path, the message
    a user would read, the template it came from and the constraint that raised it. Messages are
    interpolated in the locale typed beside the list, and with the bundle the project actually
    configures — including one named in a <code>@Bean</code> — so a key that does not resolve shows up
    as its own key, exactly as a user would see it.</li>
</ul>
<p>
  On a Spring Boot project the payload binds the way the application binds it: an unknown property
  is ignored and dates are written as text, which is what Boot changes from Jackson's own defaults.
  A project without Jackson has its payload bound by field name, with no JSON written back.
</p>

<h2>How it runs</h2>
<p>
  The answers come from the project itself, so the project is compiled first when anything changed
  since its last compile, and its classes are loaded and run — static initialisers included. The JVM
  is the one the project declares, and it is started the first time the lab needs it and stopped
  after ten minutes with no questions; a rebuild is picked up without restarting it. Classes under
  <code>src/test</code> are not on its classpath, and an unsaved class has not been compiled yet.
</p>
<p>
  A class whose static initialiser waits on something that never comes — a database, a network
  service — gets no answer within a minute; the lab says so and starts a fresh JVM for the next
  question.
</p>

<h2>Tests</h2>
<p>
  The Tests tab turns the constraints of the class into test cases: one per way each constraint can
  fail — a <code>@Size(min = 2, max = 40)</code> is too short and too long, a <code>@NotBlank</code>
  is null and blank — plus one checking that a valid instance really is valid. Choose the fields,
  the template and where the tests go, then <strong>Generate</strong>: nothing is written until you
  apply the preview.
</p>
<p>
  Every case is checked on the JVM before it is written. The valid instance is validated, then each
  case — the valid instance with one field changed — and <strong>what the validator reports is what
  the test expects</strong>, including constraints the source does not show, like a custom
  <code>@Constraint</code> or a composed one. When the JVM cannot answer, the expectations are
  predicted from the source and the preview says so. The preview also lists what needs a look: a
  valid instance that is not valid, or a value that did not violate the constraint it was chosen for.
</p>
<p>
  A new test goes beside the class's tests — <code>src/test/java</code>, same package, named
  <code><em>Class</em>ValidationTest</code>. When that file exists, or you choose another test file,
  the tests are added to it, and the preview lets you pick the class inside it, nested ones included.
  Added to a file open in a tab, they arrive in the buffer as one undo step.
</p>

<h2>Templates</h2>
<p>
  A test is written from a <strong>template</strong>, and the built-in one writes plain Bean
  Validation tests with the project's own JUnit, and AssertJ when the project has it. To write tests
  your way — your assertion helpers, your factories, your naming — create a template from it with
  <strong>New template…</strong> and change anything: it opens in the editor, and the next generation
  uses it.
</p>
<ul>
  <li>Templates are yours rather than a project's, kept in your profile, so every project sees them.</li>
  <li><strong>Use for this project</strong> makes a template the one a project generates with; a
    single generation can still pick another.</li>
  <li>The built-in template cannot be changed or replaced, so there is always one that works.</li>
</ul>
<p>
  Templates are Jinja (<code>*.java.jinja</code>). Lines holding only a <code>{'{%'} … {'%}'}</code>
  tag leave nothing behind, and four filters are available: <code>snake</code>, <code>camel</code>,
  <code>pascal</code> and <code>java_string</code>. What a template is rendered with:
</p>
<ul>
  <li><code>mode</code> — <code>"file"</code> for a new test class, <code>"members"</code> for what goes
    inside an existing one (which has its own imports, so spell types in full)</li>
  <li><code>class</code> — <code>name</code>, <code>package</code>, <code>fqn</code>, <code>binary</code>,
    <code>record</code>; <code>test_class</code> — the new test class's name</li>
  <li><code>java</code> — the language level; <code>junit</code> — <code>4</code> or <code>5</code>;
    <code>assertj</code>; <code>validation</code> — <code>jakarta.validation</code> or
    <code>javax.validation</code>; <code>verified</code> — the expectations came from the JVM</li>
  <li><code>valid</code> — one entry per field: <code>field</code>, <code>setter</code>,
    <code>constrained</code>, <code>type_simple</code>, <code>value</code>, <code>value_java</code></li>
  <li><code>fields</code> — the constrained fields: <code>index</code>, <code>name</code>,
    <code>json_name</code>, <code>type_name</code>, <code>type_simple</code>, <code>setter</code>,
    <code>getter</code>, <code>cases</code></li>
  <li><code>case</code> — <code>index</code>, <code>name</code> (<code>size_max</code>),
    <code>constraint</code>, <code>constraint_fqn</code>, <code>violated</code> (the attribute crossed),
    <code>attributes</code>, <code>value</code>, <code>value_java</code>, <code>expected</code>,
    <code>verified</code></li>
  <li><code>value</code> — the value as data: <code>kind</code> is one of <code>null</code>,
    <code>empty</code>, <code>blank</code>, <code>string_of_length</code> (with <code>length</code>),
    <code>integer</code> / <code>decimal</code> (with <code>value</code>), <code>bool</code>,
    <code>empty_collection</code>, <code>collection_of_size</code> (with <code>size</code>),
    <code>past_date</code>, <code>future_date</code>, <code>email</code> (with <code>valid</code>),
    <code>text</code>, <code>non_matching</code> (with <code>regexp</code>) and <code>unknown</code> —
    the kind a constraint Bennu does not know gets, for the template to fill in by constraint name.
    <code>value_java</code> is the same value already spelled in Java.</li>
  <li><code>expected</code> — each violation: <code>path</code>, <code>template</code>,
    <code>message</code>, <code>constraint</code>, <code>attributes</code>, <code>constant</code></li>
</ul>
<p>
  A line reading <code>bennu.constants: com.example.Messages</code> anywhere in a template — normally
  in a comment — asks Bennu to look through those classes for the <code>public static final</code>
  string holding each expected template or message, and hand it over as
  <code>expected.constant</code>. That is how a template writes <code>Messages.REQUIRED</code> instead
  of the text the constant holds.
</p>

<h2>For an AI client</h2>
<p>
  The same check is offered to an AI client connected to Arbor as <code>bennu_validate_payload</code>:
  a payload, a class, and back come the binding and every violation. It compiles and runs project
  code, so it asks for permission like any tool that changes something.
</p>
