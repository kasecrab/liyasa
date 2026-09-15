//! One module per component, named after its directive (PRD §31.5).

pub mod api;
pub mod callout;
pub mod card;
pub mod code;
pub mod disclosure;
pub mod frame;
pub mod inline;
pub mod media;
pub mod page;
pub mod steps;
pub mod tabs;
pub mod tree;

use crate::registry::Registry;

/// Registers every built-in component. Order is irrelevant except that a later
/// registration of the same name wins, which is how overrides work (CMP-94).
pub fn register_builtins(registry: &mut Registry) {
    registry
        .add(callout::Note)
        .add(callout::Tip)
        .add(callout::Warning)
        .add(callout::Info)
        .add(callout::Check)
        .add(callout::Danger)
        .add(callout::Callout)
        .add(card::Card)
        .add(card::Cards)
        .add(card::Columns)
        .add(card::Column)
        .add(card::Tiles)
        .add(card::Tile)
        .add(frame::Frame)
        .add(frame::Panel)
        .add(frame::Hero)
        .add(frame::Divider)
        .add(disclosure::Accordion)
        .add(disclosure::Accordions)
        .add(disclosure::Expandable)
        .add(disclosure::Expandables)
        .add(tabs::Tabs)
        .add(tabs::Tab)
        .add(steps::Steps)
        .add(steps::Step)
        .add(tree::Tree)
        .add(tree::Toc)
        .add(code::CodeGroup)
        .add(code::Code)
        .add(code::Terminal)
        .add(code::SnippetFrom)
        .add(api::Param)
        .add(api::ResponseField)
        .add(api::RequestExample)
        .add(api::ResponseExample)
        .add(api::Endpoint)
        .add(api::OpenapiSchema)
        .add(media::Image)
        .add(media::Video)
        .add(media::IFrame)
        .add(media::Embed)
        .add(media::File)
        .add(media::Files)
        .add(media::Screenshot)
        .add(inline::Badge)
        .add(inline::Color)
        .add(inline::Icon)
        .add(inline::Tooltip)
        .add(inline::Kbd)
        .add(inline::Fact)
        .add(page::Banner)
        .add(page::Update)
        .add(page::Prompt)
        .add(page::Github)
        .add(page::Md)
        .add(page::Visibility)
        .add(page::Region)
        .add(page::Feedback)
        .add(page::Assistant);
}
