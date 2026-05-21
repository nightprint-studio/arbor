arbor.events.on("on_plugin_load", function(ctx)
  arbor.log.info("__SLUG__: loaded (api_version=" .. tostring(ctx.api_version) .. ")")
  print("[__SLUG__] hello from main.lua")
end)

