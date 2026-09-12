<script lang="ts">
  /**
   * JPA: generated sources, derived query names checked against entities, @Query, the gutter, and the generators the toolbar
   * offers on entities and repositories.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>JPA</h1>

<p class="doc-lead">
  Entities, their mapping and the queries written against them — resolved together, so a field name in a query is a reference rather than a string.
</p>
<p>
  On a project with JPA or Spring Data on its classpath — and only there, like every framework tool here.
</p>

<h2>Generated sources are part of the project</h2>
<p>
  The static metamodel Criteria queries are written against — <code>Order_</code>, <code>Customer_</code> — is written by an annotation processor into
  <code>target/generated-sources</code>, and so are MapStruct's <code>*MapperImpl</code>, QueryDSL's <code>QOrder</code> and jOOQ's output. None of it is under
  <code>src/</code>, all of it is referenced from there, so Bennu indexes those generated roots.
</p>
<Callout variant="tip" title="Build once and they resolve">
  Nothing else under <code>target/</code> is indexed — it holds build output, and sometimes an unpacked copy of somebody else's sources.
</Callout>

<h2>Derived query names are checked</h2>
<pre><code>{@html highlightCode(`List<Order> findByCustomerNameAndTotalGreaterThan(String name, BigDecimal total);`, 'java')}</code></pre>
<p>
  That is not a name but a query Spring Data compiles at <em>application start</em>. A typo in one is invisible to the compiler and to every test not touching that
  repository — and then it takes the context down on deploy. So every segment is resolved against the entity:
</p>
<table>
  <thead><tr><th>Segment</th><th>Resolved as</th></tr></thead>
  <tbody>
    <tr><td><code>CustomerName</code></td><td><code>customer.name</code> — relations are followed</td></tr>
    <tr><td><code>TotalGreaterThan</code></td><td><code>total</code>, compared with one argument</td></tr>
  </tbody>
</table>
<ul>
  <li>A segment addressing nothing is flagged where you wrote it.</li>
  <li>The argument count is checked too: <code>Between</code> wants two, <code>IsNull</code> none, and a <code>Pageable</code> is Spring's, not yours.</li>
</ul>
<Callout variant="info" title="Quiet rather than guessing">
  An entity whose <code>@MappedSuperclass</code> chain leaves the project, a relation whose target was never scanned, a repository over a type Bennu does not have —
  each turns the check off for that method. Nothing about the database is checked: whether a column exists needs a connection, which is Picus's business.
</Callout>

<h2>A <code>@Query</code> stops being a string</h2>
<ul>
  <li>Keywords, parameters, literals and numbers are coloured inside it.</li>
  <li>JPQL and native SQL are tinted apart, being different risks: JPQL is resolved against the entity model, native SQL sent to the database as written.</li>
  <li>A <code>:name</code> no parameter binds is an error on the placeholder, with the fix named.</li>
  <li><kbd>Ctrl</kbd> + <kbd>B</kbd> inside a query opens the entity it selects from.</li>
</ul>

<h2>The gutter links the two ends</h2>
<table>
  <thead><tr><th>Icon</th><th>Beside</th><th>Opens</th></tr></thead>
  <tbody>
    <tr><td><code>▤</code></td><td>An entity</td><td>The repositories managing it</td></tr>
    <tr><td><code>◇</code></td><td>A repository</td><td>Its entity</td></tr>
  </tbody>
</table>
<p>Hovering a repository method says what it asks for — a derived name rendered as the sentence it compiles to.</p>

<h2>The toolbar follows the file</h2>
<table>
  <thead><tr><th>Standing on</th><th>The toolbar offers</th></tr></thead>
  <tbody>
    <tr><td>An entity</td><td><strong>Add attribute</strong>, <strong>Add lifecycle callback</strong>, <strong>Add named query</strong>, <strong>Repository</strong>, <strong>Projection</strong></td></tr>
    <tr><td>A <code>@MappedSuperclass</code></td><td>The attribute and callback buttons — it has no table, so none of the repository ones</td></tr>
    <tr><td>A repository</td><td><strong>Add query method</strong>, <strong>Add modify method</strong></td></tr>
    <tr><td>Anything else</td><td>Nothing — the buttons present <em>are</em> the answer to what kind of file this is</td></tr>
  </tbody>
</table>
<p>
  Every generator previews live, <kbd>Ctrl</kbd> + <kbd>Enter</kbd> commits, and nothing is written before that. Each is also in the command palette by name.
</p>

<h3>Adding an attribute</h3>
<p>
  It writes the field, how it is stored — a plain column, <code>@Enumerated(STRING)</code>, <code>@Embedded</code>, <code>@Lob</code> — its constraints, the Bean
  Validation you ask for, and optionally its accessors. A second preview tab shows the <code>alter table</code> the column implies, because field and column are one
  decision usually made in two places, the second from memory later.
</p>
<Callout variant="warning" title="A starting point, not a migration">
  No dialect, and no back-fill for a <code>not null</code> added to a table that already has rows.
</Callout>
<p>
  Choose a <strong>relation</strong> and it writes the pair people get backwards by hand: the owning side gets the <code>@JoinColumn</code>, and filling in
  <em>mapped by</em> makes it the inverse side, owning no column.
</p>
<ul>
  <li>A to-many is a <code>Set</code> unless you say otherwise — a <code>List</code> of children makes Hibernate delete and re-insert the whole collection on any change.</li>
  <li>It is always initialized: the omission that becomes a <code>NullPointerException</code> the first time anything adds to a new entity.</li>
  <li>Cascade and orphan removal are there; the helper methods keeping both sides of a bidirectional relation in step are still yours to write.</li>
</ul>

<h3>Query methods</h3>
<p>
  Built from the entity's own properties — the point: a name assembled from properties that exist cannot be misspelled, and the parameter list follows from the keywords.
  Leave <em>method name</em> empty and the derived name is used; write one and the method arrives with its <code>@Query</code> spelled out, since a name Spring Data
  cannot parse is no longer a derived query.
</p>
<table>
  <thead><tr><th>Returns</th><th>Worth knowing</th></tr></thead>
  <tbody>
    <tr><td><code>Optional</code>, the bare entity, <code>List</code>, <code>Stream</code></td><td>—</td></tr>
    <tr><td><code>Page</code></td><td>Takes a <code>Pageable</code>, and also runs a <code>count(*)</code> for the total</td></tr>
    <tr><td><code>Slice</code></td><td>Takes a <code>Pageable</code>, skips the count and only says whether more rows follow — the one behind infinite scrolling</td></tr>
  </tbody>
</table>
<p>
  A finder can take a <code>Sort</code> so the caller decides the order — except a paged one, whose <code>Pageable</code> carries one already; taking both would not
  compile, and the dialog says so. The button you pressed decides where the form <em>opens</em>, not what it produces: the verb, the return shape, the ordering, a limit
  and <code>distinct</code> are all editable. <strong>Add and continue</strong> writes one and clears the form for the next, keeping the repository.
</p>

<h3>Modify methods</h3>
<p>
  Always <code>@Modifying</code> with the JPQL written out — Spring Data has no naming scheme for an update.
</p>
<Callout variant="warning" title="A bulk write skips the entity lifecycle">
  It goes straight to the database: rows are not loaded, so <code>@PreUpdate</code> and <code>@PreRemove</code> do not fire and the persistence context does not see it.
  The dialog says so, and warns when there are no conditions at all.
</Callout>

<h3>Repositories and projections</h3>
<p>
  <strong>Repositories</strong> land in the package the project already keeps them in, read off the ones that exist. A <strong>projection</strong> can be its own file
  <em>or</em> an interface nested in the repository that returns it — both idiomatic, both offered.
</p>
