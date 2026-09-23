//! Milestone 0 spike: a Blitz document on a layer-shell surface. See `SPIKE.md` (S1).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = shell_host::spike::app::parse_args(&args).and_then(shell_host::spike::app::run);
    if let Err(e) = result {
        eprintln!("spike_bar: {e}");
        std::process::exit(1);
    }
}
