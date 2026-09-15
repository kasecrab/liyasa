//! One module per component, named after its directive (PRD §31.5).

pub mod callout;
pub mod card;

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
        .add(card::Tile);
}
