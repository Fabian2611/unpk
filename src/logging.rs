#[macro_export] macro_rules! info {
    ($($arg:tt)*) => {{
        println!("\x1b[1;34m[\x1b[1;32mINFO\x1b[1;34m]\x1b[0m {}", format_args!($($arg)*));
    }};
}

#[macro_export] macro_rules! warn {
    ($($arg:tt)*) => {{
        println!("\x1b[1;34m[\x1b[1;33mWARN\x1b[1;34m]\x1b[0m {}", format_args!($($arg)*));
    }};
}

#[macro_export] macro_rules! error {
    ($($arg:tt)*) => {{
        println!("\x1b[1;34m[\x1b[1;31mERROR\x1b[1;34m]\x1b[0m {}", format_args!($($arg)*));
    }};
}
