//! Milestone 0 spike: a maximized checkerboard toplevel that logs its configures (item a).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = shell_host::spike::probe::run(&args) {
        eprintln!("probe_toplevel: {e}");
        std::process::exit(1);
    }
}
