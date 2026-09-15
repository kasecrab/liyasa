//! The six scored fields and their BM25 weights (SRC-03).

use serde::{Deserialize, Serialize};

/// The fields a term can occur in. `route`, `anchor`, `tab`, `version`,
/// `locale`, `type`, `groups`, and `regions` are filters and facets rather
/// than scored text, so they live in `docs-<n>.bin` and never in the postings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Field {
    Title,
    Section,
    Keywords,
    Breadcrumb,
    Body,
    Code,
}

impl Field {
    pub const ALL: [Field; 6] = [
        Field::Title,
        Field::Section,
        Field::Keywords,
        Field::Breadcrumb,
        Field::Body,
        Field::Code,
    ];

    /// SRC-03's weights: title 5, section 3, keywords 3, breadcrumb 2,
    /// body 1, code 0.5.
    pub const fn weight(self) -> f32 {
        match self {
            Field::Title => 5.0,
            Field::Section | Field::Keywords => 3.0,
            Field::Breadcrumb => 2.0,
            Field::Body => 1.0,
            Field::Code => 0.5,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Field::Title => "title",
            Field::Section => "section",
            Field::Keywords => "keywords",
            Field::Breadcrumb => "breadcrumb",
            Field::Body => "body",
            Field::Code => "code",
        }
    }

    pub const fn id(self) -> u8 {
        match self {
            Field::Title => 0,
            Field::Section => 1,
            Field::Keywords => 2,
            Field::Breadcrumb => 3,
            Field::Body => 4,
            Field::Code => 5,
        }
    }

    pub const fn from_id(id: u8) -> Option<Self> {
        Some(match id {
            0 => Field::Title,
            1 => Field::Section,
            2 => Field::Keywords,
            3 => Field::Breadcrumb,
            4 => Field::Body,
            5 => Field::Code,
            _ => return None,
        })
    }

    pub fn parse(name: &str) -> Option<Self> {
        Field::ALL.into_iter().find(|f| f.as_str() == name)
    }
}

/// Per-field values, indexed by [`Field::id`]. A fixed array rather than a map
/// because every field is always present and the reader is on a budget.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ByField<T>(pub [T; 6]);

impl<T> std::ops::Index<Field> for ByField<T> {
    type Output = T;

    fn index(&self, field: Field) -> &T {
        &self.0[field.id() as usize]
    }
}

impl<T> std::ops::IndexMut<Field> for ByField<T> {
    fn index_mut(&mut self, field: Field) -> &mut T {
        &mut self.0[field.id() as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_for_every_field() {
        for field in Field::ALL {
            assert_eq!(Field::from_id(field.id()), Some(field));
            assert_eq!(Field::parse(field.as_str()), Some(field));
        }
        assert_eq!(Field::from_id(6), None);
        assert_eq!(Field::parse("route"), None);
    }

    #[test]
    fn the_weights_are_the_ones_src_03_names() {
        assert_eq!(Field::Title.weight(), 5.0);
        assert_eq!(Field::Section.weight(), 3.0);
        assert_eq!(Field::Keywords.weight(), 3.0);
        assert_eq!(Field::Breadcrumb.weight(), 2.0);
        assert_eq!(Field::Body.weight(), 1.0);
        assert_eq!(Field::Code.weight(), 0.5);
    }
}
