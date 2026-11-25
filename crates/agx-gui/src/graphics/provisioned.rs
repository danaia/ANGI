//! Test whether the optional userspace required by the accelerated backend can load.
pub(crate) fn runtime_available() -> bool {
    // The signed installer installs these into the system's library directories.
    // Drop the probe handles before the selected renderer opens its own copies.
    ["libgbm.so.1", "libEGL.so.1"]
        .into_iter()
        .all(|name| unsafe { libloading::Library::new(name) }.is_ok())
}
