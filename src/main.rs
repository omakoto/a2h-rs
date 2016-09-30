#[macro_use]
extern crate log;
extern crate env_logger;
extern crate fileinput;
extern crate a2h;
#[macro_use]
extern crate clap;

use clap::{Arg, App, SubCommand, Shell};
use std::cmp::max;
use std::io;
use std::io::BufReader;
use std::io::prelude::*;
use std::env;
use std::sync::mpsc::*;
use std::thread;
use std::sync::*;

use fileinput::FileInput;

use a2h::*;

fn error(message: &String) {
    writeln!(&mut std::io::stderr(),
             "{}: {}",
             env::args().nth(0).unwrap(),
             message);
}

const FLAG_AUTO_FLUSH: &'static str = "auto-flush";
const FLAG_BASHCOMP: &'static str = "bash-completion";
const FLAG_TITLE: &'static str = "title";
const FLAG_GAMMA: &'static str = "gamma";
const FLAG_BG_COLOR: &'static str = "bg-color";
const FLAG_FG_COLOR: &'static str = "fg-color";
const FLAG_FONT_SIZE: &'static str = "font-size";
const FLAG_FILES: &'static str = "files";

fn get_app<'a, 'b>() -> App<'a, 'b> {
    App::new("A2H")
        .version("0.1")
        .author("Makoto Onuki <makoto.onuki@gmail.com>")
        .about("Regex based text highlighter")
        .arg(Arg::with_name(FLAG_AUTO_FLUSH)
            .short("f")
            .long(FLAG_AUTO_FLUSH)
            .help("Auto-flush stdout"))
        .arg(Arg::with_name(FLAG_BASHCOMP)
            .long(FLAG_BASHCOMP)
            .help("Print bash completion script"))
        .arg(Arg::with_name(FLAG_TITLE)
            .short("t")
            .long(FLAG_TITLE)
            .default_value("a2h")
            .takes_value(true)
            .help("Set HTML title"))
        .arg(Arg::with_name(FLAG_GAMMA)
            .short("g")
            .long(FLAG_GAMMA)
            .default_value("1.0")
            .takes_value(true)
            .help("Gamma value for RGB conversion"))
        .arg(Arg::with_name(FLAG_BG_COLOR)
            .short("b")
            .long(FLAG_BG_COLOR)
            .default_value("#000000")
            .takes_value(true)
            .help("Background color"))
        .arg(Arg::with_name(FLAG_FG_COLOR)
            .short("c")
            .long(FLAG_FG_COLOR)
            .default_value("#ffffff")
            .takes_value(true)
            .help("Foreground color"))
        .arg(Arg::with_name(FLAG_FONT_SIZE)
            .short("s")
            .long(FLAG_FONT_SIZE)
            .default_value("9pt")
            .takes_value(true)
            .help("Text size"))
        .arg(Arg::with_name(FLAG_FILES)
            .index(1)
            .required(false)
            .multiple(true)
            .help("Input files"))
}

fn read_main() -> Result<(), String> {
    env_logger::init().unwrap();

    let matches = get_app().get_matches();
    if matches.is_present(FLAG_BASHCOMP) {
        get_app().gen_completions_to("a2h", Shell::Bash, &mut io::stdout());
        return Ok(());
    }

    let args: Vec<String> = env::args().collect();
    let program = &args[0];

    let auto_flush = matches.is_present(FLAG_AUTO_FLUSH);

    let title = matches.value_of(FLAG_TITLE).unwrap();
    let gamma = value_t!(matches, FLAG_GAMMA, f64).unwrap();
    let fg_color = matches.value_of(FLAG_FG_COLOR).unwrap();
    let bg_color = matches.value_of(FLAG_BG_COLOR).unwrap();
    let font_size = matches.value_of(FLAG_FONT_SIZE).unwrap();

    let mut files: Vec<String> = vec![];
    if let Some(arg_files) = matches.values_of("files") {
        for f in arg_files {
            files.push(f.to_string());
        }
    }

    // This works.
    let fileinput = FileInput::new(&files);
    let reader = BufReader::new(fileinput);

    let writer = move |out: &str| {
        print!("{}", out);
        if auto_flush {
            io::stdout().flush();
        }
    };

    // TODO Actually pass the FG/BG.
    let mut filter = A2hFilter::new(&title, 0xffffff, 0x000000, &font_size, gamma);

    filter.write_header(&writer);

    for line in reader.lines() {
        match line {
            Err(e) => {
                match e.kind() {
                    std::io::ErrorKind::InvalidData => continue, // OK
                    _ => return Err(format!("{}", e)),
                }
            }
            Ok(s) => filter.process(&s, &writer),
        }
    }

    filter.write_footer(&writer);

    return Ok(());
}

fn main() {
    match read_main() {
        Ok(_) => return, // okay
        Err(err) => {
            error(&err);
           std::process::exit(1);
        }
    }
}
