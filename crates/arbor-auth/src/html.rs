//! Default success / error HTML pages for the loopback callback.
//!
//! Parameterised on the provider label so the same page works for any
//! OAuth integration ("Jira", "Google Cloud Storage", "GitHub", …).
//!
//! Consumers can override by passing custom `success_html` /
//! `error_html_template` on [`crate::oauth2::InstalledAppFlow`]. The error
//! template must contain the literal token `__MSG__` where the failure
//! reason will be substituted.

/// Build the success page. `{provider}` is inlined into the message.
pub fn default_success(provider: &str) -> String {
    SUCCESS_TEMPLATE.replace("__PROVIDER__", provider)
}

/// Build the error page. `{provider}` + `{message}` are inlined.
pub fn default_error(provider: &str, message: &str) -> String {
    ERROR_TEMPLATE
        .replace("__PROVIDER__", provider)
        .replace("__MSG__", message)
}

/// Apply a custom error template that contains the `__MSG__` token.
pub fn render_error_template(template: &str, message: &str) -> String {
    template.replace("__MSG__", message)
}

const SUCCESS_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>Arbor — Connected</title>
  <style>
    *{margin:0;padding:0;box-sizing:border-box}
    body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;
      background:#1e1f22;color:#dfe1e5;min-height:100vh;
      display:flex;align-items:center;justify-content:center}
    .card{background:#2b2d30;border:1px solid #3c3f41;border-radius:12px;
      padding:44px 52px;max-width:420px;width:90%;text-align:center}
    .brand{font-size:11px;font-weight:700;letter-spacing:.18em;text-transform:uppercase;
      color:#6c707a;margin-bottom:32px}
    .icon{width:56px;height:56px;border-radius:50%;background:#1a3326;
      border:2px solid #6aab73;display:flex;align-items:center;justify-content:center;
      margin:0 auto 20px;font-size:24px}
    h1{font-size:18px;font-weight:600;color:#6aab73;margin-bottom:10px}
    p{font-size:13px;color:#888d94;line-height:1.6}
    .provider{color:#dfe1e5;font-weight:500}
  </style>
</head>
<body>
  <div class="card">
    <div class="brand">Arbor</div>
    <div class="icon">✓</div>
    <h1>Connected successfully</h1>
    <p><span class="provider">__PROVIDER__</span> has been authorized.<br>You can close this tab and return to Arbor.</p>
  </div>
</body>
</html>"#;

const ERROR_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>Arbor — Authorization failed</title>
  <style>
    *{margin:0;padding:0;box-sizing:border-box}
    body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;
      background:#1e1f22;color:#dfe1e5;min-height:100vh;
      display:flex;align-items:center;justify-content:center}
    .card{background:#2b2d30;border:1px solid #3c3f41;border-radius:12px;
      padding:44px 52px;max-width:460px;width:90%;text-align:center}
    .brand{font-size:11px;font-weight:700;letter-spacing:.18em;text-transform:uppercase;
      color:#6c707a;margin-bottom:32px}
    .icon{width:56px;height:56px;border-radius:50%;background:#2d1f1f;
      border:2px solid #f87171;display:flex;align-items:center;justify-content:center;
      margin:0 auto 20px;font-size:24px}
    h1{font-size:18px;font-weight:600;color:#f87171;margin-bottom:10px}
    p{font-size:13px;color:#888d94;line-height:1.6}
    .provider{color:#dfe1e5;font-weight:500}
    .detail{margin-top:20px;padding:12px 14px;background:#1e1f22;
      border:1px solid #3c3f41;border-radius:6px;font-size:11px;
      color:#888d94;font-family:'JetBrains Mono','Fira Code',monospace;
      text-align:left;word-break:break-all;line-height:1.5}
  </style>
</head>
<body>
  <div class="card">
    <div class="brand">Arbor</div>
    <div class="icon">✗</div>
    <h1>Authorization failed</h1>
    <p>Could not connect <span class="provider">__PROVIDER__</span>. Please return to Arbor and try again.</p>
    <div class="detail">__MSG__</div>
  </div>
</body>
</html>"#;
