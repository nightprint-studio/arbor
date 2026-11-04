//! Reading a pasted SOAP envelope (and, optionally, its WSDL) with the XML editor's own scanner and tree
//! ([`bennu_xml::prelude::Doc`]): which SOAP version, which operation and namespace, which leaf values are
//! the call's parameters, and the envelope / payload as a template with `%s` where a parameter goes.

use bennu_xml::prelude::{Doc, TagKind};

use crate::names::camel;

pub const SOAP11: &str = "http://schemas.xmlsoap.org/soap/envelope/";
pub const SOAP12: &str = "http://www.w3.org/2003/05/soap-envelope";

/// A value in the request that the generated method takes as an argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    /// The Java variable.
    pub name: String,
    /// The element it is the text of.
    pub element: String,
    /// What the pasted envelope had there.
    pub sample: String,
}

#[derive(Debug, Clone)]
pub struct Soap {
    /// `1.1` | `1.2`
    pub version: &'static str,
    pub env_ns: String,
    pub env_prefix: String,
    /// The local name of the first element in the Body.
    pub operation: String,
    pub operation_ns: String,
    pub params: Vec<Param>,
    /// The whole envelope; `%s` where a parameter goes, preceded by one for a security header when asked.
    pub envelope_template: String,
    /// Just the operation element (what Spring-WS and JAX-WS payload mode send), same placeholders without the security one.
    pub payload_template: String,
    pub has_header: bool,
}

/// An edit to the source: replace `start..end` with `text`.
struct Edit {
    start: usize,
    end: usize,
    text: String,
}

/// A small envelope to start from: the dialog opens with it so there is always something to preview.
pub const SAMPLE_ENVELOPE: &str = r#"<?xml version="1.0"?>
<soapenv:Envelope xmlns:soapenv="http://schemas.xmlsoap.org/soap/envelope/" xmlns:wea="http://example.com/weather">
   <soapenv:Header/>
   <soapenv:Body>
      <wea:GetWeather>
         <wea:City>Rome</wea:City>
         <wea:Country>IT</wea:Country>
      </wea:GetWeather>
   </soapenv:Body>
</soapenv:Envelope>"#;

const MARK: char = '\u{1}';

fn attr<'a>(doc: &'a Doc, tag: usize, name: &str) -> Option<&'a str> {
    doc.scan().tags.get(tag)?.attrs.iter().find(|a| a.name == name).map(|a| a.value.as_str())
}

fn prefix_of(name: &str) -> Option<&str> {
    name.split_once(':').map(|(p, _)| p)
}

/// The namespace URI a prefix (or the default) means on `tag`, looking at the tag and then the root.
fn namespace(doc: &Doc, tag: usize, root: usize, prefix: Option<&str>) -> String {
    let key = match prefix {
        Some(p) => format!("xmlns:{p}"),
        None => "xmlns".to_string(),
    };
    attr(doc, tag, &key).or_else(|| attr(doc, root, &key)).unwrap_or_default().to_string()
}

fn leaves(doc: &Doc, element: usize, out: &mut Vec<usize>) {
    let children = doc.children(element);
    if children.is_empty() {
        if doc.text_span(element).is_some() {
            out.push(element);
        }
        return;
    }
    for c in children {
        leaves(doc, c, out);
    }
}

/// `source[range]` with `edits` applied (offsets in `source`), `%` doubled so the result is a `String.format` template.
fn render(source: &str, range: (usize, usize), edits: &[Edit]) -> String {
    let mut inside: Vec<&Edit> = edits.iter().filter(|e| e.start >= range.0 && e.end <= range.1).collect();
    inside.sort_by(|a, b| b.start.cmp(&a.start));
    let mut text = source[range.0..range.1].to_string();
    for e in inside {
        text.replace_range(e.start - range.0..e.end - range.0, &e.text);
    }
    text.replace("\r\n", "\n").replace('%', "%%").replace(MARK, "%s")
}

pub fn parse_soap(source: &str, security: bool) -> Result<Soap, String> {
    let doc = Doc::new(source);
    let root = doc.root().ok_or("There is no XML element in the envelope")?;
    if doc.name(root) != "Envelope" {
        return Err(format!("The root element is `{}`, not a SOAP `Envelope`", doc.name(root)));
    }
    let tags = &doc.scan().tags;
    let root_name = tags[root].name.clone();
    let env_prefix = prefix_of(&root_name).unwrap_or_default().to_string();
    let env_ns = namespace(&doc, root, root, prefix_of(&root_name));
    let version = match env_ns.as_str() {
        SOAP11 => "1.1",
        SOAP12 => "1.2",
        other => return Err(format!("`{other}` is not a SOAP 1.1 or 1.2 envelope namespace")),
    };
    let body = doc.child(root, "Body").ok_or("The envelope has no Body")?;
    let op = doc.children(body).into_iter().next().ok_or("The Body is empty")?;
    let op_name = tags[op].name.clone();
    let operation_ns = namespace(&doc, op, root, prefix_of(&op_name));

    let mut leaf_tags = Vec::new();
    leaves(&doc, op, &mut leaf_tags);
    let mut params: Vec<Param> = Vec::new();
    let mut edits: Vec<Edit> = Vec::new();
    for leaf in leaf_tags {
        let Some((start, end)) = doc.text_span(leaf) else { continue };
        let element = doc.name(leaf).to_string();
        let mut name = camel(&element);
        if name.is_empty() || !name.chars().next().is_some_and(|c| c.is_alphabetic()) {
            name = format!("value{}", params.len() + 1);
        }
        let taken = params.iter().filter(|p| p.name == name || p.name.trim_end_matches(|c: char| c.is_ascii_digit()) == name).count();
        if taken > 0 {
            name = format!("{name}{}", taken + 1);
        }
        params.push(Param { name, element, sample: source[start..end].to_string() });
        edits.push(Edit { start, end, text: MARK.to_string() });
    }

    let header = doc.child(root, "Header");
    if security {
        match header {
            Some(h) if tags[h].kind == TagKind::SelfClose => {
                let t = &tags[h];
                edits.push(Edit { start: t.start, end: t.end, text: format!("<{0}>{MARK}</{0}>", t.name) });
            }
            Some(h) => edits.push(Edit { start: tags[h].end, end: tags[h].end, text: MARK.to_string() }),
            None => {
                let name = if env_prefix.is_empty() { "Header".to_string() } else { format!("{env_prefix}:Header") };
                edits.push(Edit { start: tags[body].start, end: tags[body].start, text: format!("<{name}>{MARK}</{name}>") });
            }
        }
    }
    // The payload gets the namespaces the envelope declared, which its open tag does not carry itself.
    let (op_start, op_end) = doc.element_span(op).ok_or("The operation element is not closed")?;
    let missing: String = tags[root]
        .attrs
        .iter()
        .filter(|a| (a.name == "xmlns" || a.name.starts_with("xmlns:")) && a.value != env_ns)
        .filter(|a| !tags[op].attrs.iter().any(|b| b.name == a.name))
        .map(|a| format!(" {}=\"{}\"", a.name, a.value))
        .collect();
    let mut payload_edits: Vec<Edit> = edits.iter().filter(|e| e.start >= op_start && e.end <= op_end).map(|e| Edit { start: e.start, end: e.end, text: e.text.clone() }).collect();
    if !missing.is_empty() {
        let at = tags[op].end - if tags[op].kind == TagKind::SelfClose { 2 } else { 1 };
        payload_edits.push(Edit { start: at, end: at, text: missing });
    }

    let envelope_span = doc.element_span(root).ok_or("The envelope is not closed")?;
    let envelope_template = render(source, envelope_span, &edits);
    let payload_template = render(source, (op_start, op_end), &payload_edits);
    Ok(Soap {
        version,
        env_ns,
        env_prefix,
        operation: doc.name(op).to_string(),
        operation_ns,
        params,
        envelope_template: envelope_template.trim().to_string(),
        payload_template: payload_template.trim().to_string(),
        has_header: header.is_some(),
    })
}

/// What a WSDL says about the service, as far as a client needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wsdl {
    pub target_ns: String,
    pub service: Option<String>,
    pub port: Option<String>,
    pub port_type: Option<String>,
    pub address: Option<String>,
    /// `(operation, soapAction)`
    pub actions: Vec<(String, String)>,
}

impl Wsdl {
    pub fn action_of(&self, operation: &str) -> Option<&str> {
        self.actions.iter().find(|(op, _)| op == operation).map(|(_, a)| a.as_str()).filter(|a| !a.is_empty())
    }
}

pub fn parse_wsdl(source: &str) -> Result<Wsdl, String> {
    let doc = Doc::new(source);
    let root = doc.root().ok_or("There is no XML element in the WSDL")?;
    if !matches!(doc.name(root), "definitions" | "description") {
        return Err(format!("The root element is `{}`, not a WSDL `definitions`", doc.name(root)));
    }
    let mut out = Wsdl { target_ns: attr(&doc, root, "targetNamespace").unwrap_or_default().to_string(), ..Wsdl::default() };
    let mut current_op: Option<String> = None;
    for (i, tag) in doc.scan().tags.iter().enumerate() {
        if tag.kind == TagKind::Close {
            continue;
        }
        let value = |n: &str| attr(&doc, i, n).map(str::to_string);
        match tag.local() {
            "service" if out.service.is_none() => out.service = value("name"),
            "port" if out.port.is_none() => out.port = value("name"),
            "portType" if out.port_type.is_none() => out.port_type = value("name"),
            "address" if out.address.is_none() => out.address = value("location"),
            "operation" => match (value("soapAction"), value("name")) {
                (Some(action), _) => {
                    if let Some(op) = &current_op {
                        out.actions.push((op.clone(), action));
                    }
                }
                (None, Some(name)) => current_op = Some(name),
                _ => {}
            },
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::SAMPLE_ENVELOPE as ENVELOPE;

    #[test]
    fn the_operation_namespace_and_parameters_are_read_from_the_body() {
        let s = parse_soap(ENVELOPE, false).unwrap();
        assert_eq!(s.version, "1.1");
        assert_eq!(s.operation, "GetWeather");
        assert_eq!(s.operation_ns, "http://example.com/weather");
        assert_eq!(s.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["city", "country"]);
        assert_eq!(s.params[0].sample, "Rome");
    }

    #[test]
    fn parameters_become_format_placeholders_and_percent_is_escaped() {
        let s = parse_soap(&ENVELOPE.replace("Rome", "100%"), false).unwrap();
        assert!(s.envelope_template.starts_with("<soapenv:Envelope"), "{}", s.envelope_template);
        assert!(s.envelope_template.contains("<wea:City>%s</wea:City>"), "{}", s.envelope_template);
        assert!(!s.envelope_template.contains("100"), "{}", s.envelope_template);
        let literal = parse_soap(&ENVELOPE.replace("<soapenv:Header/>", "<soapenv:Header><x>50%</x></soapenv:Header>"), false).unwrap();
        assert!(literal.envelope_template.contains("<x>50%%</x>"), "{}", literal.envelope_template);
    }

    #[test]
    fn a_security_slot_goes_into_an_empty_header_an_existing_one_or_a_new_one() {
        let empty = parse_soap(ENVELOPE, true).unwrap();
        assert!(empty.envelope_template.contains("<soapenv:Header>%s</soapenv:Header>"), "{}", empty.envelope_template);
        let existing = parse_soap(&ENVELOPE.replace("<soapenv:Header/>", "<soapenv:Header><a/></soapenv:Header>"), true).unwrap();
        assert!(existing.envelope_template.contains("<soapenv:Header>%s<a/>"), "{}", existing.envelope_template);
        let none = parse_soap(&ENVELOPE.replace("   <soapenv:Header/>\n", ""), true).unwrap();
        assert!(none.envelope_template.contains("<soapenv:Header>%s</soapenv:Header>"), "{}", none.envelope_template);
        assert!(!parse_soap(ENVELOPE, false).unwrap().envelope_template.contains("Header>%s"));
    }

    #[test]
    fn the_payload_carries_the_namespaces_the_envelope_declared() {
        let s = parse_soap(ENVELOPE, true).unwrap();
        assert!(s.payload_template.starts_with("<wea:GetWeather xmlns:wea=\"http://example.com/weather\">") || s.payload_template.starts_with("<wea:GetWeather"), "{}", s.payload_template);
        assert!(s.payload_template.contains("xmlns:wea=\"http://example.com/weather\""), "{}", s.payload_template);
        assert!(!s.payload_template.contains("Header"), "{}", s.payload_template);
        assert_eq!(s.payload_template.matches("%s").count(), 2);
    }

    #[test]
    fn soap_12_is_recognised_and_other_documents_refused() {
        let v12 = ENVELOPE.replace(SOAP11, SOAP12);
        assert_eq!(parse_soap(&v12, false).unwrap().version, "1.2");
        assert!(parse_soap("<a/>", false).is_err());
        assert!(parse_soap(&ENVELOPE.replace(SOAP11, "urn:other"), false).is_err());
        assert!(parse_soap("", false).is_err());
    }

    #[test]
    fn repeated_element_names_get_distinct_arguments() {
        let source = ENVELOPE.replace("<wea:Country>IT</wea:Country>", "<wea:City>Milan</wea:City>");
        let s = parse_soap(&source, false).unwrap();
        assert_eq!(s.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["city", "city2"]);
    }

    #[test]
    fn a_wsdl_gives_the_address_the_port_and_the_soap_action_per_operation() {
        let wsdl = r#"<wsdl:definitions xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/" xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/" targetNamespace="http://example.com/weather">
  <wsdl:portType name="WeatherPortType"><wsdl:operation name="GetWeather"/></wsdl:portType>
  <wsdl:binding name="WeatherBinding" type="tns:WeatherPortType">
    <wsdl:operation name="GetWeather"><soap:operation soapAction="http://example.com/GetWeather"/></wsdl:operation>
  </wsdl:binding>
  <wsdl:service name="WeatherService"><wsdl:port name="WeatherPort" binding="tns:WeatherBinding"><soap:address location="http://host/ws"/></wsdl:port></wsdl:service>
</wsdl:definitions>"#;
        let w = parse_wsdl(wsdl).unwrap();
        assert_eq!(w.target_ns, "http://example.com/weather");
        assert_eq!(w.service.as_deref(), Some("WeatherService"));
        assert_eq!(w.port.as_deref(), Some("WeatherPort"));
        assert_eq!(w.port_type.as_deref(), Some("WeatherPortType"));
        assert_eq!(w.address.as_deref(), Some("http://host/ws"));
        assert_eq!(w.action_of("GetWeather"), Some("http://example.com/GetWeather"));
        assert_eq!(w.action_of("Other"), None);
        assert!(parse_wsdl("<a/>").is_err());
    }
}
