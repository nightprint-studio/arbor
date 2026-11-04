//! The XML flavour of the Java HTTP client: content negotiation per library and the mapper behind the bodies.

use super::*;
use crate::testing::sample_context;

fn gen(library: &str, extra: serde_json::Value) -> Output {
    let mut options = serde_json::json!({
        "library": library,
        "package": "com.acme.client",
        "content": "xml",
        "endpoints": ["GET /api/orders/{id}", "POST /api/orders"],
    });
    if let (Some(base), Some(add)) = (options.as_object_mut(), extra.as_object()) {
        for (k, v) in add {
            base.insert(k.clone(), v.clone());
        }
    }
    HttpClient.generate(&options, &sample_context()).expect("generates")
}

fn client(out: &Output) -> &str {
    &out.files[0].text
}

#[test]
fn rest_client_and_web_client_ask_for_xml() {
    for library in ["restclient", "webclient"] {
        let t = client(&gen(library, serde_json::json!({}))).to_string();
        assert!(t.contains(".accept(MediaType.APPLICATION_XML)"), "{library}: {t}");
        assert!(t.contains(".contentType(MediaType.APPLICATION_XML)"), "{library}: {t}");
        assert!(t.contains("import org.springframework.http.MediaType;"), "{library}: {t}");
    }
}

#[test]
fn rest_template_always_builds_headers_for_xml() {
    let t = client(&gen("resttemplate", serde_json::json!({}))).to_string();
    assert!(t.contains("headers.setAccept(List.of(MediaType.APPLICATION_XML));"), "{t}");
    assert!(t.contains("headers.setContentType(MediaType.APPLICATION_XML);"), "{t}");
    assert!(t.contains("new HttpEntity<>(request, headers)"), "{t}");
}

#[test]
fn declarative_clients_name_the_media_type_once() {
    let exchange = client(&gen("httpinterface", serde_json::json!({}))).to_string();
    assert!(exchange.contains("@HttpExchange(accept = MediaType.APPLICATION_XML_VALUE, contentType = MediaType.APPLICATION_XML_VALUE)"), "{exchange}");
    let feign = client(&gen("feign", serde_json::json!({}))).to_string();
    assert!(feign.contains("@GetMapping(value = \"/api/orders/{id}\", consumes = \"application/xml\", produces = \"application/xml\")"), "{feign}");
}

#[test]
fn the_jdk_client_writes_xml_with_an_xml_mapper() {
    let out = gen("jdk", serde_json::json!({ "xml_binding": "jaxb" }));
    let t = client(&out);
    assert!(t.contains("\"Content-Type\", \"application/xml\""), "{t}");
    assert!(t.contains("\"Accept\", \"application/xml\""), "{t}");
    let config = out.files.iter().find(|f| f.name.ends_with("Config.java")).unwrap();
    assert!(config.text.contains("ObjectMapper mapper = new XmlMapper();"), "{}", config.text);
    assert!(config.text.contains("mapper.registerModule(new JaxbAnnotationModule());"), "{}", config.text);
    assert!(!config.text.contains("ObjectMapper mapper,"), "{}", config.text);
    assert!(out.notes.iter().any(|n| n.contains("JaxbAnnotationModule")));
}

#[test]
fn okhttp_sends_the_xml_media_type() {
    let t = client(&gen("okhttp", serde_json::json!({}))).to_string();
    assert!(t.contains("MediaType.get(\"application/xml\")"), "{t}");
    assert!(t.contains("requestBuilder.header(\"Accept\", \"application/xml\");"), "{t}");
}

#[test]
fn retrofit_uses_a_jaxb_converter_for_xml() {
    let out = gen("retrofit", serde_json::json!({}));
    assert!(client(&out).contains("@Headers(\"Accept: application/xml\")"), "{}", client(&out));
    let config = out.files.iter().find(|f| f.name.ends_with("Config.java")).unwrap();
    assert!(config.text.contains(".addConverterFactory(JaxbConverterFactory.create())"), "{}", config.text);
    assert!(config.text.contains("import retrofit2.converter.jaxb3.JaxbConverterFactory;"), "{}", config.text);
}

#[test]
fn json_stays_the_default() {
    let t = client(&gen("restclient", serde_json::json!({ "content": "json" }))).to_string();
    assert!(!t.contains("APPLICATION_XML"), "{t}");
}
