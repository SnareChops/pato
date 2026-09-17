//! Validation + normalisation of a plugin-supplied custom-widget node table.
//!
//! The core is the trust boundary: a plugin cannot express an invalid `tag`
//! (it's a WIT enum), but it can still send a malformed tree, oversized
//! payloads, attributes that don't belong on their element, or hostile URLs.
//! [`tree`] rejects structurally broken / oversized trees outright and quietly
//! drops individual bad attributes, returning a clean [`view::Node`] list ready
//! to serialize to the webview.

use super::plugin::pato::plugin::widget_dom as dom;
use super::view;

/// Hard ceiling on nodes in one widget tree.
const MAX_NODES: usize = 4096;
/// Hard ceiling on the length (bytes) of any single string in the tree.
const MAX_STR: usize = 64 * 1024;
/// Hard ceiling on tree depth (root = depth 1).
const MAX_DEPTH: usize = 32;
/// Hard ceiling on event bindings per element.
const MAX_EVENTS: usize = 16;

/// Validate and normalise a flat node table. `plugin` is the owning plugin's
/// name, used to scope `pato-asset:` URLs.
pub fn tree(nodes: Vec<dom::Node>, plugin: &str) -> Result<Vec<view::Node>, String> {
    if nodes.is_empty() {
        return Err("custom widget has no nodes".into());
    }
    if nodes.len() > MAX_NODES {
        return Err(format!(
            "custom widget has {} nodes (max {MAX_NODES})",
            nodes.len()
        ));
    }

    // Structural pass: parents point backwards, only node 0 is a root, and
    // every parent is an element.
    let is_element: Vec<bool> = nodes
        .iter()
        .map(|n| matches!(n.kind, dom::NodeKind::Element(_)))
        .collect();
    for (i, node) in nodes.iter().enumerate() {
        match (i, node.parent) {
            (0, None) => {}
            (0, Some(_)) => return Err("node 0 (root) must have no parent".into()),
            (_, None) => return Err(format!("node {i} has no parent (only node 0 may)")),
            (_, Some(p)) => {
                let p = p as usize;
                if p >= i {
                    return Err(format!("node {i} parent {p} is not a lower index"));
                }
                if !is_element[p] {
                    return Err(format!("node {i} parent {p} is not an element"));
                }
            }
        }
    }
    if !is_element[0] {
        return Err("node 0 (root) must be an element".into());
    }

    // Depth pass (parents already proven to point backwards, so this is O(n)).
    let mut depth = vec![0usize; nodes.len()];
    depth[0] = 1;
    for i in 1..nodes.len() {
        let p = nodes[i].parent.unwrap() as usize;
        depth[i] = depth[p] + 1;
        if depth[i] > MAX_DEPTH {
            return Err(format!("custom widget tree exceeds depth {MAX_DEPTH}"));
        }
    }

    let mut out = Vec::with_capacity(nodes.len());
    for (i, node) in nodes.into_iter().enumerate() {
        let kind = match node.kind {
            dom::NodeKind::Text(s) => {
                check_str(&s, i)?;
                view::NodeKind::Text(s)
            }
            dom::NodeKind::ValueSlot(s) => {
                check_str(&s, i)?;
                view::NodeKind::ValueSlot(s)
            }
            dom::NodeKind::Element(el) => view::NodeKind::Element(element(el, i, plugin)?),
        };
        out.push(view::Node {
            parent: node.parent,
            kind,
        });
    }
    Ok(out)
}

fn check_str(s: &str, node: usize) -> Result<(), String> {
    if s.len() > MAX_STR {
        return Err(format!(
            "node {node} has a string longer than {MAX_STR} bytes"
        ));
    }
    Ok(())
}

fn element(el: dom::Element, node: usize, plugin: &str) -> Result<view::Element, String> {
    let tag = view::Tag::from(el.tag);

    if let Some(key) = &el.key {
        check_str(key, node)?;
    }

    let mut attrs = Vec::with_capacity(el.attrs.len());
    for attr in el.attrs {
        if let Some(a) = attribute(attr, tag, node, plugin)? {
            attrs.push(a);
        }
    }

    if el.events.len() > MAX_EVENTS {
        return Err(format!(
            "node {node} has {} event bindings (max {MAX_EVENTS})",
            el.events.len()
        ));
    }
    let mut events = Vec::with_capacity(el.events.len());
    for b in el.events {
        check_str(&b.handler, node)?;
        if b.handler.is_empty() {
            continue; // an unaddressable binding is dead weight; drop it
        }
        events.push(view::Binding::from(b));
    }

    Ok(view::Element {
        key: el.key,
        tag,
        attrs,
        style: el.style.map(view::Style::from),
        events,
    })
}

/// Returns `Ok(Some(_))` to keep the attribute, `Ok(None)` to drop it, `Err`
/// only for a cap violation.
fn attribute(
    attr: dom::Attr,
    tag: view::Tag,
    node: usize,
    plugin: &str,
) -> Result<Option<view::Attr>, String> {
    use view::Tag::*;
    let form_control = matches!(tag, Input | Select | Textarea | Button | Option);

    let kept = match attr {
        dom::Attr::Href(u) => {
            check_str(&u, node)?;
            (tag == A)
                .then(|| url(&u, plugin))
                .flatten()
                .map(view::Attr::Href)
        }
        dom::Attr::Src(u) => {
            check_str(&u, node)?;
            (tag == Img)
                .then(|| url(&u, plugin))
                .flatten()
                .map(view::Attr::Src)
        }
        dom::Attr::Alt(s) => {
            check_str(&s, node)?;
            (tag == Img).then_some(view::Attr::Alt(s))
        }
        dom::Attr::Title(s) => {
            check_str(&s, node)?;
            Some(view::Attr::Title(s))
        }
        dom::Attr::Name(s) => {
            check_str(&s, node)?;
            (form_control || tag == A).then_some(view::Attr::Name(s))
        }
        dom::Attr::Placeholder(s) => {
            check_str(&s, node)?;
            matches!(tag, Input | Textarea).then_some(view::Attr::Placeholder(s))
        }
        dom::Attr::Value(s) => {
            check_str(&s, node)?;
            matches!(tag, Input | Textarea | Option).then_some(view::Attr::Value(s))
        }
        dom::Attr::Kind(k) => (tag == Input).then_some(view::Attr::Kind(k.into())),
        dom::Attr::Checked(b) => (tag == Input).then_some(view::Attr::Checked(b)),
        dom::Attr::Disabled(b) => form_control.then_some(view::Attr::Disabled(b)),
        dom::Attr::Rows(n) => (tag == Textarea).then_some(view::Attr::Rows(n)),
        dom::Attr::Min(v) => matches!(tag, Input | Progress | Meter).then_some(view::Attr::Min(v)),
        dom::Attr::Max(v) => matches!(tag, Input | Progress | Meter).then_some(view::Attr::Max(v)),
        dom::Attr::Step(v) => (tag == Input).then_some(view::Attr::Step(v)),
    };
    Ok(kept)
}

/// Accepts `https:` URLs unchanged and `pato-asset:` URLs after scoping them to
/// the owning plugin. Everything else (javascript:, data:, file:, ...) is
/// dropped.
fn url(raw: &str, plugin: &str) -> Option<String> {
    let raw = raw.trim();
    if let Some(rest) = raw
        .strip_prefix("pato-asset://")
        .or_else(|| raw.strip_prefix("pato-asset:/"))
        .or_else(|| raw.strip_prefix("pato-asset:"))
    {
        let path = rest.trim_start_matches('/');
        if path.is_empty()
            || path
                .split('/')
                .any(|seg| seg == ".." || seg == "." || seg.is_empty())
        {
            return None;
        }
        return Some(format!("pato-asset://{plugin}/{path}"));
    }
    if raw.starts_with("https://") {
        return Some(raw.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(parent: Option<u32>, tag: dom::Tag) -> dom::Node {
        dom::Node {
            parent,
            kind: dom::NodeKind::Element(dom::Element {
                key: None,
                tag,
                attrs: vec![],
                style: None,
                events: vec![],
            }),
        }
    }

    #[test]
    fn rejects_forward_parent() {
        let nodes = vec![
            el(None, dom::Tag::Div),
            el(Some(2), dom::Tag::Span),
            el(Some(0), dom::Tag::Span),
        ];
        assert!(tree(nodes, "p").is_err());
    }

    #[test]
    fn rejects_second_root() {
        let nodes = vec![el(None, dom::Tag::Div), el(None, dom::Tag::Div)];
        assert!(tree(nodes, "p").is_err());
    }

    #[test]
    fn drops_attr_on_wrong_tag_and_bad_url() {
        let nodes = vec![dom::Node {
            parent: None,
            kind: dom::NodeKind::Element(dom::Element {
                key: None,
                tag: dom::Tag::Div,
                attrs: vec![
                    dom::Attr::Href("https://example.com".into()), // href not valid on div
                    dom::Attr::Title("ok".into()),
                ],
                style: None,
                events: vec![],
            }),
        }];
        let out = tree(nodes, "p").unwrap();
        let view::NodeKind::Element(root) = &out[0].kind else {
            panic!()
        };
        assert_eq!(root.attrs, vec![view::Attr::Title("ok".into())]);
    }

    #[test]
    fn scopes_pato_asset_urls() {
        assert_eq!(
            url("pato-asset:icon.png", "twitch").as_deref(),
            Some("pato-asset://twitch/icon.png")
        );
        assert_eq!(
            url("pato-asset://icon.png", "twitch").as_deref(),
            Some("pato-asset://twitch/icon.png")
        );
        assert_eq!(url("pato-asset:../secret", "twitch"), None);
        assert_eq!(url("javascript:alert(1)", "twitch"), None);
        assert_eq!(
            url("https://cdn.example/x.png", "twitch").as_deref(),
            Some("https://cdn.example/x.png")
        );
    }
}
