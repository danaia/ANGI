//! agx-securityd: per-link SSH access, this computer's public key and peer pinning.
mod ctl;
mod gate;
mod peer;
mod server;
#[cfg(test)]
#[path = "daemon_tests.rs"]
mod tests;

fn main() {
    // With arguments this is the command-line client for the running service.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        std::process::exit(ctl::run(&args));
    }
    if let Err(error) = server::run() {
        eprintln!("agx-securityd: {error}");
        std::process::exit(1);
    }
}
