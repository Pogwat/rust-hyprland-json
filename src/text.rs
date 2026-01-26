pub const HELP: &str = "\
rust-hyprland-json

USAGE:
  app [OPTIONS] 

FLAGS:
  -h, --help            Prints help information
  -v, --version         Prints version

OPTIONS:
  -a, --all

ARGS:
  -p, --path            [path to hyprland IPC socket]
";

pub const VERSION: &str = env!("CARGO_PKG_VERSION");   