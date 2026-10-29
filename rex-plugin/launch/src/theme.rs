use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// Backgrounds and border color for the card. Every cell the composer
/// draws needs an explicit background, so these always have a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub card: Color,
    pub picked: Color,
    pub edge: Color,
}

const FALLBACK: Theme = Theme {
    card: Color::Rgb(26, 27, 38),
    picked: Color::Rgb(48, 52, 74),
    edge: Color::Rgb(86, 92, 124),
};

impl Theme {
    /// Falls back to fixed colors unless both terminal colors are known.
    pub fn from_terminal(background: Option<Rgb>, foreground: Option<Rgb>) -> Theme {
        let (Some(bg), Some(fg)) = (background, foreground) else {
            return FALLBACK;
        };
        Theme {
            card: color(bg),
            picked: color(blend(bg, fg, 12)),
            edge: color(blend(bg, fg, 30)),
        }
    }
}

fn color(Rgb(r, g, b): Rgb) -> Color {
    Color::Rgb(r, g, b)
}

/// Moves `from` toward `to` by `percent`. It darkens a light background
/// when `to` is a dark foreground.
pub fn blend(from: Rgb, to: Rgb, percent: u32) -> Rgb {
    let mix = |a: u8, b: u8| ((a as u32 * (100 - percent) + b as u32 * percent + 50) / 100) as u8;
    Rgb(mix(from.0, to.0), mix(from.1, to.1), mix(from.2, to.2))
}

/// Finds the replies to OSC 11 (background) and OSC 10 (foreground) in
/// `buf`. Each reply ends in BEL or ESC `\`.
pub fn parse_replies(buf: &[u8]) -> (Option<Rgb>, Option<Rgb>) {
    let text = String::from_utf8_lossy(buf);
    (reply(&text, "11"), reply(&text, "10"))
}

fn reply(text: &str, code: &str) -> Option<Rgb> {
    let marker = format!("\x1b]{code};");
    let start = text.find(&marker)? + marker.len();
    let rest = &text[start..];
    let end = rest.find(['\x07', '\x1b'])?;
    parse_rgb(&rest[..end])
}

/// Parses `rgb:R/G/B` with one to four hex digits per channel.
pub fn parse_rgb(spec: &str) -> Option<Rgb> {
    let mut channels = spec.strip_prefix("rgb:")?.split('/');
    let mut next = || {
        let digits = channels.next()?;
        if digits.is_empty() || digits.len() > 4 {
            return None;
        }
        let value = u32::from_str_radix(digits, 16).ok()?;
        let max = 16u32.pow(digits.len() as u32) - 1;
        Some(((value * 255 + max / 2) / max) as u8)
    };
    let rgb = Rgb(next()?, next()?, next()?);
    channels.next().is_none().then_some(rgb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_one_to_four_digit_channels() {
        assert_eq!(parse_rgb("rgb:f/0/8"), Some(Rgb(255, 0, 136)));
        assert_eq!(parse_rgb("rgb:ff/80/00"), Some(Rgb(255, 128, 0)));
        assert_eq!(parse_rgb("rgb:fff/000/800"), Some(Rgb(255, 0, 128)));
        assert_eq!(parse_rgb("rgb:ffff/0000/8080"), Some(Rgb(255, 0, 128)));
    }

    #[test]
    fn rejects_malformed_specs() {
        assert_eq!(parse_rgb("rgb:ff/ff"), None);
        assert_eq!(parse_rgb("rgb:ff/ff/ff/ff"), None);
        assert_eq!(parse_rgb("rgb:fffff/00/00"), None);
        assert_eq!(parse_rgb("rgb:zz/00/00"), None);
        assert_eq!(parse_rgb("#ffffff"), None);
    }

    #[test]
    fn parses_replies_ended_by_bel_or_st() {
        let buf = b"\x1b]11;rgb:0000/0000/0000\x07\x1b]10;rgb:ffff/ffff/ffff\x1b\\";
        assert_eq!(
            parse_replies(buf),
            (Some(Rgb(0, 0, 0)), Some(Rgb(255, 255, 255)))
        );
    }

    #[test]
    fn a_missing_reply_is_none() {
        let buf = b"\x1b]11;rgb:10/20/30\x07";
        assert_eq!(parse_replies(buf), (Some(Rgb(16, 32, 48)), None));
        assert_eq!(parse_replies(b""), (None, None));
    }

    #[test]
    fn blends_toward_the_foreground_on_dark_and_light() {
        let black = Rgb(0, 0, 0);
        let white = Rgb(255, 255, 255);
        assert_eq!(blend(black, white, 12), Rgb(31, 31, 31));
        assert_eq!(blend(white, black, 12), Rgb(224, 224, 224));
    }

    #[test]
    fn card_is_the_background_exactly() {
        let theme = Theme::from_terminal(Some(Rgb(0, 0, 0)), Some(Rgb(255, 255, 255)));
        assert_eq!(theme.card, Color::Rgb(0, 0, 0));
        assert_eq!(theme.edge, Color::Rgb(77, 77, 77));
    }

    #[test]
    fn any_missing_color_falls_back() {
        assert_eq!(Theme::from_terminal(Some(Rgb(0, 0, 0)), None), FALLBACK);
        assert_eq!(Theme::from_terminal(None, None), FALLBACK);
    }
}
