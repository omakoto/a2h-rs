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
use std::io::*;
use std::cmp::*;
use rustache::*;

pub type W = Fn(&str);

const COLOR_NONE: i32 = -1000;

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


lazy_static!{
    static ref STANDARD_COLORS : [i32;8] = [
        rgb(0, 0, 0),
        rgb(205, 0, 0),
        rgb(0, 205, 0),
        rgb(205, 205, 0),
        rgb(64, 64, 238),
        rgb(205, 0, 205),
        rgb(0, 205, 205),
        rgb(229, 229, 229),
    ];
    static ref INTENSE_COLORS : [i32;8] = [
        rgb(127, 127, 127),
        rgb(255, 0, 0),
        rgb(0, 255, 0),
        rgb(255, 255, 0),
        rgb(92, 92, 255),
        rgb(255, 0, 255),
        rgb(0, 255, 255),
        rgb(255, 255, 255),
    ];
}

fn rgb(r8: i32, g8: i32, b8: i32) -> i32 {
    r8 << 16 | g8 << 8 | b8
}

fn gamma(gamma_value: f64, v: i32) -> i32 {
    let mut x: f64 = ((v as f64) / 255.0).powf(gamma_value);
    if x < 0f64 {
        x = 0f64;
    } else if x > 1f64 {
        x = 1f64;
    }
    (x * 255f64) as i32
}

fn gamma_rgb(gamma_value: f64, rgb888: i32) -> i32 {
    let r8 = gamma(gamma_value, (rgb888 >> 16) & 255);
    let g8 = gamma(gamma_value, (rgb888 >> 8) & 255);
    let b8 = gamma(gamma_value, (rgb888) & 255);
    rgb(r8, g8, b8)
}

fn xterm256_to_rgb(value: i32) -> i32 {
    if value < 8 {
        return get_index_color(value,
                               // bold=
                               false);
    }
    if value < 16 {
        return get_index_color(value,
                               // bold=
                               true);
    }
    if 232 <= value && value <= 256 {
        // Gray
        let level = (value - 232) * 10 + 8;
        return rgb(level, level, level);
    }

    let value = value - 16;

    let b = value % 6;
    let g = (value / 6) % 6;
    let r = (value / 36) % 6;
    rgb(r * 255 / 5, g * 255 / 5, b * 255 / 5)
}

fn get_index_color(index: i32, bold: bool) -> i32 {
    if bold {
        return STANDARD_COLORS[index as usize];
    } else {
        return INTENSE_COLORS[index as usize];
    }
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

fn rgb_to_hex(rgb888: i32) -> String {
    format!("#{:06x}", rgb888)
}

#[test]
fn test_rgb_to_hex() {
    assert_eq!("#000000", rgb_to_hex(0));
    assert_eq!("#0000ff", rgb_to_hex(0xff));
    assert_eq!("#ffffff", rgb_to_hex(0xffffff));
}

const CSI_BUF_SIZE: usize = 10;

pub struct A2hFilter {
    title: String,
    fg_color: String,
    bg_color: String,
    font_size: String,
    gamma: f64,

    /// FG color: positive: rgb, negative: index, or COLOR_NONE
    fg: i32,
    bg: i32,

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
    csi_values: Box<[i32; CSI_BUF_SIZE]>,
    csi_values_len: usize,
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
fn csi_to_rgb(i: usize, csi_vals: &Box<[i32; CSI_BUF_SIZE]>) -> (i32, usize) {
    let mut i = i;
    let mut ret = 0;
    let next = csi_vals[i];
    if next == 5 {
        if i + 1 < csi_vals.len() {
            // Xterm 256 colors
            i += 1;
            ret = xterm256_to_rgb(csi_vals[i]);
            i += 1;
        }
    } else if next == 2 {
        // Kterm 24 bit color
        if i + 3 < csi_vals.len() {
            i += 1;
            ret = rgb(csi_vals[i], csi_vals[i + 1], csi_vals[i + 2]);
            i += 3;
        }
    }
    return (ret, i);
}

impl A2hFilter {
    pub fn new(title: &str,
               fg_color: &str,
               bg_color: &str,
               font_size: &str,
               gamma: f64)
               -> A2hFilter {
        A2hFilter {
            title: title.to_string(),
            fg_color: fg_color.to_string(),
            bg_color: bg_color.to_string(),
            font_size: font_size.to_string(),
            gamma: gamma,

            fg: COLOR_NONE,
            bg: COLOR_NONE,
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
            csi_values: Box::new([0; CSI_BUF_SIZE]),
            csi_values_len: 0,
        }
    }

    pub fn reset(&mut self) {
        self.fg = COLOR_NONE;
        self.bg = COLOR_NONE;
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
        self.fg != COLOR_NONE || self.bg != COLOR_NONE || self.bold || self.faint ||
        self.italic || self.underline || self.blink || self.negative || self.conceal ||
        self.crossout
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

    fn parse_csi_values(&mut self, csi: &[char]) {
        self.csi_values_len = 0;
        let mut val = 0;
        let mut has_val = false;
        for ch in csi {
            if *ch == ';' {
                self.csi_values[self.csi_values_len] = val;
                self.csi_values_len += 1;
                val = 0;
                has_val = false;
            } else if ch.is_digit(10) {
                val *= 10;
                val += (*ch as i32) - ('0' as i32);
                has_val = true;
            } else {
                return;
            }
        }
        if has_val {
            self.csi_values[self.csi_values_len] = val;
            self.csi_values_len += 1;
        }
    }

    fn convert_csi(&mut self, csi: &[char]) {
        self.parse_csi_values(csi);

        let mut i = 0usize;
        while i < self.csi_values_len {
            let code = self.csi_values[i]; // first code
            i += 1;
            if code == 0 {
                self.reset();
            } else if code == 1 {
                self.bold = true;
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
                self.fg = -((code as i32) - 30 + 1); // FG color, index
            } else if 40 <= code && code <= 47 {
                self.bg = -((code as i32) - 40 + 1); // BG color, index
            } else if code == 38 {
                let (fg, next_i) = csi_to_rgb(i, &self.csi_values);
                self.fg = fg as i32;
                i = next_i;
            } else if code == 48 {
                let (bg, next_i) = csi_to_rgb(i, &self.csi_values);
                self.bg = bg as i32;
                i = next_i;
            } else {
                // Unknown
            }
        }

        if !self.has_attr() {
            self.end_span();
            return;
        }

        let mut fg = self.fg;
        let mut bg = self.bg;
        // Convert index color to RGB
        if fg < 0 && fg != COLOR_NONE {
            fg = get_index_color(-fg - 1, self.bold);
        }
        if bg < 0 && bg != COLOR_NONE {
            bg = get_index_color(-bg - 1, false);
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

        // TODO This part should just use integers.
        let mut b = if bg == COLOR_NONE {
            self.bg_color.clone()
        } else {
            rgb_to_hex(gamma_rgb(self.gamma, bg))
        };

        let mut f = if fg == COLOR_NONE {
            self.fg_color.clone()
        } else {
            rgb_to_hex(gamma_rgb(self.gamma, fg))
        };
        if self.negative {
            std::mem::swap(&mut f, &mut b);
        }
        if self.conceal {
            f = b.clone();
        }

        if f != self.fg_color {
            self.add_to_line("color:");
            self.add_to_line(&f);
            self.add_to_line(";");
        }
        if b != self.bg_color {
            self.add_to_line("background-color:");
            self.add_to_line(&b);
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
                            // i += 1;
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
            .insert_string(KEY_FG_COLOR, &self.fg_color)
            .insert_string(KEY_BG_COLOR, &self.bg_color)
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
