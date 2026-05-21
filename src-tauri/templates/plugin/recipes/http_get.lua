
-- ── Recipe: HTTP GET ─────────────────────────────────────────────────────────
-- Requires the target host in the [permissions].network allowlist.
local function fetch_demo()
  local res = arbor.http.get("https://api.github.com/zen")
  if res.status == 200 then
    arbor.notify{ message = res.body, level = "info" }
  else
    arbor.notify{ message = "HTTP " .. tostring(res.status), level = "error" }
  end
end
