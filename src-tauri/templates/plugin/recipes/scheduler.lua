
-- ── Recipe: background scheduler ─────────────────────────────────────────────
-- Fixed-rate trigger: fires every 60 seconds. Set `only_when_focused = true`
-- if you want the scheduler to skip ticks while the app is in the background.
arbor.scheduler.register({
  action            = "__SLUG__:tick",
  trigger           = { kind = "fixed_rate", interval_sec = 60 },
  on_load           = false,
  only_when_focused = true,
})

arbor.events.on("__SLUG__:tick", function(_ctx)
  arbor.log.info("scheduled tick at " .. os.date("%H:%M:%S"))
end)
