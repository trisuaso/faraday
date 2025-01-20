use compiler::{checking::Registers, process};
use parser::FaradayParser;
use parser::Parser;

use std::env::args;
use std::fs::{read_to_string, write};
use std::process::Command;
use std::time::SystemTime;

fn main() {
    let mut args = args().skip(1);

    let input = args.next().unwrap_or("main.fd".to_string());
    let output = args.next().unwrap_or("out.lua".to_string());
    let output_json = args.next().unwrap_or("out.json".to_string());
    let output_state_json = args.next().unwrap_or("state_out.json".to_string());

    let exec = args.next().unwrap_or("-nr".to_string());
    let run = exec.starts_with("-r=");

    let bench = args.next().unwrap_or("-nb".to_string()) == "-b";

    match FaradayParser::parse(parser::Rule::document, &read_to_string(input).unwrap()) {
        Ok(mut s) => {
            // write file
            write(&output_json, s.to_json()).unwrap();

            let lua = process(s.next().unwrap().into_inner(), Registers::default());
            write(&output, lua.0).unwrap();
            write(
                &output_state_json,
                serde_json::to_string_pretty(&lua.1).unwrap(),
            )
            .unwrap();

            // run
            if run {
                let start = SystemTime::now();
                Command::new(exec.replace("-r=", ""))
                    .arg(output)
                    .spawn()
                    .unwrap()
                    .wait()
                    .unwrap();

                if bench {
                    println!("took: {}μs", start.elapsed().unwrap().as_micros())
                }
            }
        }
        Err(e) => panic!("{}", e),
    };
}
