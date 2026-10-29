<script lang="ts">
  /**
   * MyBatis statements: parameters, result maps, fragments, and the database check.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">MyBatis</span>
<h1>Statements, parameters and result maps</h1>

<p class="doc-lead">
  A statement is SQL with holes in it. Bennu draws the holes apart from the SQL, links every name in a mapper to its declaration, and — with a database open —
  asks the server whether the SQL is valid.
</p>

<h2>Parameters: <code>#&#123;&#125;</code> and <code>$&#123;&#125;</code></h2>
<pre><code>{@html highlightCode(`<select id="findByStatus" resultMap="orderMap">
    SELECT * FROM \${table}
    WHERE status = #{status}
      AND customer LIKE '%\${customer}%'
</select>`, 'markup')}</code></pre>
<table>
  <thead><tr><th>Written</th><th>Meaning</th><th>Drawn</th></tr></thead>
  <tbody>
    <tr><td><code>#&#123;status&#125;</code></td><td>A bound parameter: the value travels separately from the SQL</td><td>As a parameter</td></tr>
    <tr><td><code>$&#123;table&#125;</code></td><td>Text pasted into the statement before it is prepared</td><td>In the warning colour, bold — also inside a string literal such as <code>'%$&#123;customer&#125;%'</code></td></tr>
  </tbody>
</table>
<Callout variant="warning" title="A splice is not a binding">
  Anything reaching <code>$&#123;&#125;</code> from outside becomes part of the SQL itself. It is the right tool for a table or a column chosen by the caller and
  the wrong one for a value. Bennu draws it loudly for that reason; it does not report it as a problem — see <strong>Mapper checks</strong>.
</Callout>

<h2>Result maps</h2>
<pre><code>{@html highlightCode(`<resultMap id="orderMap" type="com.acme.orders.Order">
    <id property="id" column="id"/>
    <result property="customer" column="customer"/>
    <association property="owner" javaType="com.acme.User" resultMap="userMap"/>
    <collection property="lines" ofType="com.acme.Line" select="findLines" column="id"/>
</resultMap>`, 'markup')}</code></pre>
<p>
  The <code>property</code> is the Java side and is coloured as a member; the <code>column</code> stays plain. The class a <code>property</code> belongs to is the
  <strong>nearest</strong> enclosing container: the <code>&lt;resultMap type&gt;</code>, an <code>&lt;association javaType&gt;</code>, or a
  <code>&lt;collection ofType&gt;</code>. <kbd>Ctrl</kbd> + <kbd>B</kbd> on a <code>property</code> lands on the field, else the setter or getter.
</p>
<p>
  A property path such as <code>address.city</code> is judged by its first segment. A <code>&lt;discriminator&gt;</code> or <code>&lt;case&gt;</code> has a result
  type of its own, and an <code>&lt;association&gt;</code> without a <code>javaType</code> infers it from the owner — so the properties nested in them are never
  judged against the outer class.
</p>

<h2>Fragments</h2>
<pre><code>{@html highlightCode(`<sql id="columns">\${alias}.id, \${alias}.customer</sql>

<select id="findAll" resultMap="orderMap">
    SELECT <include refid="columns"><property name="alias" value="o"/></include>
    FROM orders o
</select>`, 'markup')}</code></pre>
<p>
  <code>&lt;sql id&gt;</code> and <code>&lt;resultMap id&gt;</code> are declarations, drawn in bold; <code>refid</code>, <code>resultMap</code>, <code>select</code> and
  <code>extends</code> are references — what <kbd>Ctrl</kbd> + <kbd>B</kbd> follows. <kbd>Alt</kbd> + <kbd>F7</kbd> on either lists every reference
  across the project's mappers. A reference with a dot is read as <code>namespace.id</code>; a bare one belongs to this mapper's namespace, and may be declared in
  a sibling file of the same namespace.
</p>
<p>
  The attributes that name a Java type — <code>parameterType</code>, <code>resultType</code>, <code>type</code>, <code>javaType</code>, <code>ofType</code>,
  <code>typeHandler</code> and the mapper <code>namespace</code> — are coloured as types and followed to the class, including the class of a type handler.
</p>

<h2>Completion in a mapper</h2>
<p>
  Inside the text of a statement — between its tags, not in an attribute, not inside a <code>#&#123;…&#125;</code> or <code>$&#123;…&#125;</code> — the
  editor completes <strong>tables</strong> after <code>FROM</code>, <code>JOIN</code>, <code>INTO</code>, <code>UPDATE</code> and <code>TABLE</code>,
  and <strong>columns</strong> after <code>alias.</code> or, elsewhere, those of the tables the statement already names. Dynamic tags are looked through: what
  they wrap is still SQL, and the statement is the nearest enclosing <code>select</code>, <code>insert</code>, <code>update</code>, <code>delete</code> or
  <code>sql</code>.
</p>
<p>
  The names come from the database connection chosen in the Database panel, so this works <strong>only while that connection is open</strong>. Without one — or
  anywhere outside a statement's text — the caret gets the ordinary XML completion.
</p>

<h2>Checking statements against the database</h2>
<p>
  With a connection open, the Database panel has a <strong>MyBatis</strong> tab (it appears only when the project has mapper statements) listing every statement.
  Each is rendered to plain SQL and <em>prepared</em> on the server, so a missing table or column, or a join on nothing, comes back as the server's own message.
  <strong>Validate all</strong> runs the lot; the check button on a row runs one, and the row name opens the statement.
</p>
<p>The same check runs in the mapper editor while you type: the server's message is a red squiggle on the statement's opening tag, prefixed <em>Database:</em>.</p>
<h3>How a statement is rendered</h3>
<p>A database understands none of MyBatis's templating, so the rendering is one possible expansion of the statement — an approximation, and the panel says so:</p>
<table>
  <thead><tr><th>In the mapper</th><th>Becomes</th></tr></thead>
  <tbody>
    <tr><td><code>#&#123;…&#125;</code></td><td><code>NULL</code> — any type accepts it when a statement is only being prepared</td></tr>
    <tr><td>Dynamic tags</td><td>Expanded with every branch kept; see <strong>Dynamic SQL</strong>. A rejected dynamic statement is annotated <em>Checked with every dynamic branch included</em></td></tr>
    <tr><td><code>&lt;include&gt;</code> of a fragment of the same mapper</td><td>The fragment, with the include's <code>&lt;property&gt;</code> values filled in</td></tr>
    <tr><td><code>$&#123;name&#125;</code> with a value you gave</td><td>That value, as text</td></tr>
    <tr><td><code>$&#123;name&#125;</code> with no value</td><td>The statement is <strong>unchecked</strong>, naming the splice that needs a value</td></tr>
    <tr><td><code>&lt;include&gt;</code> of a fragment of another mapper, or of a fragment that does not exist</td><td>The statement is <strong>unchecked</strong></td></tr>
  </tbody>
</table>
<p>
  An unchecked statement says why instead of showing a clean result it did not earn, and the editor shows nothing for it. A statement with a
  <code>databaseId</code> is checked only against a connection of the matching engine.
</p>
<h3>Giving <code>$&#123;&#125;</code> a value</h3>
<p>
  Above the list, the tab collects every <code>$&#123;name&#125;</code> the mappers splice, once each, with a field to say what it stands for — a table name,
  a column list. A value is stored <strong>per project</strong> and every statement using that name is rendered again with it.
</p>
<Callout variant="info" title="Prepared, not run">
  The server is asked to prepare the rendered SQL, which parses and resolves its names. The statement is not executed.
</Callout>
