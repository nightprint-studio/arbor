-- arbor.schema — centralised field validation
-- require("arbor.schema")
--
-- Usage:
--   local schema = require("arbor.schema")
--   if not schema.check(ctx, {
--       id      = { required = true, pattern = "^[%w_%-]+$",
--                   message = "ID: only letters, numbers, - and _" },
--       command = { required = true },
--   }) then return end

local M = {}

--- Validate `data` against a set of rules.
--- Returns ok (bool), errors (table: field -> message).
function M.validate(data, rules)
    local errors = {}
    for field, rule in pairs(rules) do
        local v = data[field]
        local empty = (v == nil or v == "")
        if rule.required and empty then
            errors[field] = rule.message or (field .. " is required")
        elseif not empty then
            if rule.pattern and not tostring(v):match(rule.pattern) then
                errors[field] = rule.message or (field .. " is invalid")
            end
            if rule.min_len and #tostring(v) < rule.min_len then
                errors[field] = field .. " must be at least " .. rule.min_len .. " characters"
            end
            if rule.max_len and #tostring(v) > rule.max_len then
                errors[field] = field .. " must be at most " .. rule.max_len .. " characters"
            end
            if rule.min and tonumber(v) and tonumber(v) < rule.min then
                errors[field] = field .. " must be >= " .. rule.min
            end
            if rule.max and tonumber(v) and tonumber(v) > rule.max then
                errors[field] = field .. " must be <= " .. rule.max
            end
        end
    end
    return next(errors) == nil, errors
end

--- Run validate() and surface the first error as a notification.
--- Returns true if all rules pass, false otherwise.
function M.check(data, rules)
    local ok, errors = M.validate(data, rules)
    if not ok then
        local _, msg = next(errors)
        arbor.notify{ title = "Validation error", message = msg, level = "warning" }
    end
    return ok
end

return M
