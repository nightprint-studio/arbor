<!-- Bennu docs — completion: what the editor offers as you type, and where it gets it. -->
<h1>Completion</h1>
<p class="doc-lead">
  What the popup knows, where each suggestion comes from, and the members that exist without
  being written anywhere.
</p>

<h2>Completions</h2>
<p>
  Typing <code>.</code> after an expression offers member completions; press
  <kbd>Ctrl</kbd> + <kbd>Space</kbd> to request them explicitly. In Java, completions come from the
  project index and appear once it is warm. Edits re-index in the background as you type, so
  completion and go-to-definition track your changes without reopening the project. (In
  <code>.dig</code> they are answered locally — see <em>geode <code>.dig</code> scripts</em> above.)
</p>
<p>
  Typing a <strong>capitalised name</strong> (not after a dot) offers <strong>type-name
  completion</strong> — every class matching the prefix across the JDK, your dependencies and your
  project, with its package shown alongside (and a <em>(+N more)</em> hint when several packages
  declare the same simple name). Accepting one whose name maps to a <strong>single</strong> class
  also <strong>adds its import</strong> automatically (turn this off with Settings → Completion →
  <em>Auto-import on accept</em>). When the name is ambiguous — several packages — only the name is
  inserted; press <kbd>Alt</kbd> + <kbd>Enter</kbd> → <strong>Import '…'</strong> to pick the package.
</p>
<p>
  An <strong>overloaded</strong> method is offered <em>once per signature</em> — each entry showing
  its own parameters and return type — while a method that merely <strong>overrides</strong> an
  inherited one appears once. Inherited members are included; a <code>private</code> member of
  another class is not.
</p>
<h2>Generated members</h2>
<p>
  Plenty of Java members exist at compile time and nowhere in the source. Bennu models them, so
  completion, hover, find-usages and the checks treat them like any declaration:
</p>
<ul>
  <li><strong>Records</strong> — an accessor per component (<code>p.x()</code>, named after the
    component, not <code>getX()</code>), the backing fields, the canonical constructor, and
    <code>toString</code> / <code>equals</code> / <code>hashCode</code>. A member the record writes
    itself always wins.</li>
  <li><strong>Lombok</strong> — <code>@Getter</code> / <code>@Setter</code> / <code>@Data</code> /
    <code>@Value</code> accessors, <code>@With</code> copy-methods, the
    <code>@Slf4j</code> <code>log</code> field, the
    constructor <code>@AllArgsConstructor</code> / <code>@RequiredArgsConstructor</code> generates
    (including on an enum with valued constants), and <code>@UtilityClass</code> — which makes every
    member <code>static</code> and the class <code>final</code>.</li>
</ul>
<p>
  Lombok's members are honoured only when the file actually <strong>imports</strong> Lombok, since
  that is what makes the annotation mean anything — your own <code>@Data</code> in another package
  generates nothing. A record's members need no such gate: they come from the language.
</p>
<p>
  <code>@Accessors</code> is honoured too, at class or field level: <code>fluent = true</code> names
  both accessors after the field (<code>o.customer()</code> reads, <code>o.customer("x")</code>
  writes), and <code>chain = true</code> — which <code>fluent</code> turns on by itself — makes the
  setter return the object so calls chain. A field's own <code>@Accessors</code> overrides the
  class's. The <code>prefix</code> element is not read yet.
</p>
<p>
  <code>AccessLevel</code> is honoured on <code>@Getter</code> / <code>@Setter</code>, at class or
  field level: <code>@Setter(AccessLevel.PACKAGE)</code> generates a package-private setter and is
  treated as one, and <code>AccessLevel.NONE</code> generates nothing at all — so no accessor is
  offered for a field that has switched it off. The generated <strong>constructors</strong> carry
  their <code>access = AccessLevel.…</code> the same way, and take the parameters Lombok actually
  gives them — <code>@RequiredArgsConstructor</code> takes the <code>final</code> fields that aren't
  already assigned, plus the <code>@NonNull</code> ones.
</p>
<p>
  A primitive <code>boolean</code> field whose name <em>already</em> begins with <code>is</code> keeps
  it rather than getting a second one, exactly as Lombok does: <code>isRunning</code> gives
  <code>isRunning()</code> and <code>setRunning(…)</code>, and <code>is_attivo</code> gives
  <code>is_attivo()</code>. The rule applies whenever what follows <code>is</code> is not a lowercase
  letter, so a field named <code>isattivo</code> does get the prefix (<code>isIsattivo()</code>). A
  <code>Boolean</code> wrapper is a plain <code>getX</code>.
</p>
<p>
  One limitation, and it is deliberate: <strong>go-to on a generated member</strong> has nothing to
  open, since there is no name in the source to jump to. Go-to on the backing <em>field</em> (or a
  record's component) works.
</p>
