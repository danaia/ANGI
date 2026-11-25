//! Wire protocol, link policy and device identity shared by agx-securityd, the desktop's
//! Settings and first-boot setup.
mod client;
mod fingerprint;
mod identity;
mod links;
mod paths;
mod policy;
mod vnc;
mod protocol;

/// Socket served by agx-securityd. `AGX_SECURITY_SOCKET` redirects it for desktop previews.
pub const SOCKET: &str = "/run/agx/security.sock";
pub const SOCKET_ENV: &str = "AGX_SECURITY_SOCKET";
pub const MAX_REQUEST: usize = 4096;

pub use client::{request, socket_path};
pub use fingerprint::sha256_fingerprint;
pub use identity::{Identity, ensure_client_key, host_public_key};
pub use links::{Interface, classify, interfaces};
pub use paths::Paths;
pub use policy::{Policy, atomic_write as write_atomic};
pub use vnc::{set_vnc, vnc_enabled};
pub use protocol::{Kind, Link, Peer, Reply, Request, Status};

#[cfg(test)]
mod tests;
