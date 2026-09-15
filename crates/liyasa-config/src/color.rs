//! Reading a CSS colour far enough to check its contrast (CFG-04).
//!
//! This is not a CSS colour parser and does not try to be: it reads the forms a
//! config actually writes — hex, `rgb()`, `hsl()`, and the basic keywords — and
//! says so plainly when it cannot. A value it does not resolve is not an error,
//! because `var(--brand)` and the long list of CSS named colours are both
//! legitimate; only a value that is clearly meant to be one of the forms above
//! and is malformed is [`Parsed::Malformed`].

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parsed {
    Known(Color),
    /// A hex or function form that does not parse: `E0132`.
    Malformed,
    /// A syntactically fine value this does not resolve, such as a named colour
    /// outside the table below or a custom property. Not a diagnostic.
    Unresolved,
}

/// The HTML basic colour keywords, plus the greys everyone writes.
const NAMED: &[(&str, (u8, u8, u8))] = &[
    ("aqua", (0, 255, 255)),
    ("black", (0, 0, 0)),
    ("blue", (0, 0, 255)),
    ("fuchsia", (255, 0, 255)),
    ("gray", (128, 128, 128)),
    ("green", (0, 128, 0)),
    ("grey", (128, 128, 128)),
    ("lime", (0, 255, 0)),
    ("maroon", (128, 0, 0)),
    ("navy", (0, 0, 128)),
    ("olive", (128, 128, 0)),
    ("purple", (128, 0, 128)),
    ("red", (255, 0, 0)),
    ("silver", (192, 192, 192)),
    ("teal", (0, 128, 128)),
    ("white", (255, 255, 255)),
    ("yellow", (255, 255, 0)),
];

impl Color {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    pub fn parse(text: &str) -> Parsed {
        let text = text.trim();
        if let Some(hex) = text.strip_prefix('#') {
            return from_hex(hex);
        }
        if let Some((name, arguments)) = function(text) {
            return match name {
                "rgb" | "rgba" => from_rgb(arguments),
                "hsl" | "hsla" => from_hsl(arguments),
                _ => Parsed::Unresolved,
            };
        }
        let lower = text.to_ascii_lowercase();
        match NAMED.binary_search_by(|(name, _)| (*name).cmp(lower.as_str())) {
            Ok(at) => {
                let (_, (red, green, blue)) = NAMED[at];
                Parsed::Known(Self::new(red, green, blue))
            }
            Err(_) => Parsed::Unresolved,
        }
    }

    /// The WCAG relative luminance of this colour.
    pub fn luminance(self) -> f64 {
        fn channel(value: u8) -> f64 {
            let value = f64::from(value) / 255.0;
            if value <= 0.03928 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.red) + 0.7152 * channel(self.green) + 0.0722 * channel(self.blue)
    }
}

/// The WCAG 2.2 contrast ratio, between 1 and 21. AA wants 4.5 for body text
/// and 3.0 for large text.
pub fn contrast(a: Color, b: Color) -> f64 {
    let (a, b) = (a.luminance(), b.luminance());
    let (lighter, darker) = if a >= b { (a, b) } else { (b, a) };
    (lighter + 0.05) / (darker + 0.05)
}

/// WCAG AA for normal-size text.
pub const AA_NORMAL: f64 = 4.5;

fn from_hex(hex: &str) -> Parsed {
    let digits: Vec<u8> = match hex
        .chars()
        .map(|c| c.to_digit(16).map(|d| d as u8))
        .collect()
    {
        Some(digits) => digits,
        None => return Parsed::Malformed,
    };
    let channels: [u8; 3] = match digits.len() {
        3 | 4 => [digits[0] * 17, digits[1] * 17, digits[2] * 17],
        6 | 8 => [
            digits[0] * 16 + digits[1],
            digits[2] * 16 + digits[3],
            digits[4] * 16 + digits[5],
        ],
        _ => return Parsed::Malformed,
    };
    Parsed::Known(Color::new(channels[0], channels[1], channels[2]))
}

/// `name(arguments)` split, for a value that is written as a function call.
fn function(text: &str) -> Option<(&str, &str)> {
    let open = text.find('(')?;
    let arguments = text[open + 1..].strip_suffix(')')?;
    Some((text[..open].trim(), arguments))
}

/// Both the legacy `r, g, b, a` and the modern `r g b / a` argument forms.
fn arguments(text: &str) -> Vec<&str> {
    let text = text.split('/').next().unwrap_or(text);
    text.split([',', ' '])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

fn from_rgb(text: &str) -> Parsed {
    let parts = arguments(text);
    if parts.len() < 3 {
        return Parsed::Malformed;
    }
    let mut channels = [0u8; 3];
    for (slot, part) in channels.iter_mut().zip(&parts) {
        match number(part) {
            Some(value) if part.ends_with('%') => *slot = scale(value / 100.0),
            Some(value) => *slot = scale(value / 255.0),
            None => return Parsed::Malformed,
        }
    }
    Parsed::Known(Color::new(channels[0], channels[1], channels[2]))
}

fn from_hsl(text: &str) -> Parsed {
    let parts = arguments(text);
    if parts.len() < 3 {
        return Parsed::Malformed;
    }
    let (Some(hue), Some(saturation), Some(lightness)) =
        (number(parts[0]), number(parts[1]), number(parts[2]))
    else {
        return Parsed::Malformed;
    };
    let hue = hue.rem_euclid(360.0) / 60.0;
    let saturation = (saturation / 100.0).clamp(0.0, 1.0);
    let lightness = (lightness / 100.0).clamp(0.0, 1.0);

    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let second = chroma * (1.0 - (hue % 2.0 - 1.0).abs());
    let (red, green, blue) = match hue as u8 {
        0 => (chroma, second, 0.0),
        1 => (second, chroma, 0.0),
        2 => (0.0, chroma, second),
        3 => (0.0, second, chroma),
        4 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let base = lightness - chroma / 2.0;
    Parsed::Known(Color::new(
        scale(red + base),
        scale(green + base),
        scale(blue + base),
    ))
}

fn number(text: &str) -> Option<f64> {
    text.trim_end_matches(['%', 'd', 'e', 'g', 'r', 'a'])
        .parse()
        .ok()
}

fn scale(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}
