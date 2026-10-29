<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>{{title}}</title>
  </head>
  <body>
    <pre id="out"></pre>
    <script type="module">
      import init, { greet } from "./pkg/{{ident}}.js";
      await init();
      document.getElementById("out").textContent = greet("wasm");
    </script>
  </body>
</html>
