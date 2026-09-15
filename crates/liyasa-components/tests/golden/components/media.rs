//! CMP-50 to CMP-55: images, video, frames, embeds, downloads, screenshots.

use liyasa_components::inst;
use liyasa_core::document::PropValue;

use crate::support::Gallery;

fn str(value: &str) -> PropValue {
    PropValue::Str(value.to_owned())
}

#[test]
fn image() {
    Gallery::new("image")
        .case(
            "default",
            inst::new("image")
                .prop("src", str("/img/dashboard.png"))
                .prop("alt", str("The dashboard, showing three builds"))
                .build(),
        )
        .case(
            "every prop",
            inst::new("image")
                .prop("src", str("/img/dashboard.png"))
                .prop("alt", str("The dashboard"))
                .prop("dark", str("/img/dashboard-dark.png"))
                .prop("width", PropValue::Num(1280.0))
                .prop("height", PropValue::Num(720.0))
                .prop("caption", str("The build dashboard"))
                .prop("zoom", PropValue::Bool(false))
                .prop("align", str("full"))
                .prop("border", PropValue::Bool(true))
                .build(),
        )
        .case("missing required props", inst::new("image").build())
        .case(
            "empty alt",
            inst::new("image")
                .prop("src", str("/img/divider.svg"))
                .prop("alt", str("  "))
                .build(),
        )
        .case(
            "one dimension",
            inst::new("image")
                .prop("src", str("/img/a.png"))
                .prop("alt", str("A"))
                .prop("width", PropValue::Num(800.0))
                .build(),
        )
        .case(
            "unsafe src",
            inst::new("image")
                .prop("src", str("javascript:alert(1)"))
                .prop("alt", str("Nope"))
                .build(),
        )
        .check();
}

#[test]
fn video() {
    Gallery::new("video")
        .case(
            "local",
            inst::new("video")
                .prop("src", str("/media/tour.mp4"))
                .prop("poster", str("/media/tour.png"))
                .prop("caption", str("A tour"))
                .build(),
        )
        .case(
            "every prop",
            inst::new("video")
                .prop("src", str("/media/tour.mp4"))
                .prop("poster", str("/media/tour.png"))
                .prop("autoplay", PropValue::Bool(true))
                .prop("loop", PropValue::Bool(true))
                .prop("muted", PropValue::Bool(false))
                .prop("controls", PropValue::Bool(false))
                .prop("caption", str("A tour"))
                .prop("title", str("Product tour"))
                .build(),
        )
        .case(
            "hosted",
            inst::new("video")
                .prop("src", str("https://www.youtube.com/watch?v=dQw4w9WgXcQ"))
                .prop("title", str("The announcement"))
                .build(),
        )
        .case("missing required prop", inst::new("video").build())
        .check();
}

#[test]
fn iframe() {
    Gallery::new("iframe")
        .case(
            "default",
            inst::new("iframe")
                .prop("src", str("https://example.com/demo"))
                .prop("title", str("The live demo"))
                .build(),
        )
        .case(
            "every prop",
            inst::new("iframe")
                .prop("src", str("https://example.com/demo"))
                .prop("title", str("The live demo"))
                .prop("height", str("480px"))
                .prop("allow", str("clipboard-write"))
                .build(),
        )
        .case("missing required props", inst::new("iframe").build())
        .check();
}

#[test]
fn embed() {
    Gallery::new("embed")
        .case(
            "youtube",
            inst::new("embed")
                .prop("url", str("https://www.youtube.com/watch?v=dQw4w9WgXcQ"))
                .build(),
        )
        .case(
            "every prop",
            inst::new("embed")
                .prop("url", str("https://www.figma.com/design/abc/Title"))
                .prop("title", str("The design file"))
                .prop("height", str("600px"))
                .build(),
        )
        .case(
            "unlisted provider",
            inst::new("embed")
                .prop("url", str("https://evil.example.com/embed/1"))
                .build(),
        )
        .check();
}

#[test]
fn file() {
    Gallery::new("file")
        .case(
            "default",
            inst::new("file").prop("src", str("/dl/report.pdf")).build(),
        )
        .case(
            "every prop",
            inst::new("file")
                .prop("src", str("/dl/report.pdf"))
                .prop("name", str("Annual report"))
                .prop("size", str("2.4 MB"))
                .prop("type", str("PDF"))
                .build(),
        )
        .case(
            "group",
            inst::new("files")
                .child(inst::nested(
                    inst::new("file")
                        .prop("src", str("/dl/a.pdf"))
                        .prop("size", str("1 MB")),
                ))
                .child(inst::nested(
                    inst::new("file").prop("src", str("/dl/b.zip")),
                ))
                .build(),
        )
        .check();
}

#[test]
fn screenshot() {
    Gallery::new("screenshot")
        .case(
            "default",
            inst::new("screenshot")
                .prop("src", str("/shots/builds.png"))
                .prop("alt", str("The builds list"))
                .build(),
        )
        .case(
            "every prop",
            inst::new("screenshot")
                .prop("src", str("/shots/builds.png"))
                .prop("alt", str("The builds list"))
                .prop("app", str("console"))
                .prop("route", str("/builds"))
                .prop("selector", str(".builds-table"))
                .prop("viewport", str("1280x800"))
                .prop("caption", str("Three builds, one failing"))
                .build(),
        )
        .check();
}
