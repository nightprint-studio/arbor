<!-- Bennu docs — JPA: entities, queries and the mapping behind them. -->
<h1>JPA</h1>
<p class="doc-lead">
  Entities, their mapping and the queries written against them — resolved together, so a field
  name in a query is a reference rather than a string.
</p>

<h2>JPA</h2>
<p>
  On a project with JPA or Spring Data on its classpath — and only there, like every other
  framework tool here. Entities, repositories and the queries between them.
</p>
<p>
  <strong>Generated sources are part of the project.</strong> The static metamodel
  (<code>Order_</code>, <code>Customer_</code>) that Criteria queries are written against is
  written by an annotation processor into <code>target/generated-sources</code>, and so are
  MapStruct's <code>*MapperImpl</code>, QueryDSL's <code>QOrder</code> and jOOQ's output. None of
  it exists under <code>src/</code> and all of it is referenced from there, so Bennu indexes those
  two roots — but nothing else under <code>target/</code>, which holds build output and sometimes
  an unpacked copy of somebody else's sources. Build the project once and they resolve.
</p>
<p>
  <strong>Derived query names are checked.</strong>
  <code>findByCustomerNameAndTotalGreaterThan</code> is not a name, it is a query that Spring Data
  compiles at <em>application start</em>. A typo in one is invisible to the compiler and to every
  test that doesn't touch that repository, and then it takes the context down on deploy. So every
  segment is resolved against the entity — following relations, so <code>CustomerName</code> is
  <code>customer.name</code> — and a segment that addresses nothing is flagged where you wrote it.
  The number of arguments the name asks for is checked too: <code>Between</code> wants two,
  <code>IsNull</code> wants none, and a <code>Pageable</code> is Spring's, not yours.
</p>
<p>
  <strong>The check goes quiet rather than guess.</strong> An entity whose
  <code>@MappedSuperclass</code> chain leaves the project, a relation whose target was never
  scanned, a repository over a type Bennu doesn't have — each turns the check off for that method.
  Nothing about the database is checked at all: whether the column exists needs a connection, which
  is Picus's business, not this one's.
</p>
<p>
  <strong>A <code>@Query</code> stops being a string.</strong> Keywords, parameters, literals and
  numbers are coloured inside it, and JPQL and native SQL are tinted apart because they are
  different risks — JPQL is resolved against the entity model, native SQL is sent to the database
  as written. A <code>:name</code> that no parameter binds is an error on the placeholder itself,
  with the fix named. <kbd>Ctrl</kbd> + <kbd>B</kbd> inside a query opens the entity it selects from.
</p>
<p>
  <strong>The gutter links the two ends.</strong> <code>▤</code> beside an entity opens the
  repositories that manage it; <code>◇</code> beside a repository opens its entity. Hovering a
  repository method says what it actually asks for — a derived name is rendered as the sentence it
  compiles to.
</p>
<p>
  <strong>The toolbar follows the file.</strong> Standing on an entity, the editor toolbar carries
  <strong>Add attribute</strong>, <strong>Add lifecycle callback</strong>,
  <strong>Add named query</strong>, <strong>Repository</strong> and <strong>Projection</strong>.
  Standing on a repository it carries <strong>Add query method</strong> and
  <strong>Add modify method</strong> instead. On a class that is neither there is nothing — the
  buttons present <em>are</em> the answer to what kind of file this is, so there is no greyed-out
  row to interpret. A <code>@MappedSuperclass</code> gets the attribute and callback buttons but
  not the repository ones: it has no table, so those could not work.
</p>
<p>
  <strong>Adding an attribute</strong> writes the field, how it is stored — a plain column, an
  <code>@Enumerated(STRING)</code>, an <code>@Embedded</code>, an <code>@Lob</code> — its
  constraints, the Bean Validation you ask for, and optionally its accessors. The second preview
  tab shows the <code>alter table</code> the column implies, because the field and the column are
  one decision usually made in two places and the second one is written later from memory. It is a
  starting point and not a migration: no dialect, and no back-fill for a <code>not null</code>
  added to a table that already has rows.
</p>
<p>
  Choose a relation instead and it writes the pair people get backwards by hand: the owning side
  gets the <code>@JoinColumn</code>, and filling in <em>mapped by</em> makes it the inverse side,
  which owns no column at all. A to-many is held in a <code>Set</code> unless you say otherwise —
  a <code>List</code> of children makes Hibernate delete and re-insert the whole collection on any
  change — and it is always initialized, which is the omission that turns into a
  <code>NullPointerException</code> the first time anything adds to a new entity. Cascade and
  orphan removal are there too; the helper methods that keep both sides of a bidirectional relation
  in step are still yours to write.
</p>
<p>
  <strong>Query methods</strong> are built from the entity's own properties, and that is the point:
  a name assembled from properties that exist cannot be misspelled, and the parameter list follows
  from the keywords instead of being counted by eye. Leave <em>method name</em> empty and the
  derived name is used; write one and the method arrives with its <code>@Query</code> spelled out,
  because a name Spring Data cannot parse is no longer a derived query.
</p>
<p>
  <strong>What a finder hands back</strong> is a row of its own: <code>Optional</code>,
  the bare entity, <code>List</code>, <code>Page</code>, <code>Slice</code> or
  <code>Stream</code>. <code>Page</code> and <code>Slice</code> both take a <code>Pageable</code>
  and differ in what they cost — a <code>Page</code> also runs a <code>count(*)</code> to know the
  total, which a <code>Slice</code> skips because it only reports whether more rows follow. That is
  the one you want behind infinite scrolling. A finder can also take a <code>Sort</code> so the
  caller decides the ordering, except on a paged method, where the <code>Pageable</code> already
  carries one and taking both would not compile — the dialog says so rather than offering it.
</p>
<p>
  The button you pressed decides where the form <em>opens</em>, not what it can produce: the verb,
  the return shape, the ordering, a limit and <code>distinct</code> are all editable from inside.
  Adding several methods in a row is what actually happens, so <strong>Add and continue</strong>
  writes one and clears the form for the next without losing the repository you chose.
</p>
<p>
  <strong>Modify methods</strong> are always <code>@Modifying</code> with the JPQL written out.
  Spring Data has no naming scheme for an update at all, and a bulk write goes straight to the
  database — the rows are not loaded, so <code>@PreUpdate</code> and <code>@PreRemove</code> do not
  fire and the persistence context does not see it. The dialog says so, and warns when there are no
  conditions at all.
</p>
<p>
  <strong>Repositories</strong> land in the package the project already keeps repositories in, read
  off the ones that exist rather than assumed. A <strong>projection</strong> can be its own file
  <em>or</em> an interface nested inside the repository that returns it — both are idiomatic, and
  the dialog offers both. Every generator previews live, <kbd>Ctrl</kbd> + <kbd>Enter</kbd>
  commits, and nothing is written before that. Each is also in the command palette by name.
</p>
