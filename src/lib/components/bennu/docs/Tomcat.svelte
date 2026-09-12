<script lang="ts">
  /**
   * Tomcat hot-swap: linking a project to a local Tomcat and pushing changed JSPs into the running web application.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>Tomcat hot-swap</h1>

<p class="doc-lead">
  Push the page you just changed into a Tomcat that is already running — no redeploy, no restart.
</p>

<h2>Linking a Tomcat</h2>
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
