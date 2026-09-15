//! Declaring a component: its schema, its editor block, and the frozen
//! `Component` impl that carries them.
//!
//! Sixty components share one shape — a name, aliases, a kind, a prop table,
//! and an editor form generated from that table — so the shape is written once
//! here and each component declares only what differs.

use liyasa_core::components::{
    EditorBlock, FormField, PropDef, PropSchema, PropType, SlotDef, Value, Widget,
};

pub fn text(value: &str) -> Value {
    Value::String(value.to_owned())
}

pub fn num(value: i64) -> Value {
    Value::from(value)
}

pub fn yes() -> Value {
    Value::Bool(true)
}

pub fn no() -> Value {
    Value::Bool(false)
}

pub fn one_of(values: &[&str]) -> PropType {
    PropType::Enum(values.iter().map(|v| (*v).to_owned()).collect())
}

pub fn list_of(item: PropType) -> PropType {
    PropType::List(Box::new(item))
}

/// `maxLines` becomes "Max lines", `card-group` becomes "Card group".
pub fn label_of(prop: &str) -> String {
    let mut out = String::with_capacity(prop.len() + 4);
    for (at, ch) in prop.char_indices() {
        if ch == '-' || ch == '_' {
            out.push(' ');
        } else if ch.is_uppercase() && at > 0 {
            out.push(' ');
            out.extend(ch.to_lowercase());
        } else if at == 0 {
            out.extend(ch.to_uppercase());
        } else {
            out.push(ch);
        }
    }
    out
}

pub fn build(props: Vec<PropDef>, slots: Vec<SlotDef>) -> PropSchema {
    PropSchema { props, slots }
}

/// The editor form for a schema: one field per prop, in declaration order.
pub fn editor(icon: &str, category: &str, inline: bool, schema: &PropSchema) -> EditorBlock {
    EditorBlock {
        icon: icon.to_owned(),
        category: category.to_owned(),
        inline,
        form: schema
            .props
            .iter()
            .map(|def| FormField {
                prop: def.name.to_owned(),
                widget: widget_for(&def.ty),
                label: label_of(def.name),
                help: def.doc.to_owned(),
            })
            .collect(),
    }
}

/// The form control a prop type asks for. One mapping, so a new prop never
/// needs a widget chosen by hand.
pub fn widget_for(ty: &PropType) -> Widget {
    match ty {
        PropType::Num => Widget::Number,
        PropType::Bool => Widget::Toggle,
        PropType::Enum(_) => Widget::Select,
        PropType::Icon => Widget::Icon,
        PropType::Asset => Widget::Asset,
        PropType::Route => Widget::Route,
        PropType::Color => Widget::Color,
        PropType::Expr => Widget::Code,
        PropType::List(item) => match widget_for(item) {
            Widget::Toggle | Widget::Number => Widget::Text,
            other => other,
        },
        _ => Widget::Text,
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! __component_deps {
    ($inst:expr, $schema:expr) => {
        $crate::deps::from_schema($inst, $schema)
    };
    ($inst:expr, $schema:expr, $extract:path) => {
        $extract($inst)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __prop_required {
    (Required) => {
        true
    };
    (Optional) => {
        false
    };
    (Default) => {
        false
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __prop_default {
    (Required) => {
        None
    };
    (Optional) => {
        None
    };
    (Default, $default:expr) => {
        Some($default)
    };
}

/// Declares a component type, its schema, its editor block, and the frozen
/// `Component` impl that forwards rendering to [`crate::render::Render`].
///
/// ```ignore
/// declare! {
///     /// A clickable card.
///     pub struct Card;
///     name = "card";
///     aliases = ["Card"];
///     kind = Container;
///     editor = ("square", "Layout");
///     props = [
///         ("title", PropType::Str, Optional, "Card heading."),
///         ("cols",  PropType::Num, Default(num(2)), "Columns, 1 to 4."),
///     ];
/// }
/// ```
#[macro_export]
macro_rules! declare {
    (
        $(#[$meta:meta])*
        $vis:vis struct $type:ident;
        name = $name:literal;
        $(aliases = [$($alias:literal),* $(,)?];)?
        kind = $kind:ident;
        editor = ($icon:literal, $category:literal);
        props = [ $( ($prop:literal, $ty:expr, $flag:ident $(($default:expr))?, $doc:literal) ),* $(,)? ];
        $(slots = [ $( ($slot:literal, $slot_required:literal, $slot_doc:literal) ),* $(,)? ];)?
        $(deps = $extract:path;)?
    ) => {
        $(#[$meta])*
        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
        $vis struct $type;

        impl $type {
            $vis fn schema_of() -> &'static $crate::PropSchema {
                static SCHEMA: ::std::sync::OnceLock<$crate::PropSchema> =
                    ::std::sync::OnceLock::new();
                SCHEMA.get_or_init(|| $crate::schema::build(
                    ::std::vec![$(
                        $crate::PropDef {
                            name: $prop,
                            ty: $ty,
                            required: $crate::__prop_required!($flag),
                            default: $crate::__prop_default!($flag $(, $default)?),
                            doc: $doc,
                        }
                    ),*],
                    ::std::vec![$($(
                        $crate::SlotDef {
                            name: $slot,
                            required: $slot_required,
                            doc: $slot_doc,
                        }
                    ),*)?],
                ))
            }
        }

        impl $crate::Component for $type {
            fn name(&self) -> &'static str {
                $name
            }

            fn aliases(&self) -> &'static [&'static str] {
                &[$($($alias),*)?]
            }

            fn schema(&self) -> &$crate::PropSchema {
                Self::schema_of()
            }

            fn kind(&self) -> $crate::ComponentKind {
                $crate::ComponentKind::$kind
            }

            fn render_html(
                &self,
                inst: &$crate::ComponentInst,
                _ctx: &mut ::liyasa_core::components::RenderCtx,
            ) -> ::std::result::Result<(), $crate::RenderError> {
                // TODO(rfc-0030): `RenderCtx` has no sink, so the markup is
                // built and dropped. Unreachable in practice: no crate outside
                // `liyasa-core` can construct the `RenderCtx` this needs.
                let mut scratch = $crate::render::HtmlCtx::detached();
                $crate::render::Render::html(self, inst, &mut scratch)
            }

            fn render_markdown(
                &self,
                inst: &$crate::ComponentInst,
                _ctx: &mut ::liyasa_core::components::MdCtx,
            ) -> ::std::result::Result<(), $crate::RenderError> {
                // TODO(rfc-0030): as above.
                let mut scratch = $crate::render::MarkdownCtx::detached();
                $crate::render::Render::markdown(self, inst, &mut scratch)
            }

            fn render_text(&self, inst: &$crate::ComponentInst) -> ::std::string::String {
                $crate::render::Render::text(self, inst)
            }

            fn editor_block(&self) -> $crate::EditorBlock {
                $crate::schema::editor(
                    $icon,
                    $category,
                    ::core::matches!(
                        $crate::ComponentKind::$kind,
                        $crate::ComponentKind::Inline
                    ),
                    Self::schema_of(),
                )
            }

            fn deps(&self, inst: &$crate::ComponentInst) -> ::std::vec::Vec<::liyasa_core::document::Dep> {
                $crate::__component_deps!(inst, Self::schema_of() $(, $extract)?)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_split_on_case_and_hyphen() {
        assert_eq!(label_of("maxLines"), "Max lines");
        assert_eq!(label_of("card-group"), "Card group");
        assert_eq!(label_of("title"), "Title");
        assert_eq!(label_of("href"), "Href");
    }

    #[test]
    fn a_list_of_toggles_is_still_a_text_field() {
        assert_eq!(widget_for(&list_of(PropType::Bool)), Widget::Text);
        assert_eq!(widget_for(&list_of(PropType::Icon)), Widget::Icon);
    }
}
