//! sRGB colours and the WCAG 2.2 contrast maths behind THM-02.
//!
//! Every value an operator can put in `theme.colors` (CFG-04) is "any CSS
//! colour", so parsing accepts what a stylesheet accepts for an opaque or
//! translucent colour and reports everything else as unsupported rather than
//! guessing.

use std::fmt;

/// A colour in sRGB with straight (non-premultiplied) alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not a colour this build can evaluate")]
pub struct ColorError(pub String);

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Parses a hex, `rgb()`, `rgba()`, `hsl()`, or `hsla()` colour, plus the
    /// two keywords a token file realistically carries.
    pub fn parse(text: &str) -> Result<Self, ColorError> {
        let text = text.trim();
        let lower = text.to_ascii_lowercase();
        match lower.as_str() {
            "white" => return Ok(Self::rgb(255, 255, 255)),
            "black" => return Ok(Self::rgb(0, 0, 0)),
            "transparent" => {
                return Ok(Self {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0.0,
                });
            }
            _ => {}
        }
        if let Some(hex) = lower.strip_prefix('#') {
            return Self::from_hex(hex).ok_or_else(|| ColorError(text.to_owned()));
        }
        for prefix in ["rgba", "rgb"] {
            if let Some(args) = function_args(&lower, prefix) {
                return Self::from_rgb_args(&args).ok_or_else(|| ColorError(text.to_owned()));
            }
        }
        for prefix in ["hsla", "hsl"] {
            if let Some(args) = function_args(&lower, prefix) {
                return Self::from_hsl_args(&args).ok_or_else(|| ColorError(text.to_owned()));
            }
        }
        Err(ColorError(text.to_owned()))
    }

    fn from_hex(hex: &str) -> Option<Self> {
        let digits: Vec<u8> = hex
            .chars()
            .map(|c| c.to_digit(16).map(|d| d as u8))
            .collect::<Option<_>>()?;
        let pair = |high: u8, low: u8| high * 16 + low;
        let short = |value: u8| value * 16 + value;
        match digits.len() {
            3 => Some(Self::rgb(
                short(digits[0]),
                short(digits[1]),
                short(digits[2]),
            )),
            4 => Some(Self {
                r: short(digits[0]),
                g: short(digits[1]),
                b: short(digits[2]),
                a: f32::from(short(digits[3])) / 255.0,
            }),
            6 => Some(Self::rgb(
                pair(digits[0], digits[1]),
                pair(digits[2], digits[3]),
                pair(digits[4], digits[5]),
            )),
            8 => Some(Self {
                r: pair(digits[0], digits[1]),
                g: pair(digits[2], digits[3]),
                b: pair(digits[4], digits[5]),
                a: f32::from(pair(digits[6], digits[7])) / 255.0,
            }),
            _ => None,
        }
    }

    fn from_rgb_args(args: &[String]) -> Option<Self> {
        let channel = |text: &str| -> Option<u8> {
            let value = match text.strip_suffix('%') {
                Some(percent) => percent.trim().parse::<f32>().ok()? * 2.55,
                None => text.parse::<f32>().ok()?,
            };
            Some(value.round().clamp(0.0, 255.0) as u8)
        };
        match args.len() {
            3 | 4 => Some(Self {
                r: channel(&args[0])?,
                g: channel(&args[1])?,
                b: channel(&args[2])?,
                a: args.get(3).map_or(Some(1.0), |text| alpha(text))?,
            }),
            _ => None,
        }
    }

    fn from_hsl_args(args: &[String]) -> Option<Self> {
        if !(3..=4).contains(&args.len()) {
            return None;
        }
        let hue = args[0]
            .trim_end_matches("deg")
            .parse::<f32>()
            .ok()?
            .rem_euclid(360.0)
            / 360.0;
        let saturation = percent(&args[1])?;
        let lightness = percent(&args[2])?;
        let a = args.get(3).map_or(Some(1.0), |text| alpha(text))?;

        let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
        let x = c * (1.0 - ((hue * 6.0) % 2.0 - 1.0).abs());
        let m = lightness - c / 2.0;
        let (r, g, b) = match (hue * 6.0) as u8 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let byte = |value: f32| ((value + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        Some(Self {
            r: byte(r),
            g: byte(g),
            b: byte(b),
            a,
        })
    }

    /// WCAG relative luminance of the opaque colour.
    pub fn luminance(self) -> f32 {
        let linear = |channel: u8| {
            let value = f32::from(channel) / 255.0;
            if value <= 0.040_45 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(self.r) + 0.7152 * linear(self.g) + 0.0722 * linear(self.b)
    }

    /// The WCAG 2.2 contrast ratio, between 1.0 and 21.0.
    ///
    /// A translucent colour is composited over `self`'s counterpart first,
    /// because a ratio against a colour nobody can see is not a measurement.
    pub fn contrast(self, background: Self) -> f32 {
        let front = self.over(background).luminance();
        let back = background.opaque().luminance();
        let (high, low) = if front > back {
            (front, back)
        } else {
            (back, front)
        };
        (high + 0.05) / (low + 0.05)
    }

    /// Composites this colour over an opaque background.
    pub fn over(self, background: Self) -> Self {
        if self.a >= 1.0 {
            return self.opaque();
        }
        let blend = |front: u8, back: u8| {
            (f32::from(front) * self.a + f32::from(back) * (1.0 - self.a)).round() as u8
        };
        let background = background.opaque();
        Self::rgb(
            blend(self.r, background.r),
            blend(self.g, background.g),
            blend(self.b, background.b),
        )
    }

    pub fn opaque(self) -> Self {
        Self::rgb(self.r, self.g, self.b)
    }

    /// Linear interpolation in sRGB space, `t` clamped to 0.0..=1.0.
    pub fn mix(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let blend = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
        Self {
            r: blend(self.r, other.r),
            g: blend(self.g, other.g),
            b: blend(self.b, other.b),
            a: self.a + (other.a - self.a) * t,
        }
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self {
            a: a.clamp(0.0, 1.0),
            ..self
        }
    }
}

impl fmt::Display for Color {
    /// Hex, because that is what the emitted token files carry: six digits when
    /// opaque, eight otherwise.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)?;
        if self.a < 1.0 {
            write!(
                f,
                "{:02x}",
                (self.a * 255.0).round().clamp(0.0, 255.0) as u8
            )?;
        }
        Ok(())
    }
}

fn function_args(text: &str, name: &str) -> Option<Vec<String>> {
    let rest = text.strip_prefix(name)?.trim_start();
    let inner = rest.strip_prefix('(')?.strip_suffix(')')?;
    Some(
        inner
            .replace('/', ",")
            .split([',', ' '])
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

fn percent(text: &str) -> Option<f32> {
    let value = text.strip_suffix('%')?.trim().parse::<f32>().ok()?;
    Some((value / 100.0).clamp(0.0, 1.0))
}

fn alpha(text: &str) -> Option<f32> {
    let value = match text.strip_suffix('%') {
        Some(percent) => percent.trim().parse::<f32>().ok()? / 100.0,
        None => text.parse::<f32>().ok()?,
    };
    Some(value.clamp(0.0, 1.0))
}

/// The two WCAG 2.2 AA thresholds THM-02 holds every pair to.
pub const AA_BODY: f32 = 4.5;
pub const AA_LARGE: f32 = 3.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Color {
        Color::parse(text).expect("colour parses")
    }

    #[test]
    fn parses_every_accepted_form() {
        assert_eq!(parse("#fff"), Color::rgb(255, 255, 255));
        assert_eq!(parse("#0A7CFF"), Color::rgb(10, 124, 255));
        assert_eq!(parse("rgb(10, 124, 255)"), Color::rgb(10, 124, 255));
        assert_eq!(parse("rgb(10 124 255)"), Color::rgb(10, 124, 255));
        assert_eq!(parse("hsl(0, 0%, 100%)"), Color::rgb(255, 255, 255));
        assert_eq!(parse("hsl(210, 100%, 50%)"), Color::rgb(0, 128, 255));
        assert_eq!(parse("  white "), Color::rgb(255, 255, 255));
        assert_eq!(parse("#00000080").a, 128.0 / 255.0);
        assert_eq!(parse("rgba(0, 0, 0, 0.5)").a, 0.5);
    }

    #[test]
    fn unsupported_colour_is_an_error_not_a_guess() {
        assert!(Color::parse("rebeccapurple").is_err());
        assert!(Color::parse("var(--ly-color-primary)").is_err());
        assert!(Color::parse("#12345").is_err());
        assert!(Color::parse("").is_err());
    }

    #[test]
    fn contrast_matches_the_published_wcag_examples() {
        let white = Color::rgb(255, 255, 255);
        let black = Color::rgb(0, 0, 0);
        assert!((black.contrast(white) - 21.0).abs() < 0.01);
        assert!((white.contrast(white) - 1.0).abs() < 0.01);
        // The two greys the WCAG techniques use as the AA boundary on white.
        assert!((parse("#767676").contrast(white) - 4.54).abs() < 0.02);
        assert!((parse("#595959").contrast(white) - 7.00).abs() < 0.02);
        // Symmetric: the ratio does not depend on which colour is in front.
        assert!((white.contrast(black) - black.contrast(white)).abs() < 0.001);
    }

    #[test]
    fn translucent_text_is_measured_where_it_lands() {
        let white = Color::rgb(255, 255, 255);
        let half_black = Color::rgb(0, 0, 0).with_alpha(0.5);
        assert!(half_black.contrast(white) < Color::rgb(0, 0, 0).contrast(white));
        assert_eq!(half_black.over(white), Color::rgb(128, 128, 128));
    }

    #[test]
    fn hex_round_trips_through_display() {
        assert_eq!(parse("#0a7cff").to_string(), "#0a7cff");
        assert_eq!(parse("rgba(0,0,0,0.5)").to_string(), "#00000080");
    }
}
