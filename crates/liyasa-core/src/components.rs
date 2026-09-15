//! The component and theme contracts (PRD §34.9).
//!
//! `liyasa-components` re-exports these; every component implementer, the
//! editor, and the generated component documentation read the same descriptor.

use serde::{Deserialize, Serialize};

use crate::document::{Dep, Node, Origin, Props, Slots};
use crate::ids::BlockId;
use crate::markdown::ComponentKind;

pub type Value = serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", content = "of", rename_all = "camelCase")]
#[non_exhaustive]
pub enum PropType {
    Str,
    Num,
    Bool,
    Enum(Vec<String>),
    List(Box<PropType>),
    Route,
    Asset,
    Icon,
    Color,
    /// A template expression, kept unevaluated until expansion.
    Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropDef {
    pub name: &'static str,
    pub ty: PropType,
    pub required: bool,
    pub default: Option<Value>,
    pub doc: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SlotDef {
    pub name: &'static str,
    pub required: bool,
    pub doc: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropSchema {
    pub props: Vec<PropDef>,
    pub slots: Vec<SlotDef>,
}

impl PropSchema {
    pub fn prop(&self, name: &str) -> Option<&PropDef> {
        self.props.iter().find(|p| p.name == name)
    }

    pub fn required_props(&self) -> impl Iterator<Item = &PropDef> {
        self.props.iter().filter(|p| p.required)
    }
}

/// One occurrence of a component in a page.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentInst {
    pub name: String,
    pub props: Props,
    pub children: Vec<Node>,
    pub slots: Slots,
    pub id: BlockId,
    pub origin: Origin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Widget {
    Text,
    Number,
    Toggle,
    Select,
    Icon,
    Asset,
    Route,
    Color,
    Code,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FormField {
    pub prop: String,
    pub widget: Widget,
    pub label: String,
    pub help: String,
}

/// How the editor offers a component: form layout, icon, category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EditorBlock {
    pub icon: String,
    pub category: String,
    pub form: Vec<FormField>,
    pub inline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RenderError {
    #[error("component `{0}` failed to render")]
    Component(String),
    #[error("render budget exceeded")]
    Budget,
    #[error("sanitizer rejected output: {0}")]
    Sanitizer(String),
}

/// Rendering state a component is handed: the theme's partial renderer, asset
/// resolver, nonce, variant, and diagnostics sink.
///
/// Opaque by design; `liyasa-theme` and `liyasa-build` own the fields, so a
/// component implementer cannot reach past the methods they are given.
#[derive(Debug)]
#[non_exhaustive]
pub struct RenderCtx;

/// Markdown serialization state: audience, site metadata, indentation, and the
/// fence-length chooser.
#[derive(Debug)]
#[non_exhaustive]
pub struct MdCtx;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[non_exhaustive]
pub struct PageMeta;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[non_exhaustive]
pub struct NavCtx;

/// What a theme partial is handed.
#[derive(Debug, Clone)]
pub struct PartialCtx {
    pub page: PageMeta,
    pub nav: NavCtx,
    pub site: crate::markdown::SiteMeta,
    pub variant: crate::build::Variant,
    /// The CSP nonce for this response.
    pub nonce: String,
}

pub trait Component: Send + Sync {
    /// The directive name, kebab-case.
    fn name(&self) -> &'static str;
    /// Tag-form names.
    fn aliases(&self) -> &'static [&'static str];
    fn schema(&self) -> &PropSchema;
    fn kind(&self) -> ComponentKind;
    fn render_html(&self, inst: &ComponentInst, ctx: &mut RenderCtx) -> Result<(), RenderError>;
    fn render_markdown(&self, inst: &ComponentInst, ctx: &mut MdCtx) -> Result<(), RenderError>;
    /// Plain text, for the search index.
    fn render_text(&self, inst: &ComponentInst) -> String;
    fn editor_block(&self) -> EditorBlock;
    /// Facts, assets, and links this instance depends on.
    fn deps(&self, inst: &ComponentInst) -> Vec<Dep>;
}

pub trait ComponentRegistry: Send + Sync {
    fn get(&self, name: &str) -> Option<&dyn Component>;
    fn names(&self) -> Vec<&str>;
}

/// The theme side of `render_html`: partials receive typed context and return
/// HTML fragments.
pub trait Renderer: Send + Sync {
    fn partial(&self, name: &str, ctx: &PartialCtx) -> Result<String, RenderError>;
    fn component_html(
        &self,
        inst: &ComponentInst,
        children: &str,
        ctx: &mut RenderCtx,
    ) -> Result<String, RenderError>;
    fn code_block(
        &self,
        block: &crate::document::Block,
        highlighted: &str,
        ctx: &mut RenderCtx,
    ) -> Result<String, RenderError>;
}
