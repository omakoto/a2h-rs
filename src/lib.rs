#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
extern crate env_logger;
extern crate rustache;

use std::env;
use std::fmt;
use std::io::Read;
use std::cmp::*;
use rustache::*;

pub type W = Fn(&str);

const KEY_TITLE: &'static str = "title";
const KEY_FG_COLOR: &'static str = "fg_color";
const KEY_BG_COLOR: &'static str = "bg_color";
const KEY_FONT_SIZE: &'static str = "font_size";
const KEY_NUM_ROWS: &'static str = "num_rows";

const HTML_HEADER: &'static str = r##"
<!DOCTYPE html>
<html>
  <head>
    <meta http-equiv="Content-Type" content="text/html; charset=utf-8">
    <title>{{title}}</title>
    <style>
body{
  background-color:{{bg_color}};
  color:{{fg_color}};
}
div{
  font-size:{{font_size}};
  font-family:monospace;
  white-space:pre;
  min-height:{{font_size}};
}
span.blink{
  animation:         blink-animation 1s infinite;
  -webkit-animation: blink-animation 1s infinite;
}
@keyframes blink-animation {
  0% { visibility: hidden; }
  50% { visibility: hidden; }
}
@-webkit-keyframes blink-animation {
  0% { visibility: hidden; }
  50% { visibility: hidden; }
}
    </style>
  <head>
<body>
"##;

const HTML_FOOTER: &'static str = r##"
<!-- {{num_rows}} rows -->
</body>
</html>
"##;

fn gamma(gamma_value: f64, v: i32) -> i32 {
    let mut x: f64 = ((v as f64) / 255.0).powf(gamma_value);
    if x < 0f64 {
        x = 0f64;
    } else if x > 1f64 {
        x = 1f64;
    }
    (x * 255f64) as i32
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum Color {
    /// No color.
    None,
    /// Index color, 0-7.
    Index { index: i32, bold: bool },
    Rgb { r: i32, g: i32, b: i32 },
}

impl Color {
    pub fn from_hex(rrggbb: &str) -> Result<Color, String> {
        match u32::from_str_radix(rrggbb, 16) {
            Ok(v) => Ok(Color::from_int(v as i32)),
            Err(e) => Err(format!("Invalid color '{}'; expected RRGGBB", rrggbb)),
        }
    }

    pub fn from_int(rgb: i32) -> Color {
        Color::from_rgb((rgb >> 16), (rgb >> 8), rgb)
    }

    pub fn from_rgb(r: i32, g: i32, b: i32) -> Color {
        Color::Rgb {
            r: r & 0xff,
            g: g & 0xff,
            b: b & 0xff,
        }
    }

    pub fn from_index(i: i32, bold: bool) -> Color {
        Color::Index {
            index: i,
            bold: bold,
        }
    }

    fn apply_bold(&self, bold: bool) -> Color {
        match self {
            &Color::Index { index, bold } => Color::from_index(index, bold),
            _ => return *self,
        }
    }

    fn or_default(&self, def: Color) -> Color {
        return if *self == Color::None { def } else { *self };
    }

    fn _apply_gamma(&self, gamma_value: f64) -> Color {
        match self {
            &Color::Rgb { r, g, b } => {
                let r = gamma(gamma_value, r);
                let g = gamma(gamma_value, g);
                let b = gamma(gamma_value, b);
                return Color::from_rgb(r, g, b);
            }
            &Color::Index { index, bold } => self._to_rgb()._apply_gamma(gamma_value),
            _ => return *self,
        }
    }

    fn _to_rgb(&self) -> Color {
        match self {
            &Color::Index { index, bold } => {
                if bold {
                    return INTENSE_COLORS[index as usize];
                } else {
                    return STANDARD_COLORS[index as usize];
                }
            }
            _ => return *self,
        }
    }

    fn from_xterm256(value: i32) -> Color {
        if value < 8 {
            return Color::from_index(value, false);
        }
        if value < 16 {
            return Color::from_index(value - 8, true);
        }
        if 232 <= value && value <= 256 {
            // Gray
            let level = (value - 232) * 10 + 8;
            return Color::from_rgb(level, level, level);
        }

        let value = value - 16;

        let b = value % 6;
        let g = (value / 6) % 6;
        let r = (value / 36) % 6;
        Color::from_rgb(r * 255 / 5, g * 255 / 5, b * 255 / 5)
    }

    fn to_int(&self) -> i32 {
        match self {
            &Color::Rgb { r, g, b } => {
                return (r << 16) as i32 | (g << 8) as i32 | b;
            }
            &Color::Index { index, bold } => self._to_rgb().to_int(),
            _ => panic!("Can't get rgb from Color::None"),
        }
    }

    fn to_css_color(&self, gamma: f64) -> String {
        format!("#{:06x}", self._apply_gamma(gamma).to_int())
    }
}

#[test]
fn test_to_css_color() {
    assert_eq!("#000000", Color::from_int(0).to_css_color(1.0));
    assert_eq!("#000080", Color::from_int(0x80).to_css_color(1.0));
    assert_eq!("#0000ff", Color::from_int(0xff).to_css_color(1.0));
    assert_eq!("#ffffff", Color::from_int(0xffffff).to_css_color(1.0));

    assert_eq!("#0000b4", Color::from_int(0x80).to_css_color(0.5));
    assert_eq!("#00005a", Color::from_int(0x80).to_css_color(1.5));
}

lazy_static!{
    static ref STANDARD_COLORS : [Color;8] = [
        Color::from_rgb(0, 0, 0),
        Color::from_rgb(205, 0, 0),
        Color::from_rgb(0, 205, 0),
        Color::from_rgb(205, 205, 0),
        Color::from_rgb(64, 64, 238),
        Color::from_rgb(205, 0, 205),
        Color::from_rgb(0, 205, 205),
        Color::from_rgb(229, 229, 229),
    ];
    static ref INTENSE_COLORS : [Color;8] = [
        Color::from_rgb(127, 127, 127),
        Color::from_rgb(255, 0, 0),
        Color::from_rgb(0, 255, 0),
        Color::from_rgb(255, 255, 0),
        Color::from_rgb(92, 92, 255),
        Color::from_rgb(255, 0, 255),
        Color::from_rgb(0, 255, 255),
        Color::from_rgb(255, 255, 255),
    ];
}

fn parse_int(s: &str, def: i32) -> i32 {
    s.parse::<i32>().unwrap_or(def)
}

#[test]
fn test_parse_int() {
    assert_eq!(0, parse_int("0", 999));
    assert_eq!(999, parse_int("", 999));
    assert_eq!(255, parse_int("255", 999));
}

pub struct A2hFilter {
    /// HTML title
    title: String,
    /// HTML fg color
    html_fg_color: Color,
    /// HTML bg color
    html_bg_color: Color,
    /// HTML font size
    font_size: String,
    /// Gomma for RGB conversion
    gamma: f64,

    /// FG color: positive: rgb, negative: index, or COLOR_NONE
    fg: Color,
    bg: Color,

    bold: bool,
    faint: bool,
    italic: bool,
    underline: bool,
    blink: bool,
    negative: bool,
    conceal: bool,
    crossout: bool,

    in_div: bool,
    in_span: bool,

    num_rows: usize,

    line_buf: String,
}

fn peek(line: &Vec<char>, index: usize) -> char {
    if index < line.len() {
        return line[index];
    } else {
        return '\x00';
    }
}

fn is_csi_end(b: char) -> bool {
    return '\x40' <= b && b <= '\x7e';
}

// TODO Make it a member.
fn csi_to_color(i: usize, csi_vals: &[i32]) -> (Color, usize) {
    let mut i = i;
    let mut ret = Color::None;
    let next = csi_vals[i];
    if next == 5 {
        if i + 1 < csi_vals.len() {
            // Xterm 256 colors
            i += 1;
            ret = Color::from_xterm256(csi_vals[i]);
            i += 1;
        }
    } else if next == 2 {
        // Kterm 24 bit color
        if i + 3 < csi_vals.len() {
            i += 1;
            ret = Color::from_rgb(csi_vals[i], csi_vals[i + 1], csi_vals[i + 2]);
            i += 3;
        }
    }
    return (ret, i);
}

impl A2hFilter {
    pub fn new(title: &str, fg_rgb: Color, bg_rgb: Color, font_size: &str, gamma: f64) -> A2hFilter {
        A2hFilter {
            title: title.to_string(),
            html_fg_color: fg_rgb,
            html_bg_color: bg_rgb,
            font_size: font_size.to_string(),
            gamma: gamma,

            fg: Color::None,
            bg: Color::None,
            bold: false,
            faint: false,
            italic: false,
            underline: false,
            blink: false,
            negative: false,
            conceal: false,
            crossout: false,

            in_div: false,
            in_span: false,

            num_rows: 0,

            line_buf: String::new(),
        }
    }

    pub fn reset(&mut self) {
        self.fg = Color::None;
        self.bg = Color::None;
        self.bold = false;
        self.faint = false;
        self.italic = false;
        self.underline = false;
        self.blink = false;
        self.negative = false;
        self.conceal = false;
        self.crossout = false;
    }

    fn has_attr(&self) -> bool {
        self.fg != Color::None || self.bg != Color::None || self.bold || self.faint ||
        self.italic || self.underline || self.blink || self.negative ||
        self.conceal || self.crossout
    }

    fn add_char_to_line(&mut self, ch: char) {
        self.line_buf.push(ch);
    }

    fn add_to_line(&mut self, v: &str) {
        self.line_buf.push_str(v);
    }

    fn flush_line(&mut self, writer: &W) {
        writer(&self.line_buf);
        self.line_buf.clear();
    }

    fn start_div(&mut self) {
        if !self.in_div {
            self.in_div = true;
            self.add_to_line("<div>");
            self.num_rows += 1;
        }
    }

    fn end_div(&mut self, writer: &W) {
        self.end_span();
        if self.in_div {
            self.in_div = false;
            self.add_to_line("</div>\n");
            self.flush_line(writer);
        }
    }

    fn start_span(&mut self) {
        if !self.in_span {
            self.in_span = true;
            self.add_to_line("<span>");
        }
    }

    fn end_span(&mut self) {
        if self.in_span {
            self.in_span = false;
            self.add_to_line("</span>");
        }
    }

    fn parse_csi_values(&mut self, csi: &[char], out: &mut [i32], out_len: &mut usize) {
        *out_len = 0;
        let mut val = 0;
        let mut has_val = false;
        for ch in csi {
            if *ch == ';' {
                out[*out_len] = val;
                *out_len += 1;
                val = 0;
                has_val = false;
                if *out_len >= out.len() {
                    break;
                }
            } else if ch.is_digit(10) {
                val *= 10;
                val += (*ch as i32) - ('0' as i32);
                has_val = true;
            } else {
                break;
            }
        }
        if has_val {
            out[*out_len] = val;
            *out_len += 1;
        }
        // Special case, ESC[m -> same as ESC[0m.
        if *out_len == 0 {
            *out_len = 1;
            out[0] = 0;
        }
    }

    fn convert_csi(&mut self, csi: &[char]) {
        let mut values = [0; 10];
        let mut values_len = 0;
        self.parse_csi_values(csi, &mut values, &mut values_len);

        let mut i = 0usize;
        while i < values_len {
            let code = values[i]; // first code
            i += 1;
            if code == 0 {
                self.reset();
            } else if code == 1 {
                self.bold = true;
                self.fg = self.fg.apply_bold(true);
            } else if code == 2 {
                self.faint = true;
            } else if code == 3 {
                self.italic = true;
            } else if code == 4 {
                self.underline = true;
            } else if code == 5 {
                self.blink = true;
            } else if code == 7 {
                self.negative = true;
            } else if code == 8 {
                self.conceal = true;
            } else if code == 9 {
                self.crossout = true;
            } else if code == 21 {
                self.bold = false;
                self.fg = self.fg.apply_bold(false);
            } else if code == 22 {
                self.bold = false;
                self.faint = false;
            } else if code == 23 {
                self.italic = false;
            } else if code == 24 {
                self.underline = false;
            } else if code == 25 {
                self.blink = false;
            } else if code == 27 {
                self.negative = false;
            } else if code == 28 {
                self.conceal = false;
            } else if code == 29 {
                self.crossout = false;
            } else if 30 <= code && code <= 37 {
                self.fg = Color::from_index((code as i32) - 30, self.bold);
            } else if 40 <= code && code <= 47 {
                self.bg = Color::from_index((code as i32) - 40, false);
            } else if code == 38 {
                let (fg, next_i) = csi_to_color(i, &values);
                self.fg = fg;
                i = next_i;
            } else if code == 48 {
                let (bg, next_i) = csi_to_color(i, &values);
                self.bg = bg;
                i = next_i;
            } else {
                // Unknown
            }
        }

        if !self.has_attr() {
            self.end_span();
            return;
        }

        self.end_span();

        self.in_span = true;
        self.add_to_line("<span ");
        if self.blink {
            self.add_to_line("class=\"blink\" ");
        }
        self.add_to_line("style=\"");

        if self.bold {
            self.add_to_line("font-weight:bold;");
        }
        if self.faint {
            self.add_to_line("opacity:0.5;");
        }
        if self.italic {
            self.add_to_line("font-style:italic;");
        }
        if self.underline {
            self.add_to_line("text-decoration:underline;");
        }
        if self.crossout {
            self.add_to_line("text-decoration:line-through;");
        }

        let mut f = self.fg.or_default(self.html_fg_color);
        let mut b = self.bg.or_default(self.html_bg_color);

        if self.negative {
            std::mem::swap(&mut f, &mut b);
        }
        if self.conceal {
            f = b;
        }

        let gamma = self.gamma;
        if f != self.html_fg_color {
            self.add_to_line("color:");
            self.add_to_line(&f.to_css_color(gamma));
            self.add_to_line(";");
        }
        if b != self.html_bg_color {
            self.add_to_line("background-color:");
            self.add_to_line(&b.to_css_color(gamma));
            self.add_to_line(";");
        }
        self.add_to_line("\">");
    }

    fn convert(&mut self, line: &str, writer: &W) {
        self.start_div();

        let chars = line.chars().collect::<Vec<char>>();
        let size = chars.len();

        let mut i = 0;
        'outer: while i < size {
            let ch = chars[i];
            match ch {
                '&' => {
                    self.add_to_line("&amp;");
                    i += 1;
                    continue;
                }
                '<' => {
                    self.add_to_line("&lt;");
                    i += 1;
                    continue;
                }
                '>' => {
                    self.add_to_line("&gt;");
                    i += 1;
                    continue;
                }
                '\x07' => {
                    // bell, ignore.
                    i += 1;
                    continue;
                }
                '\x0a' | '\x0d' => {
                    if ch == '\x0d' && peek(&chars, i + 1) == '\x0a' {
                        // CR followed by LF
                        i += 1;
                    }
                    self.end_div(writer);
                    i += 1;
                    if peek(&chars, i) != '\x00' {
                        self.start_div();
                        continue;
                    } else {
                        break;
                    }
                }
                '\x1b' => {
                    i += 1;
                    match peek(&chars, i) {
                        '\x00' => {
                            break;
                        }
                        '[' => {
                            // CSI
                            i += 1;
                            let csi_start = i;
                            while i < size && !is_csi_end(chars[i]) {
                                i += 1;
                            }
                            if i >= size {
                                break;
                            }
                            if chars[i] == 'm' {
                                self.convert_csi(&chars[csi_start..i]);
                            }
                            i += 1;
                            continue;
                        }
                        ']' => {
                            i += 1;
                            loop {
                                let n = peek(&chars, i);
                                // In xterm, they may also be terminated by BEL
                                if n == '\x00' || n == '\x07' {
                                    i += 1;
                                    continue 'outer;
                                }
                                // terminated by ST ( ESC \ )
                                if n == '\x1b' && peek(&chars, i + 1) == '\\' {
                                    i += 2;
                                    continue 'outer;
                                }
                                i += 1;
                            }
                            unreachable!();
                        }
                        '(' => {
                            // VT100 Code: e.g. ESC ( A
                            i += 2;
                            continue;
                        }
                        'c' => {
                            // "Reset to Intitial State"
                            self.reset();
                            self.end_span();
                            i += 1;
                            continue;
                        }
                        _ => {
                            // unknown.
                            continue;
                        }
                    }
                }
                _ => {}
            }
            if '\x00' <= ch && ch <= '\x1f' && ch != '\t' {
                // Control character.
                self.add_to_line("^");
                let ch: char = ((ch as u8) + ('@' as u8)) as char;
                self.add_char_to_line(ch);
            } else {
                self.add_char_to_line(ch);
            }
            i += 1;
        }
        self.end_div(writer);
    }


    pub fn write_header<W>(&self, writer: &W)
        where W: Fn(&str)
    {
        let data = HashBuilder::new()
            .insert_string(KEY_TITLE, &self.title)
            .insert_string(KEY_FG_COLOR, &self.html_fg_color.to_css_color(self.gamma))
            .insert_string(KEY_BG_COLOR, &self.html_bg_color.to_css_color(self.gamma))
            .insert_string(KEY_FONT_SIZE, &self.font_size);

        let mut s: String = String::new();
        rustache::render_text(HTML_HEADER, data)
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        writer(&s);
    }

    pub fn write_footer(&self, writer: &W) {
        let data = HashBuilder::new().insert_string(KEY_NUM_ROWS, &self.num_rows);

        let mut s: String = String::new();
        rustache::render_text(HTML_FOOTER, data)
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        writer(&s);
    }

    pub fn process(&mut self, s: &str, writer: &W) {
        self.convert(s, writer);
    }
}
