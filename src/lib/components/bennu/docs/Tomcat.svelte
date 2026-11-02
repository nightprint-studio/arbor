<script lang="ts">
  /**
   * Tomcat hot-swap: linking a project to a local Tomcat and pushing changed JSPs into the running web application.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>Tomcat</h1>

<p class="doc-lead">
  Run a web module on a Tomcat Bennu starts and configures for you — or push the page you just changed into one that is already running.
</p>

<h2>Running on Tomcat</h2>
<p>
  Add a <strong>Tomcat Server</strong> to the run configurations (the <strong>+</strong> in <em>Run Configurations</em>). ▶ and <kbd>Shift</kbd> + <kbd>F10</kbd> build the
  module, generate a server for it and start it; <kbd>Shift</kbd> + <kbd>F9</kbd> does the same under the debugger. The server's output is a console tab like any other —
  Stop ends it, ⟳ starts it again — and the browser opens on the application when Tomcat says it has started.
</p>

<h3>Server</h3>
<ul>
  <li><strong>Tomcat</strong> — an installation Bennu downloaded, or any folder holding Tomcat 8.5, 9, 10.1 or 11. <strong>Download…</strong> lists the stable releases of a line
    straight from Apache, checks the download against Apache's SHA-512 and unpacks it, by default into <strong>one folder in your Bennu profile shared by every project</strong>: a version is downloaded once,
    and two projects — or two configurations of one — run on it side by side, each with its own ports and <code>context.xml</code>. <em>Folders</em> changes where.</li>
  <li><strong>The servlet package</strong> decides what runs: Tomcat 9 and earlier speak <code>javax.servlet</code>, Tomcat 10 and later <code>jakarta.servlet</code>, and an
    application written for one does not load on the other. Bennu reads which one the project imports and marks a mismatch before launch.</li>
  <li><strong>Ports</strong> — HTTP and shutdown. A port something is already listening on is flagged as you type.</li>
</ul>

<h3>Deployment</h3>
<p>
  <strong>Live</strong> serves the module's <code>src/main/webapp</code> directly, with its compiled classes and runtime dependencies mounted underneath — nothing is packaged,
  and a JSP saved in the editor is the JSP the next request runs. <strong>External</strong> serves a WAR or exploded folder built elsewhere. The context path decides where the
  application is served.
</p>
<p>
  Each configuration gets its own generated server folder (<code>.arbor/bennu/tomcat/&lt;configuration&gt;</code>, in the project), rewritten at every launch; logs and compiled JSPs are kept.
  The installation itself is never touched — which is why one installation can serve any number of configurations, as long as their ports differ.
</p>

<h3>context.xml</h3>
<p>
  The data sources, environment entries and parameters Tomcat offers the application through JNDI. The generated file is written into the server folder, <em>not</em> into the
  project, so development credentials never end up in a WAR or in the repository.
</p>
<ul>
  <li><strong>Guided</strong> builds the file from a list of resources, with the result shown live. <strong>Raw XML</strong> is the text itself, in the code editor, for what the form
    does not model (a <code>Valve</code>, a <code>Manager</code>). The two are one document seen twice: entering Raw starts from what Guided would write — unless the text was edited by hand,
    when <strong>Regenerate from the form</strong> starts over — and going back reads the text into the form, so an <code>&lt;Environment&gt;</code> typed in Raw is a row of the list. Parts the form cannot show are named first, and
    switching drops them from the file only on a yes.</li>
  <li><strong>Add</strong> offers a data source, an environment entry or a parameter — and the two ways of not starting from nothing: <strong>Detect from the project</strong>
    lists the JNDI names the project's own files ask for (<code>web.xml</code>, <code>@Resource</code>, <code>lookup()</code>, <code>persistence.xml</code>, Spring XML), each
    with the line it came from, and adds the missing ones; <strong>Paste XML</strong> reads an existing <code>context.xml</code> or a <code>&lt;Resource&gt;</code> copied from
    a <code>server.xml</code>.</li>
  <li><strong>From a Picus connection</strong> is a picker in the Add menu: each connection is a row, and the one you pick is written as a data source — host, port, database and user, and the driver for its engine (PostgreSQL, Oracle).
    It takes the first JNDI name the project asks for that the context lacks. Picus never hands out a password, so you type it once. On a data source,
    <strong>Test connection</strong> reads the host, database and user out of the JDBC URL, takes the password typed in the form, and asks Picus to open that connection and close it again — it shows the database answers
    with what the form says, not that the driver jar is the right one. A data source taken from a Picus connection and not changed since is tried with the password Picus holds.</li>
  <li>Each row of the list has a checkbox and its own trash, shown on hover. Tick several rows (or <kbd>Ctrl</kbd>/<kbd>⌘</kbd>-click, <kbd>Shift</kbd>-click, <kbd>Ctrl</kbd> + <kbd>A</kbd>):
    with more than one data source selected the form becomes a <strong>bulk editor</strong> — pool sizes, driver, a find-and-replace across the URLs (swap a host everywhere), and
    where the passwords live.</li>
  <li>The JDBC driver jar is found on the project's runtime classpath and copied into the server's <code>lib/</code> at launch. One the project does not depend on is fetched from Maven Central
    for PostgreSQL, MySQL, MariaDB and SQLite — once, into the Tomcat folder of your Bennu profile, and reused by every project — as long as automatic dependency downloads are on. For any other driver (Oracle, DB2, SQL Server),
    <strong>Driver jar</strong> names the file to copy instead (absolute, or relative to the project); it also overrides the lookup for a driver Bennu could fetch.</li>
</ul>
<Callout variant="info" title="Passwords">
  By default a data source's password is kept <strong>outside the repository</strong>, in a file in your Bennu profile, and handed to Tomcat as a system property: the generated
  context holds only <code>$&lbrace;jdbc.Gestionale.password&rbrace;</code>. It is a plain file in your profile, not the system keychain — it keeps the password out of anything that is
  versioned, not out of reach of someone who can read your files. <em>In the context file</em> writes it as text instead.
</Callout>

<h3>Launch</h3>
<p>
  JVM options, the classpath scope, the environment (rows and an optional <code>.env</code> file, as for any configuration) and whether the debugger waits before starting.
  The default scope, <em>Runtime</em>, leaves out what the project marks <code>provided</code>; an API that an application server would have supplied (JAX-RS, say) is supplied by nobody on Tomcat, so
  add it as a runtime dependency or run with <em>Every scope</em>.
</p>

<h2>Linking a Tomcat that is already running</h2>
<ol class="step-list">
  <li>Open <strong>Tomcat hot-swap…</strong> from the command palette.</li>
  <li>Pick the Tomcat root — the folder holding <code>webapps/</code>, <code>bin/</code> and <code>conf/</code>. Bennu validates it and lists the deployed web
    applications.</li>
  <li>The one this project maps to is selected for you — by <code>&lt;finalName&gt;</code>, by artifactId or folder name, or as the only application deployed.
    Pick another if that is not it.</li>
</ol>
<p>The link is remembered per project.</p>

<h2>Deploying pages</h2>
<table>
  <thead><tr><th>Command</th><th>Does</th></tr></thead>
  <tbody>
    <tr>
      <td><strong>Deploy current JSP</strong> <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F10</kbd></td>
      <td>Saves the open JSP and copies it into the deployed application, at the same path it has under the project's webapp source folder. Tomcat recompiles it on the next request.</td>
    </tr>
    <tr>
      <td><strong>Deploy all JSPs to Tomcat</strong> — command palette</td>
      <td>Copies every page at once.</td>
    </tr>
  </tbody>
</table>
<Callout variant="info" title="Byte for byte">
  Files are copied as they are, so each JSP keeps its own page encoding.
</Callout>
<p>
  A toast confirms what was deployed — or says why it could not be: no Tomcat linked, no webapp source folder, or an ambiguous target to pick in the settings.
</p>
