<script lang="ts">
  /**
   * MyBatis checks: every problem Bennu reports in a mapper XML or a mapper interface.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">MyBatis</span>
<h1>Mapper checks</h1>

<p class="doc-lead">
  MyBatis reads a mapper at start-up, or at the first call, and refuses it with a stack trace. Bennu reports the same mistakes while you type — in the mapper
  XML, on the interface, and, when a database is open, in the SQL itself.
</p>

<h2>The checks</h2>
<table>
  <thead><tr><th>Code</th><th>Level</th><th>Where</th><th>Reported when</th></tr></thead>
  <tbody>
    <tr>
      <td><code>mybatis-property-unknown</code></td><td>Error</td><td>Mapper XML</td>
      <td>A result <code>property</code> is no field, getter or setter of the class the map describes</td>
    </tr>
    <tr>
      <td><code>mybatis-ref-unresolved</code></td><td>Error</td><td>Mapper XML</td>
      <td>An <code>&lt;include refid&gt;</code>, a <code>resultMap</code>, an <code>extends</code> or an association's <code>select</code> names a declaration its namespace does not have</td>
    </tr>
    <tr>
      <td><code>mybatis-duplicate-id</code></td><td>Error</td><td>Mapper XML</td>
      <td>A statement, <code>&lt;resultMap&gt;</code> or <code>&lt;sql&gt;</code> id is declared twice in one mapper</td>
    </tr>
    <tr>
      <td><code>mybatis-type-missing</code></td><td>Warning</td><td>Mapper XML</td>
      <td>A fully-qualified type names no class, in a package the project does have</td>
    </tr>
    <tr>
      <td><code>mybatis-statement-missing</code></td><td>Error</td><td>Java interface</td>
      <td>An abstract method of a mapper interface has no statement with its name in the mapper XML</td>
    </tr>
    <tr>
      <td><code>mybatis-statement-unmapped</code></td><td>Weak</td><td>Mapper XML</td>
      <td>A statement has no method of that name on its interface</td>
    </tr>
    <tr>
      <td><em>Database: …</em></td><td>Error</td><td>Mapper XML</td>
      <td>The server rejects a statement when asked to prepare it — a connection must be open</td>
    </tr>
  </tbody>
</table>
<p>
  The first six run in the full validation pass, a moment after you stop typing; the fast pass that paints syntax errors does not run them. They are judged
  against the text in the editor, so unsaved edits count. Like every check, they appear as squiggles and in the Problems panel.
</p>

<h2>Under-reporting on purpose</h2>
<p>
  Every check here prefers silence to a guess. A hierarchy that leaves the project, a name that two classes share, a namespace nobody in the project declares —
  each is skipped, because a false error on working code costs more than a missed one.
</p>

<h3>Unknown property</h3>
<pre><code>{@html highlightCode(`<resultMap id="orderMap" type="com.acme.orders.Order">
    <result property="totl" column="total"/>   <!-- not a property of Order -->
</resultMap>`, 'markup')}</code></pre>
<ul>
  <li>The owner must be a project class: a qualified <code>type</code>, or a bare name that only one project class carries (usually an alias).</li>
  <li>The class and <strong>all of its superclasses must be in the project</strong>. A library base class may hold any property, so a chain that leaves the project proves nothing and is not checked. Enums, records, annotations and interfaces are not checked.</li>
  <li>A <code>property</code> is a property if a field, a getter or a setter has that name. For <code>address.city</code> or <code>items[0]</code> only the first segment is checked.</li>
  <li>An <code>&lt;association&gt;</code> without a <code>javaType</code>, a <code>&lt;collection&gt;</code> without an <code>ofType</code>, and anything under a <code>&lt;discriminator&gt;</code> or <code>&lt;case&gt;</code> are not checked: the class cannot be named for certain.</li>
  <li>A value written as an expression (<code>#&#123;…&#125;</code> or <code>$&#123;…&#125;</code>) is not checked.</li>
</ul>

<h3>Unresolved reference</h3>
<p>
  A bare value is looked for in this mapper first. A qualified one (<code>com.acme.OtherMapper.columns</code>) is looked for in the mapper files of that namespace.
  The check stays quiet when the project has not yet indexed its mappers, when the namespace is declared by no indexed mapper (another module, a generated
  file), and when a sibling file of the namespace cannot be read.
</p>

<h3>Duplicate id</h3>
<p>
  The second declaration is the one marked. Two statements with the same <code>id</code> are legitimate when their <code>databaseId</code> differs — one per
  vendor — so the <code>databaseId</code> is part of the identity.
</p>

<h3>Missing class</h3>
<p>
  Only a qualified name is judged, in <code>type</code>, <code>resultType</code>, <code>parameterType</code>, <code>javaType</code> and <code>ofType</code>.
  An alias (<code>Order</code>), a built-in (<code>long</code>, <code>map</code>), a library class, a <code>typeHandler</code> and a <code>&lt;cache type&gt;</code> are not.
  Nested classes written with <code>$</code> or <code>.</code> are accepted when the enclosing class exists.
</p>

<h3>Statement missing — on the interface</h3>
<pre><code>{@html highlightCode(`public interface OrderMapper {
    Order findById(long id);        // no <select id="findById"> in the XML
    default String label() { … }    // default and static methods are not mapped
    @Select("SELECT 1")
    int ping();                     // the annotation is the statement
}`, 'java')}</code></pre>
<p>
  MyBatis fails the first call of such a method with <em>Invalid bound statement (not found)</em>. The error is drawn on the method name. A method is
  answered by a statement of that id, or by one of <code>@Select</code>, <code>@Insert</code>, <code>@Update</code>, <code>@Delete</code>, their
  <code>*Provider</code> forms, or <code>@Flush</code>. It applies only to an interface that
</p>
<ul>
  <li>some indexed mapper XML declares as its <code>namespace</code> — an interface with no XML may be annotation-only, or have its XML in a module the project has not opened;</li>
  <li>extends and implements nothing — a parent interface, or a generic base mapper supplied at run time, may bring methods and statements from elsewhere.</li>
</ul>

<h3>Statement unmapped — the reverse</h3>
<p>
  A statement no method maps is only a faint hint, because a legacy codebase calls statements by name (<code>sqlSession.selectList("ns.findAll")</code>), and a
  statement reached that way is still reached. It is also what a renamed method leaves behind, which is why it is worth a quiet line. The same two conditions
  apply: the namespace is a project interface, and that interface stands alone.
</p>

<h3>Database errors</h3>
<p>
  With a connection open in the Database panel, each statement is rendered to plain SQL and prepared on the server; a rejection is drawn on the
  statement's opening tag as <em>Database: …</em>, because a position inside rendered SQL has no place in the XML. A statement that cannot be rendered into
  something preparable — a <code>$&#123;…&#125;</code> with no value, a fragment of another mapper — is left alone rather than reported. Editing one statement
  asks the server again about that one only. See <strong>Statements, parameters and result maps</strong>.
</p>

<h2>Silencing a check</h2>
<ul>
  <li>
    <strong>By kind</strong>: the project's inspection policy re-levels or turns off a check by its code, for every diagnostic alike — the six codes above included.
  </li>
  <li>
    <strong>By place, in Java</strong>: a comment on the method's line or the line above it, or <code>@SuppressWarnings</code> on the declaration.
    <pre><code>{@html highlightCode(`// bennu:ignore mybatis-statement-missing
Order findById(long id);`, 'java')}</code></pre>
  </li>
  <li>
    <strong>By place, in the mapper XML</strong>: there is no comment form. Fix the declaration, or turn the check down by kind.
  </li>
  <li><strong>Database errors</strong> are asked only while a connection is open; they are the server's verdict on the SQL, not a Bennu inspection.</li>
</ul>

<Callout variant="info" title="A splice is not reported">
  Bennu draws <code>$&#123;&#125;</code> in the warning colour, and a statement that uses one is unchecked against the database until you say what it stands for — but
  no diagnostic is raised for the splice itself. It is often right, so the decision stays visible rather than flagged.
</Callout>
