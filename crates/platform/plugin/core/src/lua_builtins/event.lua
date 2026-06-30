-- arbor.event — internal pub/sub bus
-- require("arbor.event")
--
-- Allows decoupled communication between plugin modules without circular requires.
--
-- Usage:
--   local event = require("arbor.event")
--
--   -- combo.lua
--   event.emit("configs_changed", { repo = state.repo })
--
--   -- forms.lua
--   event.on("configs_changed", function(payload)
--       -- refresh open settings UI if needed
--   end)

local M = {}
local listeners = {}

--- Register a handler for `event`.
function M.on(event, fn)
    listeners[event] = listeners[event] or {}
    table.insert(listeners[event], fn)
end

--- Remove all handlers for `event` (or a specific handler if `fn` is given).
function M.off(event, fn)
    if not listeners[event] then return end
    if fn == nil then
        listeners[event] = {}
    else
        local filtered = {}
        for _, h in ipairs(listeners[event]) do
            if h ~= fn then filtered[#filtered + 1] = h end
        end
        listeners[event] = filtered
    end
end

--- Emit `event` with `payload` to all registered handlers.
--- Errors inside handlers are logged but do not stop other handlers.
function M.emit(event, payload)
    for _, fn in ipairs(listeners[event] or {}) do
        local ok, err = pcall(fn, payload)
        if not ok then
            arbor.log.warn("event error [" .. tostring(event) .. "]: " .. tostring(err))
        end
    end
end

return M
