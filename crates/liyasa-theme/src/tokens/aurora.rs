//! The `aurora` preset's token table (THM-01).
//!
//! The design system in one place: an 8-point spacing scale, a fluid type
//! scale, a 12-step neutral ramp per scheme, semantic colour roles built on
//! that ramp, and motion tokens no slower than 200 ms (THM-05).
//!
//! Every colour pair a reader can see is listed in [`AA_PAIRS`] and checked by
//! `tests/thm_02_contrast.rs`, so a ramp cannot be retuned into a failure
//! without a test failing.

use super::{
    Group::{Border, Code, Color, Font, Layout, Motion, Radius, Shadow, Space, Text, ZIndex},
    Spec,
    SpecValue::{Fixed, Scheme},
};

/// What the self-hosted variable sans falls back to (THM-03, THM-32: no
/// third-party request, so the fallback is the reader's own system stack).
pub const SANS_FALLBACK: &str =
    "ui-sans-serif, system-ui, -apple-system, \"Segoe UI\", Roboto, Helvetica, Arial, sans-serif";
pub const MONO_FALLBACK: &str =
    "ui-monospace, SFMono-Regular, \"SF Mono\", Menlo, Consolas, \"Liberation Mono\", monospace";

macro_rules! specs {
    ($($group:ident $name:literal $doc:literal = $value:expr;)*) => {
        &[$(Spec { name: $name, group: $group, doc: $doc, value: $value },)*]
    };
}

pub static SPECS: &[Spec] = specs! {
    // ---- neutral ramp: 12 steps per scheme, backgrounds first, text last ----
    Color "--ly-color-neutral-1"  "App background." = Scheme { light: "#fcfdfe", dark: "#10131a" };
    Color "--ly-color-neutral-2"  "Subtle background." = Scheme { light: "#f7f8fb", dark: "#151820" };
    Color "--ly-color-neutral-3"  "Component surface." = Scheme { light: "#eef0f6", dark: "#1d202a" };
    Color "--ly-color-neutral-4"  "Component surface, hovered." = Scheme { light: "#e7e9f1", dark: "#242834" };
    Color "--ly-color-neutral-5"  "Component surface, pressed." = Scheme { light: "#dfe2ea", dark: "#2b303d" };
    Color "--ly-color-neutral-6"  "Subtle border and separator." = Scheme { light: "#d6d9e3", dark: "#343948" };
    Color "--ly-color-neutral-7"  "Element border." = Scheme { light: "#c9cdd8", dark: "#414758" };
    Color "--ly-color-neutral-8"  "Element border, hovered." = Scheme { light: "#b3b8c7", dark: "#545b70" };
    Color "--ly-color-neutral-9"  "Solid neutral fill; clears 3:1 on the app background." = Scheme { light: "#818798", dark: "#767d93" };
    Color "--ly-color-neutral-10" "Low-contrast text that still clears 4.5:1." = Scheme { light: "#5d6479", dark: "#828aa1" };
    Color "--ly-color-neutral-11" "Secondary text." = Scheme { light: "#4b5163", dark: "#afb5c8" };
    Color "--ly-color-neutral-12" "Primary text." = Scheme { light: "#1a1e2b", dark: "#f0f2f7" };

    // ---- semantic surfaces ----
    Color "--ly-color-bg" "Page background." = Fixed("var(--ly-color-neutral-1)");
    Color "--ly-color-bg-subtle" "Background of the chrome: navbar, sidebar, footer." = Fixed("var(--ly-color-neutral-2)");
    Color "--ly-color-surface" "Cards, callouts, and code blocks." = Fixed("var(--ly-color-neutral-3)");
    Color "--ly-color-surface-hover" "Surface under the pointer." = Fixed("var(--ly-color-neutral-4)");
    Color "--ly-color-surface-active" "Surface while pressed." = Fixed("var(--ly-color-neutral-5)");
    Color "--ly-color-elevated" "Panels that float: menus, dialogs, the search overlay." = Scheme { light: "#ffffff", dark: "#1d202a" };
    Color "--ly-color-border-subtle" "Hairline between sections." = Fixed("var(--ly-color-neutral-6)");
    Color "--ly-color-border" "Default border." = Fixed("var(--ly-color-neutral-7)");
    Color "--ly-color-border-strong" "Border of an interactive control; clears 3:1 (WCAG 2.2 non-text)." = Fixed("var(--ly-color-neutral-9)");

    // ---- text ----
    Color "--ly-color-text" "Body text." = Fixed("var(--ly-color-neutral-12)");
    Color "--ly-color-text-muted" "Secondary text: metadata, captions, inactive navigation." = Fixed("var(--ly-color-neutral-11)");
    Color "--ly-color-text-subtle" "The faintest text the theme uses; still AA on the background." = Fixed("var(--ly-color-neutral-10)");
    Color "--ly-color-text-inverse" "Text on a solid neutral or brand fill." = Scheme { light: "#ffffff", dark: "#10131a" };

    // ---- brand ----
    Color "--ly-color-primary" "Brand fill: primary buttons, the active navigation marker." = Scheme { light: "#1e34c8", dark: "#7484e7" };
    Color "--ly-color-primary-hover" "Brand fill under the pointer." = Scheme { light: "#1a2dad", dark: "#8e9beb" };
    Color "--ly-color-primary-active" "Brand fill while pressed." = Scheme { light: "#16267f", dark: "#a3aeef" };
    Color "--ly-color-primary-subtle" "Brand tint behind selected rows and callouts." = Scheme { light: "#edeffc", dark: "#1c2040" };
    Color "--ly-color-primary-border" "Border of a brand-tinted surface." = Scheme { light: "#adb6eb", dark: "#313c81" };
    Color "--ly-color-primary-contrast" "Text and icons on the brand fill." = Scheme { light: "#ffffff", dark: "#10131a" };
    Color "--ly-color-primary-text" "The brand as text: links and the active item's label." = Scheme { light: "#1b2fb1", dark: "#8290ed" };

    Color "--ly-color-accent" "Secondary brand fill." = Scheme { light: "#087581", dark: "#52d2e0" };
    Color "--ly-color-accent-subtle" "Accent tint." = Scheme { light: "#e5f8fa", dark: "#193e43" };
    Color "--ly-color-accent-contrast" "Text on the accent fill." = Scheme { light: "#ffffff", dark: "#10131a" };
    Color "--ly-color-accent-text" "The accent as text." = Scheme { light: "#08717d", dark: "#6adae7" };

    Color "--ly-color-success" "Success fill: check callouts, passing checks." = Scheme { light: "#0d7742", dark: "#45d38c" };
    Color "--ly-color-success-subtle" "Success tint." = Scheme { light: "#e1faed", dark: "#183929" };
    Color "--ly-color-success-text" "Success as text." = Scheme { light: "#0d7340", dark: "#60dc9e" };
    Color "--ly-color-warning" "Warning fill." = Scheme { light: "#986206", dark: "#f2ad36" };
    Color "--ly-color-warning-subtle" "Warning tint." = Scheme { light: "#fdf1dd", dark: "#3d2e14" };
    Color "--ly-color-warning-text" "Warning as text." = Scheme { light: "#8e5c06", dark: "#f5b547" };
    Color "--ly-color-danger" "Danger fill." = Scheme { light: "#bd1f29", dark: "#e25a63" };
    Color "--ly-color-danger-subtle" "Danger tint." = Scheme { light: "#fce8e9", dark: "#43191c" };
    Color "--ly-color-danger-text" "Danger as text." = Scheme { light: "#af1d26", dark: "#ed7880" };

    Color "--ly-color-focus" "Focus ring; 3:1 against both the background and the control (RX-90)." = Scheme { light: "#1e34c8", dark: "#7e8ef1" };
    Color "--ly-color-selection" "Text selection background." = Scheme { light: "#dce0fa", dark: "#212a63" };
    Color "--ly-color-overlay" "Scrim behind dialogs and the mobile drawer." = Scheme { light: "#1a1e2b99", dark: "#04060ab3" };

    // ---- typography ----
    Font "--ly-font-sans" "Body and UI face (THM-03)." = Fixed("InterVariable, Inter, ui-sans-serif, system-ui, -apple-system, \"Segoe UI\", Roboto, Helvetica, Arial, sans-serif");
    Font "--ly-font-heading" "Heading face; the body face unless `theme.fonts.heading` says otherwise." = Fixed("var(--ly-font-sans)");
    Font "--ly-font-mono" "Code face." = Fixed("\"JetBrains Mono\", ui-monospace, SFMono-Regular, \"SF Mono\", Menlo, Consolas, \"Liberation Mono\", monospace");
    Font "--ly-font-weight-normal" "Body weight." = Fixed("400");
    Font "--ly-font-weight-medium" "Emphasis in UI chrome." = Fixed("500");
    Font "--ly-font-weight-semibold" "Headings and active navigation." = Fixed("600");
    Font "--ly-font-weight-bold" "Strong emphasis in prose." = Fixed("700");
    Font "--ly-font-features-tabular" "Lining tabular figures for tables (THM-03)." = Fixed("\"tnum\" 1, \"lnum\" 1");
    Font "--ly-font-optical-body" "Optical size axis for body copy." = Fixed("16");
    Font "--ly-font-optical-display" "Optical size axis for display headings." = Fixed("32");

    // ---- type scale: fluid between 320 px and 1440 px ----
    Text "--ly-text-2xs" "Badges and table superscripts." = Fixed("clamp(0.6875rem, 0.67rem + 0.09vw, 0.75rem)");
    Text "--ly-text-xs" "Metadata and breadcrumb labels." = Fixed("clamp(0.75rem, 0.73rem + 0.1vw, 0.8125rem)");
    Text "--ly-text-sm" "Sidebar, table of contents, captions." = Fixed("clamp(0.8125rem, 0.79rem + 0.12vw, 0.875rem)");
    Text "--ly-text-base" "Body copy." = Fixed("clamp(0.9375rem, 0.91rem + 0.14vw, 1rem)");
    Text "--ly-text-lg" "Lead paragraph and H4." = Fixed("clamp(1.0625rem, 1.02rem + 0.2vw, 1.125rem)");
    Text "--ly-text-xl" "H3." = Fixed("clamp(1.1875rem, 1.12rem + 0.32vw, 1.3125rem)");
    Text "--ly-text-2xl" "H2." = Fixed("clamp(1.4375rem, 1.33rem + 0.53vw, 1.625rem)");
    Text "--ly-text-3xl" "H1." = Fixed("clamp(1.75rem, 1.57rem + 0.9vw, 2.125rem)");
    Text "--ly-text-4xl" "Landing-page display." = Fixed("clamp(2.125rem, 1.84rem + 1.42vw, 2.75rem)");
    Text "--ly-text-leading-tight" "Display headings." = Fixed("1.15");
    Text "--ly-text-leading-snug" "Headings and UI rows." = Fixed("1.35");
    Text "--ly-text-leading-normal" "Body copy." = Fixed("1.65");
    Text "--ly-text-leading-relaxed" "Callout and long-form body." = Fixed("1.75");
    Text "--ly-text-tracking-tight" "Display headings." = Fixed("-0.014em");
    Text "--ly-text-tracking-normal" "Everything else." = Fixed("0");
    Text "--ly-text-tracking-wide" "Eyebrows and small caps." = Fixed("0.04em");

    // ---- 8-point spacing scale ----
    Space "--ly-space-0" "Zero." = Fixed("0");
    Space "--ly-space-px" "Hairline." = Fixed("1px");
    Space "--ly-space-1" "4 px." = Fixed("0.25rem");
    Space "--ly-space-2" "8 px: the base step." = Fixed("0.5rem");
    Space "--ly-space-3" "12 px." = Fixed("0.75rem");
    Space "--ly-space-4" "16 px." = Fixed("1rem");
    Space "--ly-space-5" "24 px." = Fixed("1.5rem");
    Space "--ly-space-6" "32 px." = Fixed("2rem");
    Space "--ly-space-7" "48 px." = Fixed("3rem");
    Space "--ly-space-8" "64 px." = Fixed("4rem");
    Space "--ly-space-9" "96 px." = Fixed("6rem");
    Space "--ly-space-10" "128 px." = Fixed("8rem");

    Radius "--ly-radius-xs" "Tags and inline code." = Fixed("3px");
    Radius "--ly-radius-sm" "Buttons and inputs." = Fixed("5px");
    Radius "--ly-radius-md" "Cards, callouts, code blocks." = Fixed("8px");
    Radius "--ly-radius-lg" "Dialogs and the search overlay." = Fixed("12px");
    Radius "--ly-radius-xl" "Hero panels." = Fixed("18px");
    Radius "--ly-radius-full" "Pills and avatars." = Fixed("9999px");

    Shadow "--ly-shadow-color" "The shade every shadow is built from." = Scheme { light: "#1a1e2b1f", dark: "#00000080" };
    Shadow "--ly-shadow-sm" "Resting elevation: sticky header, tag." = Fixed("0 1px 2px var(--ly-shadow-color)");
    Shadow "--ly-shadow-md" "Menus and popovers." = Fixed("0 2px 4px var(--ly-shadow-color), 0 8px 16px -8px var(--ly-shadow-color)");
    Shadow "--ly-shadow-lg" "Dialogs and the search overlay." = Fixed("0 4px 8px var(--ly-shadow-color), 0 24px 48px -16px var(--ly-shadow-color)");

    Border "--ly-border-width" "Default border width." = Fixed("1px");
    Border "--ly-border-width-strong" "Emphasis border and focus ring." = Fixed("2px");
    Border "--ly-border" "The default border, composed." = Fixed("var(--ly-border-width) solid var(--ly-color-border)");
    Border "--ly-border-subtle" "The hairline border, composed." = Fixed("var(--ly-border-width) solid var(--ly-color-border-subtle)");

    // ---- motion: nothing over 200 ms (THM-05) ----
    Motion "--ly-motion-instant" "State changes that must not feel animated." = Fixed("80ms");
    Motion "--ly-motion-fast" "Hover and focus." = Fixed("120ms");
    Motion "--ly-motion-base" "Tabs, accordions, copy confirmation." = Fixed("160ms");
    Motion "--ly-motion-slow" "Overlays and the mobile drawer." = Fixed("200ms");
    Motion "--ly-motion-ease" "Standard curve." = Fixed("cubic-bezier(0.2, 0, 0, 1)");
    Motion "--ly-motion-ease-out" "Entrances." = Fixed("cubic-bezier(0, 0, 0, 1)");
    Motion "--ly-motion-ease-in" "Exits." = Fixed("cubic-bezier(0.3, 0, 1, 1)");

    ZIndex "--ly-z-base" "Document flow." = Fixed("0");
    ZIndex "--ly-z-raised" "Sticky table headers and code-block chrome." = Fixed("10");
    ZIndex "--ly-z-sticky" "Navbar and the sticky compact header." = Fixed("100");
    ZIndex "--ly-z-drawer" "Mobile navigation drawer." = Fixed("200");
    ZIndex "--ly-z-overlay" "Scrim." = Fixed("300");
    ZIndex "--ly-z-dialog" "Dialogs and the search palette." = Fixed("400");
    ZIndex "--ly-z-toast" "Toasts." = Fixed("500");
    ZIndex "--ly-z-tooltip" "Tooltips." = Fixed("600");
    ZIndex "--ly-z-skip-link" "The skip link, which must beat everything (RX-90)." = Fixed("700");

    Layout "--ly-layout-sidebar" "Sidebar column width." = Fixed("17rem");
    Layout "--ly-layout-content" "Measure of the content column." = Fixed("46rem");
    Layout "--ly-layout-toc" "Right rail width." = Fixed("15rem");
    Layout "--ly-layout-navbar" "Navbar height; the compact header is 0.75 of it." = Fixed("4rem");
    Layout "--ly-layout-banner" "Banner height when one is shown." = Fixed("2.5rem");
    Layout "--ly-layout-gutter" "Page gutter." = Fixed("var(--ly-space-5)");
    Layout "--ly-layout-max" "Widest the shell grows before it centres." = Fixed("96rem");
    Layout "--ly-layout-scroll-margin" "Offset that keeps an anchored heading clear of the header." = Fixed("calc(var(--ly-layout-navbar) + var(--ly-space-4))");

    // ---- code theme: both schemes as variables so RX-41 needs no re-render ----
    Code "--ly-code-bg" "Code block background." = Scheme { light: "#f5f6fa", dark: "#161922" };
    Code "--ly-code-fg" "Default code foreground." = Scheme { light: "#1a1e2b", dark: "#f0f2f7" };
    Code "--ly-code-border" "Code block border." = Fixed("var(--ly-color-border-subtle)");
    Code "--ly-code-gutter" "Line numbers." = Scheme { light: "#5d6479", dark: "#828aa1" };
    Code "--ly-code-line-highlight" "Highlighted line background." = Scheme { light: "#e4e7fb", dark: "#1f2447" };
    Code "--ly-code-selection" "Selection inside a code block." = Fixed("var(--ly-color-selection)");
    Code "--ly-code-font-size" "Code font size." = Fixed("var(--ly-text-sm)");
    Code "--ly-code-leading" "Code line height." = Fixed("1.6");
    Code "--ly-code-token-comment" "Comments." = Scheme { light: "#535a6e", dark: "#969db0" };
    Code "--ly-code-token-keyword" "Keywords." = Scheme { light: "#8129ae", dark: "#d096ee" };
    Code "--ly-code-token-string" "Strings." = Scheme { light: "#137242", dark: "#6cdaa3" };
    Code "--ly-code-token-number" "Numbers and constants." = Scheme { light: "#9c4911", dark: "#f4a967" };
    Code "--ly-code-token-function" "Functions and methods." = Scheme { light: "#2237bf", dark: "#90a7f4" };
    Code "--ly-code-token-variable" "Variables and parameters." = Scheme { light: "#1c5987", dark: "#84c9eb" };
    Code "--ly-code-token-type" "Types and classes." = Scheme { light: "#0d6d77", dark: "#6cd9e5" };
    Code "--ly-code-token-operator" "Operators." = Scheme { light: "#495065", dark: "#bcc1d2" };
    Code "--ly-code-token-punctuation" "Punctuation." = Scheme { light: "#5e6578", dark: "#a6acbf" };
    Code "--ly-code-token-tag" "Markup tags." = Scheme { light: "#a92350", dark: "#ef8aac" };
    Code "--ly-code-token-attribute" "Markup attributes." = Scheme { light: "#8e5e0b", dark: "#f5c166" };
    Code "--ly-code-token-deleted" "Removed lines in a diff." = Scheme { light: "#ad1f28", dark: "#ee8189" };
    Code "--ly-code-token-inserted" "Added lines in a diff." = Scheme { light: "#116f40", dark: "#6adca3" };
};

/// `theme.layout.density: "compact"` (CFG-09): one step down on the middle of
/// the scale, where the chrome lives; the extremes stay put so rhythm holds.
pub static COMPACT_SPACE: &[(&str, &str)] = &[
    ("--ly-space-3", "0.5rem"),
    ("--ly-space-4", "0.75rem"),
    ("--ly-space-5", "1.25rem"),
    ("--ly-space-6", "1.5rem"),
    ("--ly-space-7", "2.5rem"),
];

/// Every foreground-on-background pair the theme puts on screen, with the WCAG
/// 2.2 AA minimum it must clear: 4.5:1 for text, 3:1 for large text and the
/// boundary of a control (THM-02).
pub static AA_PAIRS: &[(&str, &str, f32)] = &[
    ("--ly-color-text", "--ly-color-bg", 4.5),
    ("--ly-color-text", "--ly-color-bg-subtle", 4.5),
    ("--ly-color-text", "--ly-color-surface", 4.5),
    ("--ly-color-text", "--ly-color-elevated", 4.5),
    ("--ly-color-text-muted", "--ly-color-bg", 4.5),
    ("--ly-color-text-muted", "--ly-color-bg-subtle", 4.5),
    ("--ly-color-text-muted", "--ly-color-surface", 4.5),
    ("--ly-color-text-subtle", "--ly-color-bg", 4.5),
    ("--ly-color-text-subtle", "--ly-color-surface", 4.5),
    ("--ly-color-text-inverse", "--ly-color-neutral-12", 4.5),
    ("--ly-color-primary-text", "--ly-color-bg", 4.5),
    ("--ly-color-primary-text", "--ly-color-bg-subtle", 4.5),
    ("--ly-color-primary-text", "--ly-color-surface", 4.5),
    ("--ly-color-primary-text", "--ly-color-primary-subtle", 4.5),
    ("--ly-color-primary-contrast", "--ly-color-primary", 4.5),
    (
        "--ly-color-primary-contrast",
        "--ly-color-primary-hover",
        4.5,
    ),
    ("--ly-color-accent-text", "--ly-color-bg", 4.5),
    ("--ly-color-accent-text", "--ly-color-accent-subtle", 4.5),
    ("--ly-color-accent-contrast", "--ly-color-accent", 4.5),
    ("--ly-color-success-text", "--ly-color-bg", 4.5),
    ("--ly-color-success-text", "--ly-color-success-subtle", 4.5),
    ("--ly-color-warning-text", "--ly-color-bg", 4.5),
    ("--ly-color-warning-text", "--ly-color-warning-subtle", 4.5),
    ("--ly-color-danger-text", "--ly-color-bg", 4.5),
    ("--ly-color-danger-text", "--ly-color-danger-subtle", 4.5),
    ("--ly-color-primary", "--ly-color-bg", 3.0),
    ("--ly-color-focus", "--ly-color-bg", 3.0),
    ("--ly-color-focus", "--ly-color-surface", 3.0),
    ("--ly-color-border-strong", "--ly-color-bg", 3.0),
    ("--ly-color-border-strong", "--ly-color-surface", 3.0),
    ("--ly-code-fg", "--ly-code-bg", 4.5),
    ("--ly-code-gutter", "--ly-code-bg", 4.5),
    ("--ly-code-token-comment", "--ly-code-bg", 4.5),
    ("--ly-code-token-keyword", "--ly-code-bg", 4.5),
    ("--ly-code-token-string", "--ly-code-bg", 4.5),
    ("--ly-code-token-number", "--ly-code-bg", 4.5),
    ("--ly-code-token-function", "--ly-code-bg", 4.5),
    ("--ly-code-token-variable", "--ly-code-bg", 4.5),
    ("--ly-code-token-type", "--ly-code-bg", 4.5),
    ("--ly-code-token-operator", "--ly-code-bg", 4.5),
    ("--ly-code-token-punctuation", "--ly-code-bg", 4.5),
    ("--ly-code-token-tag", "--ly-code-bg", 4.5),
    ("--ly-code-token-attribute", "--ly-code-bg", 4.5),
    ("--ly-code-token-deleted", "--ly-code-bg", 4.5),
    ("--ly-code-token-inserted", "--ly-code-bg", 4.5),
    ("--ly-code-token-comment", "--ly-code-line-highlight", 4.5),
    ("--ly-code-fg", "--ly-code-line-highlight", 4.5),
];

/// The tokens the critical block carries (THM-30: "layout shell and
/// above-the-fold tokens only"). Everything a component needs arrives with the
/// cached stylesheet instead.
pub static CRITICAL: &[&str] = &[
    "--ly-color-neutral-1",
    "--ly-color-neutral-2",
    "--ly-color-neutral-3",
    "--ly-color-neutral-6",
    "--ly-color-neutral-7",
    "--ly-color-neutral-9",
    "--ly-color-neutral-11",
    "--ly-color-neutral-12",
    "--ly-color-bg",
    "--ly-color-bg-subtle",
    "--ly-color-surface",
    "--ly-color-elevated",
    "--ly-color-border",
    "--ly-color-border-subtle",
    "--ly-color-border-strong",
    "--ly-color-text",
    "--ly-color-text-muted",
    "--ly-color-text-subtle",
    "--ly-color-primary",
    "--ly-color-primary-subtle",
    "--ly-color-primary-text",
    "--ly-color-focus",
    "--ly-font-sans",
    "--ly-font-heading",
    "--ly-font-mono",
    "--ly-font-weight-normal",
    "--ly-font-weight-medium",
    "--ly-font-weight-semibold",
    "--ly-font-optical-body",
    "--ly-text-2xs",
    "--ly-text-xs",
    "--ly-text-sm",
    "--ly-text-base",
    "--ly-text-lg",
    "--ly-text-leading-snug",
    "--ly-text-leading-normal",
    "--ly-text-tracking-tight",
    "--ly-text-tracking-wide",
    "--ly-space-1",
    "--ly-space-2",
    "--ly-space-3",
    "--ly-space-4",
    "--ly-space-5",
    "--ly-space-6",
    "--ly-space-8",
    "--ly-radius-sm",
    "--ly-radius-md",
    "--ly-shadow-color",
    "--ly-shadow-sm",
    "--ly-border-width",
    "--ly-border-width-strong",
    "--ly-border",
    "--ly-border-subtle",
    "--ly-motion-fast",
    "--ly-motion-base",
    "--ly-motion-ease",
    "--ly-z-sticky",
    "--ly-z-skip-link",
    "--ly-layout-sidebar",
    "--ly-layout-content",
    "--ly-layout-toc",
    "--ly-layout-navbar",
    "--ly-layout-banner",
    "--ly-layout-gutter",
    "--ly-layout-max",
    "--ly-layout-scroll-margin",
];
