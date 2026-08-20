<!-- Bennu docs — Tomcat: hot-swapping compiled classes into a running container. -->
<h1>Tomcat hot-swap</h1>
<p class="doc-lead">
  Push a class you have just compiled into a Tomcat that is already running, without a redeploy.
</p>

<h2>Tomcat hot-swap</h2>
<p>
  Link the project to a local Tomcat and push changed JSPs straight into the running server — no
  redeploy, no restart. Open <strong>Tomcat hot-swap…</strong> from the command palette and pick the
  Tomcat root (the folder holding <code>webapps/</code>, <code>bin/</code>, <code>conf/</code>).
  Bennu validates it, lists the deployed web applications, and auto-selects the one this project maps
  to — by <code>&lt;finalName&gt;</code>, artifactId or folder name, or the only app deployed. The
  link is remembered per project.
</p>
<p>
  <strong>Deploy current JSP</strong> (<kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F10</kbd>) saves the
  open JSP and copies it into the deployed webapp at the same path it has under the project's webapp
  source dir; Tomcat recompiles it on the next request. <strong>Deploy all JSPs to Tomcat</strong>
  (command palette) copies every page at once. Files are copied byte-for-byte, so each JSP keeps its
  own page encoding. A toast confirms what was deployed, or explains why it couldn't be (no Tomcat
  linked, no webapp source dir, or an ambiguous target — pick one in the settings).
</p>
