//! A tiny vector scene (paths and text, in pt, y down) and its two outputs:
//! SVG text and PDF bytes. Renderers build scenes; nothing here knows mazes.

use std::fmt::Write as _;

use pdf_writer::types::{LineCapStyle, LineJoinStyle};
use pdf_writer::{Content, Name, Pdf, Rect, Ref, Str};

use crate::Error;

/// An sRGB colour, parsed from CSS hex (`#rgb` or `#rrggbb`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    pub const BLACK: Color = Color(0, 0, 0);
    pub const WHITE: Color = Color(255, 255, 255);

    pub fn parse(s: &str) -> Result<Color, Error> {
        let bad = || Error::BadColor(s.to_string());
        let hex = s.trim().strip_prefix('#').ok_or_else(bad)?;
        if !hex.is_ascii() {
            return Err(bad());
        }
        let digit = |i: usize, n: usize| u8::from_str_radix(&hex[i..i + n], 16).map_err(|_| bad());
        let c = match hex.len() {
            3 => Color(digit(0, 1)? * 17, digit(1, 1)? * 17, digit(2, 1)? * 17),
            6 => Color(digit(0, 2)?, digit(2, 2)?, digit(4, 2)?),
            _ => return Err(bad()),
        };
        Ok(c)
    }

    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    fn unit(self) -> (f32, f32, f32) {
        let f = |v: u8| v as f32 / 255.0;
        (f(self.0), f(self.1), f(self.2))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    M(f64, f64),
    L(f64, f64),
    Z,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Join {
    Miter,
    Round,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub color: Color,
    pub width: f64,
    pub cap: Cap,
    pub join: Join,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Start,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Path {
        d: Vec<Cmd>,
        fill: Option<Color>,
        stroke: Option<Stroke>,
        /// SVG class, for tests and styling (e.g. "entrance").
        class: Option<&'static str>,
    },
    Text {
        x: f64,
        y: f64,
        size: f64,
        anchor: Anchor,
        color: Color,
        text: String,
    },
}

impl Item {
    pub fn fill(d: Vec<Cmd>, color: Color) -> Item {
        Item::Path {
            d,
            fill: Some(color),
            stroke: None,
            class: None,
        }
    }

    pub fn stroke(d: Vec<Cmd>, stroke: Stroke) -> Item {
        Item::Path {
            d,
            fill: None,
            stroke: Some(stroke),
            class: None,
        }
    }

    pub fn with_class(mut self, c: &'static str) -> Item {
        if let Item::Path { class, .. } = &mut self {
            *class = Some(c);
        }
        self
    }

    fn translated(&self, dx: f64, dy: f64) -> Item {
        match self {
            Item::Path {
                d,
                fill,
                stroke,
                class,
            } => Item::Path {
                d: d.iter()
                    .map(|c| match *c {
                        Cmd::M(x, y) => Cmd::M(x + dx, y + dy),
                        Cmd::L(x, y) => Cmd::L(x + dx, y + dy),
                        Cmd::Z => Cmd::Z,
                    })
                    .collect(),
                fill: *fill,
                stroke: *stroke,
                class: *class,
            },
            Item::Text {
                x,
                y,
                size,
                anchor,
                color,
                text,
            } => Item::Text {
                x: x + dx,
                y: y + dy,
                size: *size,
                anchor: *anchor,
                color: *color,
                text: text.clone(),
            },
        }
    }
}

/// A rectangle as a closed path.
pub fn rect(x: f64, y: f64, w: f64, h: f64) -> Vec<Cmd> {
    vec![
        Cmd::M(x, y),
        Cmd::L(x + w, y),
        Cmd::L(x + w, y + h),
        Cmd::L(x, y + h),
        Cmd::Z,
    ]
}

/// Paths and text on a white `width × height` pt canvas, painted in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub width: f64,
    pub height: f64,
    pub items: Vec<Item>,
}

impl Scene {
    pub fn new(width: f64, height: f64) -> Scene {
        Scene {
            width,
            height,
            items: vec![],
        }
    }

    /// Append `other`'s items, shifted by (dx, dy).
    pub fn place(&mut self, other: &Scene, dx: f64, dy: f64) {
        self.items
            .extend(other.items.iter().map(|i| i.translated(dx, dy)));
    }

    pub fn to_svg(&self) -> String {
        let mut s = String::new();
        let _ = write!(
            s,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.2}pt\" height=\"{:.2}pt\" \
             viewBox=\"0 0 {:.3} {:.3}\">\n<rect width=\"100%\" height=\"100%\" fill=\"#fff\"/>\n",
            self.width, self.height, self.width, self.height
        );
        for item in &self.items {
            match item {
                Item::Path {
                    d,
                    fill,
                    stroke,
                    class,
                } => {
                    s.push_str("<path");
                    if let Some(c) = class {
                        let _ = write!(s, " class=\"{c}\"");
                    }
                    match fill {
                        Some(f) => {
                            let _ = write!(s, " fill=\"{}\"", f.hex());
                        }
                        None => s.push_str(" fill=\"none\""),
                    }
                    if let Some(st) = stroke {
                        let cap = match st.cap {
                            Cap::Butt => "butt",
                            Cap::Round => "round",
                            Cap::Square => "square",
                        };
                        let join = match st.join {
                            Join::Miter => "miter",
                            Join::Round => "round",
                        };
                        let _ = write!(
                            s,
                            " stroke=\"{}\" stroke-width=\"{:.3}\" stroke-linecap=\"{cap}\" \
                             stroke-linejoin=\"{join}\"",
                            st.color.hex(),
                            st.width
                        );
                    }
                    s.push_str(" d=\"");
                    for c in d {
                        let _ = match *c {
                            Cmd::M(x, y) => write!(s, "M{x:.3} {y:.3}"),
                            Cmd::L(x, y) => write!(s, "L{x:.3} {y:.3}"),
                            Cmd::Z => write!(s, "z"),
                        };
                    }
                    s.push_str("\"/>\n");
                }
                Item::Text {
                    x,
                    y,
                    size,
                    anchor,
                    color,
                    text,
                } => {
                    let anchor = match anchor {
                        Anchor::Start => "",
                        Anchor::End => " text-anchor=\"end\"",
                    };
                    let _ = writeln!(
                        s,
                        "<text x=\"{x:.2}\" y=\"{y:.2}\"{anchor} font-family=\"Helvetica, Arial, \
                         sans-serif\" font-size=\"{size}\" fill=\"{}\">{}</text>",
                        color.hex(),
                        xml_escape(text)
                    );
                }
            }
        }
        s.push_str("</svg>\n");
        s
    }

    /// A one-page PDF at true scale (1 unit = 1 pt). Text is Helvetica (a
    /// standard font, not embedded).
    pub fn to_pdf(&self) -> Vec<u8> {
        let (catalog, tree, page, font, contents) = (
            Ref::new(1),
            Ref::new(2),
            Ref::new(3),
            Ref::new(4),
            Ref::new(5),
        );
        let mut pdf = Pdf::new();
        pdf.catalog(catalog).pages(tree);
        pdf.pages(tree).kids([page]).count(1);
        {
            let mut p = pdf.page(page);
            p.media_box(Rect::new(0.0, 0.0, f(self.width), f(self.height)));
            p.parent(tree);
            p.contents(contents);
            p.resources().fonts().pair(Name(b"F1"), font);
        }
        pdf.type1_font(font)
            .base_font(Name(b"Helvetica"))
            .encoding_predefined(Name(b"WinAnsiEncoding"));

        let mut c = Content::new();
        // y down, like SVG
        c.transform([1.0, 0.0, 0.0, -1.0, 0.0, f(self.height)]);
        c.set_fill_rgb(1.0, 1.0, 1.0);
        c.rect(0.0, 0.0, f(self.width), f(self.height));
        c.fill_nonzero();
        for item in &self.items {
            match item {
                Item::Path {
                    d, fill, stroke, ..
                } => {
                    if let Some(col) = fill {
                        let (r, g, b) = col.unit();
                        c.set_fill_rgb(r, g, b);
                    }
                    if let Some(st) = stroke {
                        let (r, g, b) = st.color.unit();
                        c.set_stroke_rgb(r, g, b);
                        c.set_line_width(f(st.width));
                        c.set_line_cap(match st.cap {
                            Cap::Butt => LineCapStyle::ButtCap,
                            Cap::Round => LineCapStyle::RoundCap,
                            Cap::Square => LineCapStyle::ProjectingSquareCap,
                        });
                        c.set_line_join(match st.join {
                            Join::Miter => LineJoinStyle::MiterJoin,
                            Join::Round => LineJoinStyle::RoundJoin,
                        });
                    }
                    for cmd in d {
                        match *cmd {
                            Cmd::M(x, y) => c.move_to(f(x), f(y)),
                            Cmd::L(x, y) => c.line_to(f(x), f(y)),
                            Cmd::Z => c.close_path(),
                        };
                    }
                    match (fill.is_some(), stroke.is_some()) {
                        (true, true) => c.fill_nonzero_and_stroke(),
                        (true, false) => c.fill_nonzero(),
                        (false, true) => c.stroke(),
                        (false, false) => c.end_path(),
                    };
                }
                Item::Text {
                    x,
                    y,
                    size,
                    anchor,
                    color,
                    text,
                } => {
                    let bytes = win_ansi(text);
                    let x = match anchor {
                        Anchor::Start => *x,
                        Anchor::End => x - helvetica_width(&bytes) * size / 1000.0,
                    };
                    let (r, g, b) = color.unit();
                    c.set_fill_rgb(r, g, b);
                    c.begin_text();
                    c.set_font(Name(b"F1"), f(*size));
                    // undo the page flip for glyphs
                    c.set_text_matrix([1.0, 0.0, 0.0, -1.0, f(x), f(*y)]);
                    c.show(Str(&bytes));
                    c.end_text();
                }
            }
        }
        pdf.stream(contents, &c.finish());
        pdf.finish()
    }
}

/// pt → PDF number, rounded to 1/1000 pt like the SVG output.
fn f(v: f64) -> f32 {
    ((v * 1000.0).round() / 1000.0) as f32
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Text → WinAnsiEncoding bytes; Latin-1 maps directly, anything else is '?'.
fn win_ansi(s: &str) -> Vec<u8> {
    s.chars()
        .map(|ch| match ch as u32 {
            c @ (0x20..=0x7e | 0xa0..=0xff) => c as u8,
            _ => b'?',
        })
        .collect()
}

/// Advance width of WinAnsi text in Helvetica, in 1/1000 em (Adobe AFM).
fn helvetica_width(bytes: &[u8]) -> f64 {
    const ASCII: [u16; 95] = [
        278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278,
        278, // ' '../
        556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // 0-9
        278, 278, 584, 584, 584, 556, 1015, // :;<=>?@
        667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, // A-M
        722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, // N-Z
        278, 278, 278, 469, 556, 333, // [\]^_`
        556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, // a-m
        556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, // n-z
        334, 260, 334, 584, // {|}~
    ];
    bytes
        .iter()
        .map(|&b| match b {
            0x20..=0x7e => ASCII[(b - 0x20) as usize] as f64,
            0xb7 => 278.0, // periodcentered
            _ => 556.0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_parse() {
        assert_eq!(Color::parse("#e0218a").unwrap(), Color(0xe0, 0x21, 0x8a));
        assert_eq!(Color::parse("#111").unwrap(), Color(0x11, 0x11, 0x11));
        assert!(Color::parse("red").is_err());
        assert!(Color::parse("#12345").is_err());
        assert!(Color::parse("#ggg").is_err());
    }

    #[test]
    fn pdf_is_a_pdf_with_the_page_size() {
        let mut s = Scene::new(612.0, 792.0);
        s.items
            .push(Item::fill(rect(10.0, 10.0, 50.0, 20.0), Color::BLACK));
        s.items.push(Item::Text {
            x: 600.0,
            y: 780.0,
            size: 7.0,
            anchor: Anchor::End,
            color: Color(0x88, 0x88, 0x88),
            text: "8x10 cells · seed 3".into(),
        });
        let pdf = s.to_pdf();
        assert!(pdf.starts_with(b"%PDF-"));
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/MediaBox [0 0 612 792]"), "{text}");
        assert_eq!(s.to_pdf(), pdf, "deterministic");
    }

    #[test]
    fn helvetica_widths_match_the_afm() {
        assert_eq!(helvetica_width(b"A"), 667.0);
        assert_eq!(
            helvetica_width(b"seed 3"),
            500.0 + 556.0 * 3.0 + 278.0 + 556.0
        );
    }
}
