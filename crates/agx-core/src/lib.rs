//! Read-only Linux system information shared by AGX interfaces.
mod models;
mod parsers;
mod sampler;
mod vnc;
/// Observed VNC listeners and viewers, read from the kernel socket tables.
pub use vnc::collect as vnc_status;

pub use models::{CpuTicks, Hardware, Memory, Network, Snapshot, Storage, VncStatus};
pub use parsers::{cpu_usage, parse_cpu, parse_memory};
pub use sampler::Sampler;

#[cfg(test)]
mod tests;
