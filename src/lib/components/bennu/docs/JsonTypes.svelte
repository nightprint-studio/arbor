<!-- Bennu docs — Jackson: what a DTO actually serialises to. -->
<h1>JSON types</h1>
<p class="doc-lead">
  What a DTO will and will not put in its JSON, read from the class — instead of discovered from a
  payload that came out wrong, on the other side of an HTTP call, days later.
</p>

<h2>Jackson fails quietly</h2>
<p>
  Almost nothing here throws where you can see it. A field with no accessor is simply
  <strong>absent</strong> from the payload. A property two members both claim throws only when that
  particular object is serialised, which in practice is in front of a customer. A
  <code>@JsonIgnore</code> that meant "not on the way out" takes the property off the way
  <em>in</em> as well.
</p>
<p>
  None of it is visible in the class. What you see is a field, spelled correctly, with an annotation
  on it — and the first person to notice has no reason to suspect the DTO.
</p>

<h2>What is checked</h2>
<ul>
  <li>
    <strong>Two members claiming one JSON name</strong> — an error, because Jackson refuses to
    serialise such a type at all. A field and <em>its own</em> getter are one property, not two;
    what conflicts is two fields, or two members that both name themselves with
    <code>@JsonProperty</code>.
  </li>
  <li>
    <strong>A field nothing can reach</strong> — not public, no accessor, so it will not appear in
    the JSON.
  </li>
  <li>
    <strong>An element carrying both <code>@JsonIgnore</code> and <code>@JsonProperty</code></strong>
    — Jackson resolves it by dropping the property, which is unlikely to be what the
    <code>@JsonProperty</code> was for.
  </li>
</ul>
<p>
  Hover on a field or an accessor says what it will be called in the JSON, and why — named by
  <code>@JsonProperty</code>, named after the member, or not serialised at all.
</p>

<h2>A creator whose arguments have no names</h2>
<p>
  A <code>@JsonCreator</code> with two or more arguments is a <em>property-based</em> creator:
  Jackson matches each argument to a JSON field <strong>by name</strong>. Java does not keep
  parameter names in the class file unless the compiler is told to — <code>-parameters</code>, or
  <code>&lt;maven.compiler.parameters&gt;true&lt;/maven.compiler.parameters&gt;</code> in the pom.
  Without it the names are <code>arg0</code> and <code>arg1</code>, and deserialising fails with
  <em>"Argument #0 of constructor has no property name annotation"</em>.
</p>
<p>
  <strong>The defect lives in two files, and neither is wrong on its own.</strong> The constructor is
  ordinary. The pom is ordinary. It is the pair, and the two halves are in files nobody reads
  together — which is why it survives review and turns up the first time somebody POSTs to that
  endpoint.
</p>
<p>
  Four shapes are deliberately not reported, because all four are correct: a
  <strong>single-argument</strong> creator (Jackson reads it as delegating), an explicit
  <code>mode = Mode.DELEGATING</code>, a <strong>record</strong> (whose component names are in the
  class file regardless), and <code>@ConstructorProperties</code>, which carries the names as data.
  Nor is anything said when no pom can be read at all — a Gradle build is not evidence of a missing
  compiler flag. <code>spring-boot-starter-parent</code> sets the flag for you, and is recognised as
  such.
</p>

<h2>The gate that keeps it usable</h2>
<p>
  Jackson's visibility rules are configurable globally at runtime, per class with
  <code>@JsonAutoDetect</code>, and through mix-ins registered somewhere else entirely. So the
  reachability check runs <strong>only</strong> where the class says plainly what it is: a class that
  carries Jackson annotations, sets no visibility of its own, and generates no accessors through
  Lombok.
</p>
<p>
  That gate is the whole design. Without it the check fires on every Lombok DTO in the project —
  which is all of them — and a check that is wrong about all of them is one that gets turned off
  before it is ever right about one.
</p>
