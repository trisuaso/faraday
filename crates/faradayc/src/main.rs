use compiler::checking::Registers;
use compiler::process_file;
use pathbufd::PathBufD as PathBuf;
use std::env::args;
use std::fs::write;
use std::process::Command;
use std::time::SystemTime;

fn main() {
    let mut args = args().skip(1);

    let input = args.next().unwrap_or("main.fd".to_string());

    let exec = args.next().unwrap_or("-nr".to_string());
    let run = exec.starts_with("-r=");

    let bench = args.next().unwrap_or("-nb".to_string()) == "-b";

    // create build dir
    let out_path = PathBuf::current().extend(&["build", "main.lua"]);
    let parent = out_path.as_path().parent().unwrap();

    // if !parent.exists() {
    // std::fs::create_dir_all(parent).unwrap();
    // }
    std::fs::remove_dir_all(parent).unwrap();
    std::fs::create_dir_all(parent).unwrap();

    // process
    let output = process_file(PathBuf::current().join(input), Registers::default());

    // write file
    write(&out_path, output.0).unwrap();

    // run
    if run {
        let start = SystemTime::now();
        Command::new(exec.replace("-r=", ""))
            .arg(&out_path.to_string())
            .current_dir("build")
            .spawn()
            .unwrap()
            .wait()
            .unwrap();

        if bench {
            println!("took: {}μs", start.elapsed().unwrap().as_micros())
        }
    }
}
