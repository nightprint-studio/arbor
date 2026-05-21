<script lang="ts">
  import '../app.css';

  let { children } = $props();
</script>

{@render children()}

<!-- Toasts and notifications are rendered inside AppShell's unified
     bottom-right stack alongside operation cards.  Don't duplicate the
     toast layer here. -->

