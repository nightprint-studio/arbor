//! XML set — minimal XPath-like editor for `arbor.fs.xml_set`.
//!
//! Supports:
//!   · `/a/b/c`                 — absolute element path (sets text)
//!   · `//c`                    — first `c` anywhere (sets text)
//!   · `/a/b/@attr`             — attribute on element (sets value)
//!   · `/a/b[@k='v']/c`         — predicate: filter `b` where attr `k = v`
//!
//! Built on top of quick-xml reader + writer. Preserves the rest of the
//! document byte-for-byte (comments, whitespace) by streaming events and
//! only rewriting the targeted element's contents / attribute.

pub(crate) fn apply_xml_set(
    input: &str,
    xpath: &str,
    new_value: &str,
) -> std::result::Result<String, String> {
    use quick_xml::events::{BytesText, Event};
    use quick_xml::{Reader, Writer};

    let (steps, target_attr) = parse_xpath(xpath)?;
    let mut reader = Reader::from_str(input);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::new());

    let mut stack: Vec<String> = Vec::new();
    // track element name + attributes for predicate matching
    let mut attr_stack: Vec<Vec<(String, String)>> = Vec::new();
    let mut replacing_text_for: Option<usize> = None;

    loop {
        let ev = reader.read_event()
            .map_err(|e| format!("xml parse: {e}"))?;
        match ev {
            Event::Eof => break,
            Event::Start(e) => {
                let name = std::str::from_utf8(e.name().as_ref())
                    .map_err(|_| "non-utf8 tag name".to_string())?
                    .to_string();
                let mut attrs: Vec<(String, String)> = Vec::new();
                for a in e.attributes().flatten() {
                    let k = std::str::from_utf8(a.key.as_ref()).unwrap_or("").to_string();
                    let v = a.unescape_value().map(|v| v.into_owned()).unwrap_or_default();
                    attrs.push((k, v));
                }
                stack.push(name.clone());
                attr_stack.push(attrs.clone());

                // Element-text target?
                if target_attr.is_none() && path_matches(&steps, &stack, &attr_stack) {
                    // Rewrite: keep the Start event, swap children by emitting
                    // a text event until the matching End, then suppress
                    // original children. We set the flag; next events until
                    // End at depth len(stack) get dropped.
                    writer.write_event(Event::Start(e.to_owned()))
                        .map_err(|e| format!("write: {e}"))?;
                    let _ = writer.write_event(Event::Text(BytesText::new(new_value)));
                    replacing_text_for = Some(stack.len());
                    continue;
                }

                // Attribute target?
                if let Some(attr_name) = &target_attr {
                    if path_matches(&steps, &stack, &attr_stack) {
                        // Rewrite the Start event with the new attribute value.
                        let mut new_elem = quick_xml::events::BytesStart::new(&name);
                        let mut replaced = false;
                        for (k, v) in &attrs {
                            if k == attr_name {
                                new_elem.push_attribute((k.as_bytes(), new_value.as_bytes()));
                                replaced = true;
                            } else {
                                new_elem.push_attribute((k.as_bytes(), v.as_bytes()));
                            }
                        }
                        if !replaced {
                            new_elem.push_attribute((attr_name.as_bytes(), new_value.as_bytes()));
                        }
                        writer.write_event(Event::Start(new_elem))
                            .map_err(|e| format!("write: {e}"))?;
                        continue;
                    }
                }

                writer.write_event(Event::Start(e.to_owned()))
                    .map_err(|e| format!("write: {e}"))?;
            }
            Event::End(e) => {
                // Suppress original children while we're inside a replaced-text block.
                let closing_at = stack.len();
                stack.pop();
                attr_stack.pop();
                if let Some(depth) = replacing_text_for {
                    if depth == closing_at {
                        replacing_text_for = None;
                    } else if depth < closing_at {
                        // still inside, drop end event
                        continue;
                    }
                }
                writer.write_event(Event::End(e.to_owned()))
                    .map_err(|e| format!("write: {e}"))?;
            }
            Event::Empty(e) => {
                // Self-closing. If this is the target, rewrite it as a pair
                // with the new text (for element-text) or swap attr.
                let name = std::str::from_utf8(e.name().as_ref())
                    .map_err(|_| "non-utf8 tag name".to_string())?
                    .to_string();
                let mut attrs: Vec<(String, String)> = Vec::new();
                for a in e.attributes().flatten() {
                    let k = std::str::from_utf8(a.key.as_ref()).unwrap_or("").to_string();
                    let v = a.unescape_value().map(|v| v.into_owned()).unwrap_or_default();
                    attrs.push((k, v));
                }
                stack.push(name.clone());
                attr_stack.push(attrs.clone());
                let matched = path_matches(&steps, &stack, &attr_stack);

                if matched && target_attr.is_none() {
                    // Element-text on an empty element: expand to `<foo>text</foo>`.
                    let start = quick_xml::events::BytesStart::new(&name);
                    writer.write_event(Event::Start(start))
                        .map_err(|e| format!("write: {e}"))?;
                    let _ = writer.write_event(Event::Text(BytesText::new(new_value)));
                    let _ = writer.write_event(Event::End(quick_xml::events::BytesEnd::new(&name)));
                } else if matched && target_attr.is_some() {
                    let attr_name = target_attr.as_ref().unwrap();
                    let mut new_elem = quick_xml::events::BytesStart::new(&name);
                    let mut replaced = false;
                    for (k, v) in &attrs {
                        if k == attr_name {
                            new_elem.push_attribute((k.as_bytes(), new_value.as_bytes()));
                            replaced = true;
                        } else {
                            new_elem.push_attribute((k.as_bytes(), v.as_bytes()));
                        }
                    }
                    if !replaced {
                        new_elem.push_attribute((attr_name.as_bytes(), new_value.as_bytes()));
                    }
                    writer.write_event(Event::Empty(new_elem))
                        .map_err(|e| format!("write: {e}"))?;
                } else {
                    writer.write_event(Event::Empty(e.to_owned()))
                        .map_err(|e| format!("write: {e}"))?;
                }
                stack.pop();
                attr_stack.pop();
            }
            other => {
                if replacing_text_for.is_some() {
                    // suppress original children inside text-replacement block
                    continue;
                }
                writer.write_event(other)
                    .map_err(|e| format!("write: {e}"))?;
            }
        }
    }

    let bytes = writer.into_inner();
    String::from_utf8(bytes).map_err(|e| format!("non-utf8 output: {e}"))
}

/// One step in a minimal XPath expression. `name` "" means wildcard (`//`).
#[derive(Debug, Clone)]
struct XPathStep {
    name:          String,
    any_ancestor:  bool,    // true when preceded by `//`
    attr_filter:   Option<(String, String)>, // from `[@k='v']`
}

/// Split `xpath` into (steps, optional_attr_target).
/// Returns Err for clearly malformed input.
fn parse_xpath(xpath: &str) -> std::result::Result<(Vec<XPathStep>, Option<String>), String> {
    let s = xpath.trim();
    if s.is_empty() { return Err("empty xpath".into()); }
    // Check trailing attribute target `@attr`.
    let (path, attr_target) = if let Some(pos) = s.rfind("/@") {
        (&s[..pos], Some(s[pos + 2..].to_string()))
    } else {
        (s, None)
    };

    // Tokenize path by `/`, track `//` as "any ancestor".
    let mut steps = Vec::new();
    let mut any_ancestor = false;
    let path = path.trim_start_matches('/');
    // If original started with `//`, first step is wildcard-anywhere; the
    // trim_start_matches collapsed both slashes so we detect via the original.
    if xpath.starts_with("//") { any_ancestor = true; }

    for raw in path.split('/') {
        if raw.is_empty() {
            any_ancestor = true;
            continue;
        }
        // Parse predicate `[@k='v']` or `[@k="v"]`.
        let (name, pred) = if let Some(bra) = raw.find('[') {
            let end = raw.find(']').ok_or_else(|| format!("missing ']' in {raw}"))?;
            let pred_body = &raw[bra + 1..end];
            let name      = raw[..bra].to_string();
            (name, Some(pred_body.to_string()))
        } else {
            (raw.to_string(), None)
        };
        let attr_filter = match pred {
            Some(p) => {
                let p = p.trim();
                if let Some(eq) = p.find('=') {
                    let key = p[..eq].trim().trim_start_matches('@').to_string();
                    let val = p[eq + 1..].trim()
                        .trim_start_matches(['\'', '"'])
                        .trim_end_matches(['\'', '"'])
                        .to_string();
                    Some((key, val))
                } else {
                    return Err(format!("predicate '{p}' missing '='"));
                }
            }
            None => None,
        };
        steps.push(XPathStep { name, any_ancestor, attr_filter });
        any_ancestor = false;
    }
    if steps.is_empty() { return Err("no path steps".into()); }
    Ok((steps, attr_target))
}

/// True when the current `stack` (root → leaf element names) and `attrs`
/// match the xpath `steps`.
fn path_matches(
    steps: &[XPathStep],
    stack: &[String],
    attrs: &[Vec<(String, String)>],
) -> bool {
    // Anchor: walk steps left-to-right against stack, consuming one stack
    // element per step unless `any_ancestor = true` which allows skipping.
    if steps.is_empty() { return false; }
    // Start matching at stack[0].
    let mut si = 0usize;       // step index
    let mut xi = 0usize;       // stack index
    while si < steps.len() && xi < stack.len() {
        let step = &steps[si];
        // `//step` allows skipping ahead through the stack until we find a
        // matching element. Applies to the first step too (`//foo`).
        if step.any_ancestor {
            while xi < stack.len() && !step_matches(step, &stack[xi], &attrs[xi]) {
                xi += 1;
            }
            if xi == stack.len() { return false; }
        }
        if !step_matches(step, &stack[xi], &attrs[xi]) {
            return false;
        }
        si += 1;
        xi += 1;
    }
    // All steps matched AND we've consumed through the last stack element
    // (matching the LEAF — we don't want `/a/b` matching stack `[a,b,c]`).
    si == steps.len() && xi == stack.len()
}

fn step_matches(step: &XPathStep, name: &str, attrs: &[(String, String)]) -> bool {
    if !step.name.is_empty() && step.name != name { return false; }
    if let Some((k, v)) = &step.attr_filter {
        return attrs.iter().any(|(ak, av)| ak == k && av == v);
    }
    true
}
