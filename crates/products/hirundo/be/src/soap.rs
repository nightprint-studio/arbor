//! SOAP in Hirundo's backend: turning a pasted envelope or a WSDL (pasted, or fetched from a URL) into
//! requests, and reading an answer (the fault, an indented body).
//!
//! The reading of envelopes and WSDLs is Bennu's own (`bennu_generate::soap`, the code behind its `soap-call`
//! generator), so the two products agree on what an envelope's operation and a WSDL's actions are. What is
//! *not* read is the WSDL's schema: an imported operation starts with an empty body element, to be filled in.

use std::time::Duration;

use bennu_generate::soap::{parse_soap, parse_wsdl, Wsdl};
use hirundo_core::prelude::HirundoState;
use hirundo_model::prelude::{fault_of, pretty_xml, sample_envelope, Body, BodyKind, Fault, HttpRequest, Method, SoapSettings, SoapVersion};
use serde::Serialize;

/// The largest WSDL fetched.
const MAX_WSDL_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct SoapImport {
    pub requests: Vec<HttpRequest>,
    /// What the import could not do, in words (shown above the list).
    pub notes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SoapInspect {
    pub fault: Option<Fault>,
    /// The body indented, when it is XML.
    pub pretty: String,
}

fn soap_request(name: &str, url: &str, version: SoapVersion, action: &str, envelope: String) -> HttpRequest {
    HttpRequest {
        name: name.to_string(),
        method: Method::Post,
        url: url.to_string(),
        body: Body { kind: BodyKind::Soap, text: envelope, form: Vec::new() },
        soap: SoapSettings { version, action: action.to_string() },
        ..HttpRequest::default()
    }
}

/// One request per operation the WSDL gives a `soapAction` to.
fn requests_from_wsdl(wsdl: &Wsdl, source: &str) -> SoapImport {
    let version = if source.contains("wsdl/soap12") { SoapVersion::V12 } else { SoapVersion::V11 };
    let url = wsdl.address.clone().unwrap_or_else(|| "{{baseUrl}}".to_string());
    let mut seen: Vec<&str> = Vec::new();
    let mut requests = Vec::new();
    for (operation, action) in &wsdl.actions {
        if seen.contains(&operation.as_str()) {
            continue;
        }
        seen.push(operation);
        requests.push(soap_request(operation, &url, version, action, sample_envelope(version, operation, &wsdl.target_ns)));
    }
    let mut notes = vec!["The WSDL's schema is not read: each operation starts with an empty body element to fill in.".to_string()];
    if wsdl.address.is_none() {
        notes.push("The WSDL names no service address: set the URL (a {{baseUrl}} stands in).".into());
    }
    if requests.is_empty() {
        notes.push("No operation with a soapAction was found.".into());
    }
    SoapImport { requests, notes }
}

fn request_from_envelope(source: &str) -> Result<SoapImport, String> {
    let soap = parse_soap(source, false)?;
    let version = if soap.version == "1.2" { SoapVersion::V12 } else { SoapVersion::V11 };
    let name = if soap.operation.is_empty() { "SOAP call".to_string() } else { soap.operation.clone() };
    Ok(SoapImport {
        requests: vec![soap_request(&name, "{{baseUrl}}", version, "", source.trim().to_string())],
        notes: vec!["An envelope carries neither the address nor the SOAPAction: set them on the request.".into()],
    })
}

pub(crate) async fn fetch(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(15)).build().map_err(|e| e.to_string())?;
    let response = client.get(url).send().await.map_err(|e| format!("could not fetch {url}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("{url} answered {}", response.status()));
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > MAX_WSDL_BYTES {
        return Err("the WSDL is larger than 4 MB".into());
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// A WSDL (text, or a URL to fetch it from) or a SOAP envelope → requests.
#[arbor_rpc::handler]
async fn soap_import(_state: &HirundoState, source: String) -> Result<SoapImport, String> {
    let trimmed = source.trim();
    let text = if trimmed.starts_with("http://") || trimmed.starts_with("https://") { fetch(trimmed).await? } else { trimmed.to_string() };
    if text.is_empty() {
        return Err("paste a WSDL, a SOAP envelope, or the URL of a WSDL".into());
    }
    match parse_wsdl(&text) {
        Ok(wsdl) => Ok(requests_from_wsdl(&wsdl, &text)),
        Err(_) => request_from_envelope(&text),
    }
}

/// The fault and an indented view of an answer.
#[arbor_rpc::handler]
fn soap_inspect(_state: &HirundoState, body: String) -> Result<SoapInspect, String> {
    let looks_xml = body.trim_start().starts_with('<');
    Ok(SoapInspect { fault: fault_of(&body), pretty: if looks_xml { pretty_xml(&body) } else { body } })
}

#[cfg(test)]
mod tests {
    use super::*;

    const WSDL: &str = r#"<definitions xmlns="http://schemas.xmlsoap.org/wsdl/" xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/" targetNamespace="http://example.com/weather" name="Weather">
  <portType name="WeatherPort"><operation name="GetWeather"/><operation name="GetForecast"/></portType>
  <binding name="WeatherBinding" type="tns:WeatherPort">
    <operation name="GetWeather"><soap:operation soapAction="urn:GetWeather"/></operation>
    <operation name="GetForecast"><soap:operation soapAction="urn:GetForecast"/></operation>
  </binding>
  <service name="WeatherService"><port name="WeatherPort" binding="tns:WeatherBinding"><soap:address location="http://svc.example/weather"/></port></service>
</definitions>"#;

    #[test]
    fn a_wsdl_gives_one_request_per_operation_with_its_action_and_address() {
        let import = requests_from_wsdl(&parse_wsdl(WSDL).unwrap(), WSDL);
        assert_eq!(import.requests.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["GetWeather", "GetForecast"]);
        let first = &import.requests[0];
        assert_eq!((first.url.as_str(), first.soap.action.as_str(), first.method), ("http://svc.example/weather", "urn:GetWeather", Method::Post));
        assert_eq!(first.body.kind, BodyKind::Soap);
        assert!(first.body.text.contains("<tns:GetWeather>") && first.body.text.contains("http://example.com/weather"));
    }

    #[test]
    fn a_pasted_envelope_becomes_a_request_named_after_its_operation() {
        let envelope = sample_envelope(SoapVersion::V12, "Ping", "urn:x");
        let import = request_from_envelope(&envelope).unwrap();
        assert_eq!(import.requests[0].name, "Ping");
        assert_eq!(import.requests[0].soap.version, SoapVersion::V12);
        assert_eq!(import.requests[0].body.text, envelope);
    }

    #[test]
    fn something_that_is_neither_is_refused_in_words() {
        assert!(request_from_envelope("hello").is_err());
    }
}
