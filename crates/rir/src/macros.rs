pub fn icompiler_error(args: std::fmt::Arguments) -> String {
    let string = if let Some(s) = args.as_str() {
        s.to_string()
    } else {
        args.to_string()
    };

    return string;
}

#[macro_export]
macro_rules! icompiler_error {
    ($($arg:tt)*) => {
        {
            let marker = $crate::COMPILER_MARKER.lock().unwrap();

            println!(
                "\x1b[31;1merror:\x1b[0m \x1b[1m{}\x1b[0m\n    \x1b[2maround {}\x1b[0m\n    \x1b[2mto {}\x1b[0m",
                $crate::macros::icompiler_error(std::format_args!($($arg)*)),
                marker.0,
                marker.1
            );

            std::process::exit(1);
        }
    }
}
