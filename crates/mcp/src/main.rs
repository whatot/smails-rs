use std::process;

fn main() {
    if let Err(err) = smails_mcp::run_stdio() {
        eprintln!("{err}");
        process::exit(1);
    }
}
