
-- ── Recipe: keyboard shortcut ────────────────────────────────────────────────
-- The shortcut fires the named action — wire it up with arbor.events.on.
arbor.keybinding.register({
  key         = "K",
  ctrl        = true,
  shift       = true,
  action      = "__SLUG__:run",
  description = "Run the main __SLUG__ action",
})

arbor.events.on("__SLUG__:run", function(_ctx)
  arbor.log.info("shortcut fired")
end)
