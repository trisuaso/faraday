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

    // create build dir
    let out_path = PathBuf::current().extend(&["build", "main.lua"]);
    let parent = out_path.as_path().parent().unwrap();

    // if !parent.exists() {
    // std::fs::create_dir_all(parent).unwrap();
    // }
    std::fs::remove_dir_all(parent).unwrap();
    std::fs::create_dir_all(parent).unwrap();

    // process
    let start = SystemTime::now();
    let output = process_file(PathBuf::current().join(&input), Registers::default(), false);

    // finished
    let micros = start.elapsed().unwrap().as_micros();
    let gap = "-".repeat(((micros / 50) as usize) / 2);

    println!("🦇 \x1b[91m{} end {}\x1b[0m 🦖", gap, gap);

    println!(
        "    \x1b[32;1mFinished\x1b[0m \x1b[2m{input}\x1b[0m in \x1b[1m{}μs ({:.4}s)\x1b[0m",
        micros,
        start.elapsed().unwrap().as_secs_f32()
    );

    // write file
    write(&out_path, output.0).unwrap();
    println!("       \x1b[32;1mSaved\x1b[0m \x1b[2m{out_path}\x1b[0m");

    // run
    if run {
        let mut pre_cmd = Command::new(exec.replace("-r=", ""));
        let cmd = pre_cmd.arg(&out_path.to_string()).current_dir("build");

        // pretty print cmd
        let mut args: String = String::new();

        for arg in cmd.get_args() {
            args.push_str(&format!("{} ", arg.to_str().unwrap().to_string()));
        }

        println!(
            "     \x1b[32;1mRunning\x1b[0m \x1b[2m{} {args}\x1b[0m",
            cmd.get_program().to_str().unwrap().to_string(),
        );

        // run
        println!("🦇 \x1b[92m{} run {}\x1b[0m 🌑", gap, gap);
        cmd.spawn().unwrap().wait().unwrap();
    }
}
