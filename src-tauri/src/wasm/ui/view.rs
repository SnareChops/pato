//! Wire types for the core -> webview calls (`widgetRegister`, `widgetUpdate`,
//! `widgetSetValue`, `widgetRemove`). These mirror `pato:internal/widget-view`
//! and `pato:internal/widget-dom` in `wit-internal/core-ui.wit` — the source
//! jco reads to type the JS side (see `src-ui/generated/`).
//!
//! serde is configured to emit exactly the component-model JSON shape jco
//! generates: field-less enums as kebab-case strings, `variant`s as
//! adjacently-tagged `{ tag, val }` objects, records with camelCase keys and
//! omitted `option` fields. wasmtime's bindgen cannot produce this encoding,
//! so this stays hand-written — kept honest by `wire_encoding_matches_jco`
//! below and by the exhaustive `From` conversions off the bindgen types.

use serde::Serialize;

use super::plugin::pato::plugin::widget_dom as dom;

/// Generates `From<$src>` for a pair of structurally identical field-less
/// enums (bindgen type -> wire type).
macro_rules! enum_from {
    ($src:path => $dst:ident { $($variant:ident),+ $(,)? }) => {
        impl From<$src> for $dst {
            fn from(v: $src) -> Self {
                match v { $( <$src>::$variant => Self::$variant ),+ }
            }
        }
    };
}

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Tag {
    Div, Span, Section, Header, Footer, Nav, Ul, Ol, Li,
    P, H1, H2, H3, H4, H5, H6, Strong, Em, Small, Code, Pre, Blockquote,
    Button, Input, Select, Option, Textarea, Label, A, Img,
    Table, Thead, Tbody, Tr, Th, Td, Progress, Meter,
}
enum_from!(dom::Tag => Tag {
    Div, Span, Section, Header, Footer, Nav, Ul, Ol, Li,
    P, H1, H2, H3, H4, H5, H6, Strong, Em, Small, Code, Pre, Blockquote,
    Button, Input, Select, Option, Textarea, Label, A, Img,
    Table, Thead, Tbody, Tr, Th, Td, Progress, Meter,
});

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum InputType { Text, Number, Checkbox, Radio }
enum_from!(dom::InputType => InputType { Text, Number, Checkbox, Radio });

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Layout { FlexRow, FlexCol }
enum_from!(dom::Layout => Layout { FlexRow, FlexCol });

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Space { None, Xs, Sm, Md, Lg, Xl }
enum_from!(dom::Space => Space { None, Xs, Sm, Md, Lg, Xl });

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum TextSize { Xs, Sm, Md, Lg, Xl, Xxl }
enum_from!(dom::TextSize => TextSize { Xs, Sm, Md, Lg, Xl, Xxl });

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Align { Start, Center, End, Stretch }
enum_from!(dom::Align => Align { Start, Center, End, Stretch });

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Justify { Start, Center, End, Between, Around }
enum_from!(dom::Justify => Justify { Start, Center, End, Between, Around });

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeColor {
    Fg, FgMuted, Bg, BgRaised, Border,
    Primary, Success, Warning, Danger, Info,
}
enum_from!(dom::ThemeColor => ThemeColor {
    Fg, FgMuted, Bg, BgRaised, Border, Primary, Success, Warning, Danger, Info,
});

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum EventKind { Click, Input, Change, Submit, Focus, Blur, EnterKey }
enum_from!(dom::EventKind => EventKind {
    Click, Input, Change, Submit, Focus, Blur, EnterKey,
});

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(tag = "tag", content = "val", rename_all = "kebab-case")]
pub enum Size {
    Auto,
    Fill,
    Percent(u32),
    Fixed(Space),
}
impl From<dom::Size> for Size {
    fn from(v: dom::Size) -> Self {
        match v {
            dom::Size::Auto => Size::Auto,
            dom::Size::Fill => Size::Fill,
            dom::Size::Percent(p) => Size::Percent(p),
            dom::Size::Fixed(s) => Size::Fixed(s.into()),
        }
    }
}

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(tag = "tag", content = "val", rename_all = "kebab-case")]
pub enum Attr {
    Href(String),
    Src(String),
    Alt(String),
    Title(String),
    Name(String),
    Placeholder(String),
    Value(String),
    Kind(InputType),
    Checked(bool),
    Disabled(bool),
    Rows(u32),
    Min(f64),
    Max(f64),
    Step(f64),
}

#[derive(Serialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Style {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<Layout>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<Space>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding: Option<Space>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin: Option<Space>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius: Option<Space>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ThemeColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<ThemeColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<TextSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub justify: Option<Justify>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grow: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<Size>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<Size>,
}
impl From<dom::Style> for Style {
    fn from(s: dom::Style) -> Self {
        Style {
            layout: s.layout.map(Into::into),
            gap: s.gap.map(Into::into),
            padding: s.padding.map(Into::into),
            margin: s.margin.map(Into::into),
            radius: s.radius.map(Into::into),
            color: s.color.map(Into::into),
            background: s.background.map(Into::into),
            font_size: s.font_size.map(Into::into),
            align: s.align.map(Into::into),
            justify: s.justify.map(Into::into),
            grow: s.grow,
            wrap: s.wrap,
            width: s.width.map(Into::into),
            height: s.height.map(Into::into),
        }
    }
}

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub kind: EventKind,
    pub handler: String,
}
impl From<dom::Binding> for Binding {
    fn from(b: dom::Binding) -> Self {
        Binding { kind: b.kind.into(), handler: b.handler }
    }
}

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub tag: Tag,
    pub attrs: Vec<Attr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<Style>,
    pub events: Vec<Binding>,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(tag = "tag", content = "val", rename_all = "kebab-case")]
pub enum NodeKind {
    Element(Element),
    Text(String),
    ValueSlot(String),
}

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<u32>,
    pub kind: NodeKind,
}

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TileSize {
    pub w: u32,
    pub h: u32,
}
impl From<dom::TileSize> for TileSize {
    fn from(t: dom::TileSize) -> Self {
        TileSize { w: t.w, h: t.h }
    }
}

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WidgetSpec {
    pub id: String,
    pub size: TileSize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<TileSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<TileSize>,
}

// --- status-widget (pre-made) ------------------------------------------

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Action {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct StatusWidget {
    pub id: String,
    pub icon: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    pub actions: Vec<Action>,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CustomWidget {
    pub id: String,
    pub nodes: Vec<Node>,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(tag = "tag", content = "val", rename_all = "kebab-case")]
pub enum Widget {
    StatusWidget(StatusWidget),
    Custom(CustomWidget),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards that `view::*` serializes to the component-model JSON shape jco
    /// generates for `pato:internal/widget-view` (see `src-ui/generated/`). If
    /// this drifts, the typed JS side receives a shape it can't read.
    #[test]
    fn wire_encoding_matches_jco() {
        let status = Widget::StatusWidget(StatusWidget {
            id: "twitch|live".into(),
            icon: "i".into(),
            label: "l".into(),
            tooltip: None,
            actions: vec![Action {
                id: "a".into(),
                label: "Go".into(),
                tooltip: Some("t".into()),
            }],
        });
        assert_eq!(
            serde_json::to_value(&status).unwrap(),
            serde_json::json!({
                "tag": "status-widget",
                "val": {
                    "id": "twitch|live",
                    "icon": "i",
                    "label": "l",
                    "actions": [{ "id": "a", "label": "Go", "tooltip": "t" }],
                }
            })
        );

        let custom = Widget::Custom(CustomWidget {
            id: "twitch|panel".into(),
            nodes: vec![
                Node {
                    parent: None,
                    kind: NodeKind::Element(Element {
                        key: None,
                        tag: Tag::Div,
                        attrs: vec![],
                        style: Some(Style {
                            layout: Some(Layout::FlexCol),
                            font_size: Some(TextSize::Lg),
                            color: Some(ThemeColor::FgMuted),
                            ..Default::default()
                        }),
                        events: vec![],
                    }),
                },
                Node {
                    parent: Some(0),
                    kind: NodeKind::Element(Element {
                        key: Some("twitch|btn".into()),
                        tag: Tag::Button,
                        attrs: vec![Attr::Disabled(false)],
                        style: None,
                        events: vec![Binding {
                            kind: EventKind::Click,
                            handler: "refresh".into(),
                        }],
                    }),
                },
                Node { parent: Some(1), kind: NodeKind::Text("Refresh".into()) },
                Node { parent: Some(0), kind: NodeKind::ValueSlot("count".into()) },
            ],
        });
        assert_eq!(
            serde_json::to_value(&custom).unwrap(),
            serde_json::json!({
                "tag": "custom",
                "val": {
                    "id": "twitch|panel",
                    "nodes": [
                        { "kind": { "tag": "element", "val": {
                            "tag": "div", "attrs": [], "events": [],
                            "style": { "layout": "flex-col", "fontSize": "lg", "color": "fg-muted" }
                        } } },
                        { "parent": 0, "kind": { "tag": "element", "val": {
                            "key": "twitch|btn", "tag": "button",
                            "attrs": [{ "tag": "disabled", "val": false }],
                            "events": [{ "kind": "click", "handler": "refresh" }]
                        } } },
                        { "parent": 1, "kind": { "tag": "text", "val": "Refresh" } },
                        { "parent": 0, "kind": { "tag": "value-slot", "val": "count" } }
                    ]
                }
            })
        );
    }
}
