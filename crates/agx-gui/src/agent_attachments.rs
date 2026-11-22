//! Files and images attached to the next Chat message.
//!
//! The composer keeps a small collection the user can add to (plus button, Files picker, Ctrl+V),
//! view, replace and remove before sending. Every attachment is copied into a private staging
//! folder, so a later change to the original cannot alter what Angi receives, and the runtime
//! only accepts files from that folder (`apps/agent/chat.py`). Thumbnails are decoded from PNG;
//! other formats are still sent, shown as a file tile.
mod folder;

use crate::{AgentChat, AppWindow, ChatAttachment};
use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, Timer, TimerMode, VecModel};
use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    rc::Rc,
    sync::{mpsc, Mutex},
    time::Duration,
};

pub(crate) const MAX_ATTACHMENTS: usize = 8;
const MAX_BYTES: u64 = 20 * 1024 * 1024;
const PREVIEW: u32 = 720;
const IDLE_PURGE: Duration = Duration::from_secs(7 * 24 * 3600);

/// Paths most recently copied in ANGI Files; Ctrl+V in the composer attaches them.
static COPIED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

pub(crate) fn offer_copied(paths: &[PathBuf]) {
    if let Ok(mut copied) = COPIED.lock() {
        *copied = paths.to_vec();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Staged {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    /// A summary of a folder (see `folder.rs`), not a copied file.
    pub folder: bool,
}

/// Stage whatever the user chose: a file is copied, a folder becomes a text summary.
pub(crate) fn stage_path(source: &Path, dir: &Path) -> Result<Staged, String> {
    if source.is_dir() {
        stage_folder(source, dir)
    } else {
        stage_file(source, dir)
    }
}

pub(crate) fn stage_folder(source: &Path, dir: &Path) -> Result<Staged, String> {
    let source = fs::canonicalize(source).map_err(|_| "That folder cannot be read.".to_string())?;
    let label = source.file_name().map_or_else(|| "root".into(), |n| n.to_string_lossy().into_owned());
    fs::read_dir(&source).map_err(|_| format!("“{label}” cannot be read."))?;
    ensure_dir(dir)?;
    let text = folder::manifest(&source);
    let target = unique_path(dir, &format!("folder-{label}.md"));
    fs::write(&target, &text).map_err(|_| format!("“{label}” could not be attached."))?;
    Ok(Staged { name: target.file_name().unwrap().to_string_lossy().into_owned(), path: target, size: text.len() as u64, folder: true })
}

/// Where the agent runtime keeps one account's state. `agent` (apps/agent/agent.sh) runs with this
/// as `XDG_STATE_HOME`, and only accepts attachments staged under it, so the desktop must stage
/// there too. Without the shared data volume (a desktop preview) the ordinary state folder is used.
fn agent_state_dir() -> Option<PathBuf> {
    let users = Path::new("/var/lib/agx-data/agx-opencode-users");
    // SAFETY: getuid has no preconditions and cannot fail.
    users.is_dir().then(|| users.join(unsafe { libc::getuid() }.to_string()).join("state"))
}

pub(crate) fn staging_dir() -> PathBuf {
    if let Some(state) = agent_state_dir() {
        return state.join("angi-swarm/chat-attachments");
    }
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    state.join("angi-swarm/chat-attachments")
}

fn has_png_signature(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a])
}

/// A name that is safe as one path component and keeps its extension readable.
fn clean_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':' | '\0') { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').to_string();
    if cleaned.is_empty() { "attachment".into() } else { cleaned.chars().take(120).collect() }
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let name = clean_name(name);
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.clone(), String::new()),
    };
    let mut candidate = dir.join(&name);
    let mut n = 1;
    while candidate.exists() {
        candidate = dir.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    candidate
}

fn ensure_dir(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("Cannot prepare attachments: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

/// Copy a regular file the user chose into the staging folder.
pub(crate) fn stage_file(source: &Path, dir: &Path) -> Result<Staged, String> {
    let label = source.file_name().map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned());
    let meta = fs::metadata(source).map_err(|_| format!("“{label}” cannot be read."))?;
    if !meta.is_file() {
        return Err(format!("“{label}” is not a file."));
    }
    if meta.len() > MAX_BYTES {
        return Err(format!("“{label}” is larger than 20 MB."));
    }
    ensure_dir(dir)?;
    let target = unique_path(dir, &label);
    fs::copy(source, &target).map_err(|_| format!("“{label}” could not be attached."))?;
    Ok(Staged { name: target.file_name().unwrap().to_string_lossy().into_owned(), path: target, size: meta.len(), folder: false })
}

/// Save pasted image bytes (PNG) as a new attachment.
pub(crate) fn stage_bytes(bytes: &[u8], dir: &Path) -> Result<Staged, String> {
    if !has_png_signature(bytes) {
        return Err("The clipboard image is not a PNG.".into());
    }
    if bytes.len() as u64 > MAX_BYTES {
        return Err("The pasted image is larger than 20 MB.".into());
    }
    ensure_dir(dir)?;
    let target = unique_path(dir, "pasted-image.png");
    fs::write(&target, bytes).map_err(|_| "The pasted image could not be saved.".to_string())?;
    Ok(Staged { name: target.file_name().unwrap().to_string_lossy().into_owned(), path: target, size: bytes.len() as u64, folder: false })
}

/// Decode a PNG to RGBA8 no larger than `limit` on its long side (nearest-neighbour).
pub(crate) fn decode_png(bytes: &[u8], limit: u32) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buffer).ok()?;
    let (w, h) = (frame.width, frame.height);
    if w == 0 || h == 0 {
        return None;
    }
    let channels = match frame.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        png::ColorType::Indexed => return None,
    };
    let scale = (w.max(h) as f32 / limit as f32).max(1.0);
    let (tw, th) = (((w as f32 / scale) as u32).max(1), ((h as f32 / scale) as u32).max(1));
    let mut out = Vec::with_capacity((tw * th * 4) as usize);
    for y in 0..th {
        let sy = ((y as f32 * scale) as u32).min(h - 1) as usize;
        for x in 0..tw {
            let sx = ((x as f32 * scale) as u32).min(w - 1) as usize;
            let p = &buffer[(sy * w as usize + sx) * channels..][..channels];
            out.extend_from_slice(&match channels {
                4 => [p[0], p[1], p[2], p[3]],
                3 => [p[0], p[1], p[2], 255],
                2 => [p[0], p[0], p[0], p[1]],
                _ => [p[0], p[0], p[0], 255],
            });
        }
    }
    Some((tw, th, out))
}

fn image_of(bytes: &[u8], limit: u32) -> Option<Image> {
    let (w, h, pixels) = decode_png(bytes, limit)?;
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(w, h);
    buffer.make_mut_bytes().copy_from_slice(&pixels);
    Some(Image::from_rgba8(buffer))
}

/// The prompt for a Project or OS build: agents there are terminal programs that read files with
/// their own tools, so attachments travel as paths in the brief.
pub(crate) fn brief_with_files(prompt: &str, files: &[PathBuf]) -> String {
    let prompt = prompt.trim();
    if files.is_empty() {
        return prompt.to_string();
    }
    let words = if prompt.is_empty() { "Please look at the attached file(s)." } else { prompt };
    let list: Vec<String> = files.iter().map(|f| format!("- {}", f.display())).collect();
    let folders = files.iter().any(|f| f.file_name().is_some_and(|n| n.to_string_lossy().starts_with("folder-")));
    let hint = if folders { "\n\nA folder-*.md attachment summarizes a folder; follow the angi-folder-analysis skill." } else { "" };
    format!(
        "{words}\n\nAttached files (read them with your file tools; open images to see them):\n{}{hint}",
        list.join("\n")
    )
}

pub(crate) fn size_text(bytes: u64) -> String {
    match bytes {
        b if b >= 1024 * 1024 => format!("{:.1} MB", b as f64 / 1_048_576.0),
        b if b >= 1024 => format!("{} KB", b / 1024),
        b => format!("{b} B"),
    }
}

fn is_picture_name(path: &Path) -> bool {
    path.extension().is_some_and(|e| matches!(e.to_string_lossy().to_lowercase().as_str(), "jpg" | "jpeg" | "gif" | "webp" | "bmp"))
}

/// An image to show: PNGs are decoded here (and scaled to `PREVIEW`); other picture formats are
/// handed to Slint, which shows them only where its decoders are built in.
fn picture(path: &Path) -> Option<Image> {
    let bytes = fs::read(path).ok()?;
    if has_png_signature(&bytes) {
        image_of(&bytes, PREVIEW)
    } else if is_picture_name(path) {
        Image::load_from_path(path).ok()
    } else {
        None
    }
}

fn tile(item: &Staged, pictures: &mut HashMap<PathBuf, Option<Image>>) -> ChatAttachment {
    let image = pictures.entry(item.path.clone()).or_insert_with(|| if item.folder { None } else { picture(&item.path) }).clone();
    ChatAttachment {
        name: item.name.as_str().into(),
        detail: if item.folder { "Folder summary".to_string() } else { size_text(item.size) }.into(),
        has_picture: image.is_some(),
        picture: image.unwrap_or_default(),
        is_folder: item.folder,
    }
}

/// The composer's collection. Index order matches the Slint model.
#[derive(Default)]
struct Collection {
    items: Vec<Staged>,
    pictures: HashMap<PathBuf, Option<Image>>,
    /// Set while the picker is open to replace one attachment instead of adding.
    replacing: Option<usize>,
}

fn publish(ui: &AppWindow, collection: &mut Collection, notice: &str) {
    let chat = ui.global::<AgentChat>();
    let Collection { items, pictures, .. } = &mut *collection;
    pictures.retain(|path, _| items.iter().any(|i| &i.path == path));
    let tiles: Vec<ChatAttachment> = items.iter().map(|i| tile(i, pictures)).collect();
    chat.set_attachments(ModelRc::new(VecModel::from(tiles)));
    chat.set_attach_notice(notice.into());
    if chat.get_viewing() >= collection.items.len() as i32 {
        chat.set_viewing(-1);
    }
}

/// Remove files left behind by sessions that ended long ago.
fn purge_old(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let old = entry.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > IDLE_PURGE);
        if old && entry.file_type().is_ok_and(|t| t.is_file()) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn clipboard_image() -> Option<Vec<u8>> {
    let types = Command::new("wl-paste").arg("--list-types").stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    if !String::from_utf8_lossy(&types.stdout).lines().any(|t| t.trim() == "image/png") {
        return None;
    }
    let out = Command::new("wl-paste").args(["--no-newline", "--type", "image/png"]).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    (out.status.success() && !out.stdout.is_empty()).then_some(out.stdout)
}

pub(crate) struct Attachments {
    /// Held only to keep the paste poller alive.
    _timer: Timer,
    collection: Rc<RefCell<Collection>>,
}

impl Attachments {
    /// What the composer hands to the runtime when it sends: the staged files and their tiles.
    pub fn take_for_send(&self, ui: &AppWindow) -> (Vec<PathBuf>, Vec<ChatAttachment>) {
        let mut collection = self.collection.borrow_mut();
        let items = std::mem::take(&mut collection.items);
        let tiles = ui.global::<AgentChat>().get_attachments().iter().collect();
        publish(ui, &mut collection, "");
        ui.global::<AgentChat>().set_viewing(-1);
        (items.into_iter().map(|i| i.path).collect(), tiles)
    }
}

pub(crate) fn install(ui: &AppWindow) -> Attachments {
    let dir = staging_dir();
    purge_old(&dir);
    let collection = Rc::new(RefCell::new(Collection::default()));
    let chat = ui.global::<AgentChat>();
    chat.set_attachments(ModelRc::new(VecModel::from(Vec::<ChatAttachment>::new())));

    // Add: open the Files picker in file mode.
    let weak = ui.as_weak();
    let shared = collection.clone();
    chat.on_attach(move || {
        let Some(ui) = weak.upgrade() else { return };
        if shared.borrow().items.len() >= MAX_ATTACHMENTS {
            ui.global::<AgentChat>().set_attach_notice(format!("Up to {MAX_ATTACHMENTS} attachments per message.").into());
            return;
        }
        shared.borrow_mut().replacing = None;
        ui.global::<crate::Files>().invoke_open_picker_files("".into());
    });

    // Update: pick a different file for one slot.
    let (weak, shared) = (ui.as_weak(), collection.clone());
    chat.on_replace_attachment(move |index| {
        let Some(ui) = weak.upgrade() else { return };
        if (index as usize) < shared.borrow().items.len() {
            shared.borrow_mut().replacing = Some(index as usize);
            ui.global::<AgentChat>().set_viewing(-1);
            ui.global::<crate::Files>().invoke_open_picker_files("".into());
        }
    });

    // Create, from the picker: a newline-separated list of chosen files.
    let (weak, shared, dir_chosen) = (ui.as_weak(), collection.clone(), dir.clone());
    chat.on_files_chosen(move |paths| {
        let Some(ui) = weak.upgrade() else { return };
        let mut collection = shared.borrow_mut();
        let mut notice = String::new();
        let replacing = collection.replacing.take();
        for (n, path) in paths.lines().filter(|l| !l.is_empty()).enumerate() {
            match (replacing, stage_path(Path::new(path), &dir_chosen)) {
                (Some(index), Ok(new)) if n == 0 && index < collection.items.len() => {
                    let old = std::mem::replace(&mut collection.items[index], new);
                    let _ = fs::remove_file(old.path);
                }
                (_, Ok(new)) if collection.items.len() < MAX_ATTACHMENTS => collection.items.push(new),
                (_, Ok(new)) => {
                    let _ = fs::remove_file(new.path);
                    notice = format!("Up to {MAX_ATTACHMENTS} attachments per message.");
                }
                (_, Err(reason)) => notice = reason,
            }
        }
        publish(&ui, &mut collection, &notice);
    });

    // Delete.
    let (weak, shared) = (ui.as_weak(), collection.clone());
    chat.on_remove_attachment(move |index| {
        let Some(ui) = weak.upgrade() else { return };
        let mut collection = shared.borrow_mut();
        if (index as usize) < collection.items.len() {
            let gone = collection.items.remove(index as usize);
            let _ = fs::remove_file(gone.path);
        }
        publish(&ui, &mut collection, "");
    });

    // Create, from Ctrl+V: a clipboard image (wl-paste, when present), else files copied in ANGI Files.
    let (found, pasted) = mpsc::channel::<Option<Vec<u8>>>();
    let weak = ui.as_weak();
    chat.on_paste_attachment(move || {
        let Some(ui) = weak.upgrade() else { return };
        let copied: Vec<PathBuf> = COPIED.lock().map(|c| c.clone()).unwrap_or_default();
        if !copied.is_empty() {
            let list: Vec<String> = copied.iter().filter(|p| p.exists()).map(|p| p.to_string_lossy().into_owned()).collect();
            if !list.is_empty() {
                // Use the same path as the picker so limits and errors stay in one place.
                ui.global::<AgentChat>().invoke_files_chosen(SharedString::from(list.join("\n")));
                if let Ok(mut c) = COPIED.lock() { c.clear(); }
                return;
            }
        }
        let found = found.clone();
        std::thread::spawn(move || { let _ = found.send(clipboard_image()); });
    });
    let (weak, shared) = (ui.as_weak(), collection.clone());
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(150), move || {
        let Some(ui) = weak.upgrade() else { return };
        while let Ok(Some(bytes)) = pasted.try_recv() {
            let mut collection = shared.borrow_mut();
            let notice = if collection.items.len() >= MAX_ATTACHMENTS {
                format!("Up to {MAX_ATTACHMENTS} attachments per message.")
            } else {
                match stage_bytes(&bytes, &dir) {
                    Ok(item) => { collection.items.push(item); String::new() }
                    Err(reason) => reason,
                }
            };
            publish(&ui, &mut collection, &notice);
        }
    });
    Attachments { _timer: timer, collection }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("angi-attach-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn tiny_png() -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 9, 9, 9, 0]).unwrap();
        bytes
    }

    #[test]
    fn staging_copies_the_file_and_never_overwrites_a_name() {
        let (src, dir) = (scratch("src"), scratch("dir"));
        fs::write(src.join("notes.txt"), "hello").unwrap();
        let first = stage_file(&src.join("notes.txt"), &dir).unwrap();
        let second = stage_file(&src.join("notes.txt"), &dir).unwrap();
        assert_eq!(first.name, "notes.txt");
        assert_eq!(second.name, "notes (1).txt");
        fs::write(src.join("notes.txt"), "changed").unwrap();
        assert_eq!(fs::read_to_string(&first.path).unwrap(), "hello");
    }

    #[test]
    fn folders_missing_files_and_oversized_files_are_refused() {
        let (src, dir) = (scratch("refuse"), scratch("refuse-dir"));
        assert!(stage_file(&src, &dir).unwrap_err().contains("not a file"));
        assert!(stage_file(&src.join("gone"), &dir).unwrap_err().contains("cannot be read"));
        let big = src.join("big.bin");
        fs::File::create(&big).unwrap().set_len(MAX_BYTES + 1).unwrap();
        assert!(stage_file(&big, &dir).unwrap_err().contains("20 MB"));
    }

    #[test]
    fn pasted_bytes_must_be_a_png_and_get_a_unique_name() {
        let dir = scratch("paste");
        assert!(stage_bytes(b"not an image", &dir).is_err());
        let a = stage_bytes(&tiny_png(), &dir).unwrap();
        let b = stage_bytes(&tiny_png(), &dir).unwrap();
        assert_eq!((a.name.as_str(), b.name.as_str()), ("pasted-image.png", "pasted-image (1).png"));
    }

    #[test]
    fn a_build_brief_lists_attachments_and_survives_an_empty_prompt() {
        let files = [PathBuf::from("/s/a.png"), PathBuf::from("/s/b.html")];
        assert_eq!(brief_with_files(" fix it ", &[]), "fix it");
        let brief = brief_with_files("fix it", &files);
        assert!(brief.starts_with("fix it\n\nAttached files"));
        assert!(brief.ends_with("- /s/a.png\n- /s/b.html"));
        assert!(brief_with_files("", &files).starts_with("Please look at the attached"));
        assert!(!brief.contains("angi-folder-analysis"));
        let folder = brief_with_files("what is this", &[PathBuf::from("/s/folder-demo.md")]);
        assert!(folder.contains("angi-folder-analysis"));
    }

    #[test]
    fn names_cannot_escape_the_staging_folder() {
        assert_eq!(clean_name("../../etc/passwd"), "_.._etc_passwd");
        assert!(!clean_name("../x").contains('/'));
        assert_eq!(clean_name("...."), "attachment");
    }

    #[test]
    fn png_decoding_scales_down_and_keeps_alpha() {
        let (w, h, px) = decode_png(&tiny_png(), 1).unwrap();
        assert_eq!((w, h, px.len()), (1, 1, 4));
        let (w, h, px) = decode_png(&tiny_png(), 64).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(&px[12..16], &[9, 9, 9, 0]);
        assert!(decode_png(b"junk", 64).is_none());
    }
}
