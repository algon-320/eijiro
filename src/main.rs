use anyhow::{anyhow, ensure, Result};
use clap::{App, Arg, ArgMatches, SubCommand};
use std::io::prelude::*;
use std::path::{Path, PathBuf};

use eijiro_parser::fst;
use fst::{IntoStreamer, Streamer};

use log::{error, info, warn};

fn printer(key: &str, field: &eijiro_parser::Field) -> String {
    format!(
        "{} {{{}}} : {}{}{}",
        key,
        field.ident.as_ref().unwrap_or(&"".to_string()),
        field.explanation.body,
        field
            .explanation
            .complements
            .iter()
            .fold("".to_string(), |mut p, c| {
                p += &format!("◆{}", c.body);
                p
            }),
        field.examples.iter().fold("".to_string(), |mut p, e| {
            p += &format!("\n        {}", e.sentence);
            p
        })
    )
}

fn main() {
    pretty_env_logger::init();
    let app = App::new("eijiro-rs")
        .version("0.1.0")
        .author("algon-320 <algon.0320@mail.com>")
        .about("English-Japanese dictionary (using Eijiro)")
        .arg(Arg::with_name("word").required(true));
    let matches = app.get_matches();
    let word = matches.value_of("word").unwrap();

    let dict = match std::fs::read("./dict_dump.bincode") {
        Ok(bytes) => {
            info!("Loading dict");
            let dict = bincode::deserialize(&bytes).unwrap();
            info!("Loaded dict");
            dict
        }
        Err(_) => {
            info!("Parse EIJIRO.txt");
            let dict_str = std::fs::read_to_string("./EIJIRO.txt").unwrap();
            let dict = eijiro_parser::parse(dict_str.as_str()).unwrap();
            let _ = std::fs::write("./dict_dump.bincode", bincode::serialize(&dict).unwrap());
            dict
        }
    };

    let matcher = fst::automaton::Levenshtein::new(word, 0).unwrap();
    let mut stream = dict.keys.search(&matcher).into_stream();
    let mut hit_idx = 0;
    while let Some((k, idx)) = stream.next() {
        let item = std::str::from_utf8(k).unwrap();
        // println!("{}: {} : {:#?}", hit_idx, item, &dict.fields[idx as usize]);
        for f in &dict.fields[idx as usize] {
            println!("[{:3}] {}", hit_idx, printer(item, f));
            hit_idx += 1;
        }
    }
}
