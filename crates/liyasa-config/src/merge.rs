//! Deep-merging one config document over another (CFG-93).
//!
//! Objects merge key by key; anything else replaces wholesale, so an array in
//! an overlay is the new array rather than an addition to the old one. Spans
//! follow the value: whichever file last wrote a key is the file its
//! diagnostics point at.

use serde_json::Value;

use crate::json::SpanIndex;

pub fn merge(
    base: &mut Value,
    base_spans: &mut SpanIndex,
    overlay: &Value,
    overlay_spans: &SpanIndex,
) {
    merge_at(base, base_spans, overlay, overlay_spans, &mut String::new());
}

fn merge_at(
    base: &mut Value,
    base_spans: &mut SpanIndex,
    overlay: &Value,
    overlay_spans: &SpanIndex,
    pointer: &mut String,
) {
    let (Value::Object(into), Value::Object(from)) = (&mut *base, overlay) else {
        *base = overlay.clone();
        base_spans.graft(pointer, overlay_spans);
        return;
    };
    let len = pointer.len();
    for (key, value) in from {
        pointer.push('/');
        pointer.push_str(&escape(key));
        match into.get_mut(key) {
            Some(existing) => merge_at(existing, base_spans, value, overlay_spans, pointer),
            None => {
                into.insert(key.clone(), value.clone());
                base_spans.graft(pointer, overlay_spans);
            }
        }
        pointer.truncate(len);
    }
}

fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}
