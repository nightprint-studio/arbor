<h1>Event streams, cookies and SOAP</h1>

<h2>Server-Sent Events</h2>
<p>
  <b>Listen</b> (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>L</kbd>) opens the open request as a <code>text/event-stream</code> and shows the events in the <i>Event stream</i> panel as they arrive:
  filter by event name, pause, copy or save the log, and resume after the last event id. <kbd>Esc</kbd> stops it. A dropped connection retries quietly.
</p>

<h2>Cookie jar</h2>
<p>
  Cookies an answer sets are kept <b>per environment</b> (so staging's session never reaches production) and sent back to the same site. The <i>Cookies</i> panel lists them for the active environment;
  a collection can switch the jar off.
</p>

<h2>SOAP</h2>
<p>
  <b>New SOAP request</b> starts from an envelope; <b>Import a WSDL or envelope</b> reads the operations. The SOAP version and <code>SOAPAction</code> are set for you,
  <i>WS-Security</i> (UsernameToken) is an auth option on SOAP requests, and a Fault in the answer is shown with its code, message and detail.
</p>
