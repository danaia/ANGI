//! `open TARGET`: show a file or web address in the ANGI app that handles it.
//!
//! The desktop listens on a private Unix socket in the signed-in account's own cache folder. The same
//! binary, run under the name `open` (a link to `agx-gui`) or as `agx-gui open`, is the client: it turns
//! each argument into an absolute path or web address, asks the desktop, and prints the desktop's answer.
//! The desktop decides, never the client: it resolves the path itself and only the account that owns the
//! desktop may ask. HTML files and `http(s)` addresses open in Web; anything else is refused with a reason.
use crate::{App, AppWindow, keymap::Cmd, web_preview, wm::Kind};
use slint::{ComponentHandle, Timer, TimerMode};
use std::{
    cell::RefCell,
    collections::VecDeque,
    ffi::{OsStr, OsString},
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt},
        io::AsRawFd,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process::ExitCode,
    rc::Rc,
    sync::mpsc,
    time::Duration,
};

const MAX_REQUEST: u64 = 8192;
const MAX_URI: usize = 8000;
const SOCKET: &str = "angi-open.sock";

/// Where this account's desktop listens: the runtime directory if the system has one, else `~/.cache/angi`.
pub fn socket_path() -> Option<PathBuf> {
    let directory = match std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
        Some(runtime) => PathBuf::from(runtime),
        None => {
            PathBuf::from(std::env::var_os("HOME").filter(|h| !h.is_empty())?).join(".cache/angi")
        }
    };
    Some(directory.join(SOCKET))
}

/// `file://` address of an absolute path, with every byte outside the unreserved set percent-encoded.
pub fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(byte as char)
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

/// The address Web should load for `target`, or why it cannot be opened. `target` is an absolute path or an
/// `http(s)://` address. Paths are resolved here, so a link cannot dress up one kind of file as another.
pub fn resolve(target: &str) -> Result<String, String> {
    if target.is_empty() || target.chars().any(char::is_control) {
        return Err("That is not something that can be opened.".into());
    }
    if target.starts_with("http://") || target.starts_with("https://") {
        return crate::browser::web_address(target)
            .ok_or_else(|| "That is not a usable web address.".into());
    }
    let path = Path::new(target);
    if !path.is_absolute() {
        return Err("Give the full path of the file.".into());
    }
    let real = fs::canonicalize(path).map_err(|_| format!("{target}: no such file"))?;
    let metadata = fs::metadata(&real).map_err(|error| format!("{target}: {error}"))?;
    if metadata.is_dir() {
        return Err(format!(
            "{target} is a folder; folders cannot be opened from the terminal yet."
        ));
    }
    if !metadata.is_file() {
        return Err(format!("{target} is not a regular file."));
    }
    fs::File::open(&real).map_err(|error| format!("{target}: {error}"))?;
    let extension = real
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("html" | "htm" | "xhtml") => {
            let uri = file_uri(&real);
            (uri.len() <= MAX_URI)
                .then_some(uri)
                .ok_or_else(|| "That path is too long to open.".into())
        }
        Some(other) => Err(format!(
            "ANGI has no app for .{other} files yet. HTML files and web addresses open in Web."
        )),
        None => Err(
            "ANGI cannot tell what kind of file that is. HTML files and web addresses open in Web."
                .into(),
        ),
    }
}

fn effective_uid() -> u32 {
    unsafe { libc::geteuid() }
}

fn peer_uid(stream: &UnixStream) -> Option<u32> {
    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let status = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    (status == 0).then_some(credentials.uid)
}

/// Create the private folder and bind the socket, replacing a stale socket this account left behind.
fn bind(path: &Path) -> io::Result<UnixListener> {
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("no folder for the socket"))?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)?;
    let folder = fs::symlink_metadata(directory)?;
    if !folder.is_dir() || folder.uid() != effective_uid() {
        return Err(io::Error::other(
            "the socket folder is not a private folder of this account",
        ));
    }
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    match fs::symlink_metadata(path) {
        Ok(existing) if existing.file_type().is_socket() && existing.uid() == effective_uid() => {
            fs::remove_file(path)?
        }
        Ok(_) => {
            return Err(io::Error::other(
                "something that is not this desktop's socket is in the way",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let listener = UnixListener::bind(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

enum Request {
    Open(String),
    Preview(web_preview::Target),
}

fn handle(stream: &mut UnixStream, found: &mpsc::Sender<Request>) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    if peer_uid(stream) != Some(effective_uid()) {
        return writeln!(
            stream,
            "error This desktop only takes requests from its own account."
        );
    }
    let mut line = String::new();
    BufReader::new((&*stream).take(MAX_REQUEST)).read_line(&mut line)?;
    let line = line.trim_end_matches(['\r', '\n']);
    let request = if let Some(target) = line.strip_prefix("preview ") {
        Some(web_preview::resolve(target).map(Request::Preview))
    } else {
        line.strip_prefix("open ")
            .map(|target| resolve(target).map(Request::Open))
    };
    let reply = match request {
        Some(Ok(uri)) => match found.send(uri) {
            Ok(()) => "ok".to_string(),
            Err(_) => "error The desktop is shutting down.".to_string(),
        },
        Some(Err(message)) => format!("error {message}"),
        None => "error Unknown request.".to_string(),
    };
    writeln!(stream, "{reply}")
}

/// Start listening; every accepted request arrives on the returned channel as the address to load.
fn listen(path: &Path) -> io::Result<mpsc::Receiver<Request>> {
    let listener = bind(path)?;
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let _ = handle(&mut stream, &send);
        }
    });
    Ok(receive)
}

/// Wire requests into the desktop: bring Web forward and load what was asked for.
pub(super) fn install(ui: &AppWindow, app: &Rc<RefCell<App>>) -> Timer {
    let receiver = socket_path()
        .ok_or_else(|| io::Error::other("no HOME"))
        .and_then(|path| listen(&path));
    let receiver = receiver
        .map_err(|error| eprintln!("agx-gui: `open` is unavailable: {error}"))
        .ok();
    let (weak, state) = (ui.as_weak(), app.clone());
    let previews = web_preview::Previews::new();
    let mut pending: VecDeque<Request> = VecDeque::new();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(150), move || {
        let Some(receiver) = &receiver else { return };
        pending.extend(receiver.try_iter().take(8));
        while !pending.is_empty() {
            let (Some(ui), Ok(mut app)) = (weak.upgrade(), state.try_borrow_mut()) else {
                return;
            };
            match pending.pop_front().unwrap() {
                Request::Open(uri) => {
                    app.run(&ui, Cmd::Open(Kind::Web));
                    app.browser_open(&ui, &uri);
                }
                Request::Preview(target) => previews.start(target),
            }
        }
        if let (Some(ui), Ok(mut app)) = (weak.upgrade(), state.try_borrow_mut()) {
            for reply in previews.replies.try_iter().take(8) {
                match reply {
                    Ok(uri) => web_preview::show(&mut app, &ui, &uri),
                    Err(reason) => app.toast(&ui, &reason),
                }
            }
        }
    });
    timer
}

/// Arguments for the client when this process was started as `open ...` or `agx-gui open ...`.
pub fn client_arguments() -> Option<Vec<OsString>> {
    client_arguments_from(std::env::args_os())
}

fn client_arguments_from(mut arguments: impl Iterator<Item = OsString>) -> Option<Vec<OsString>> {
    let program = arguments.next()?;
    let rest: Vec<OsString> = arguments.collect();
    if Path::new(&program).file_name() == Some(OsStr::new("open")) {
        return Some(rest);
    }
    (rest.first().map(OsString::as_os_str) == Some(OsStr::new("open"))).then(|| rest[1..].to_vec())
}

fn ask(socket: &Path, directory: &Path, target: &OsStr, preview: bool) -> Result<(), String> {
    let text = target.to_str().ok_or("File names must be valid UTF-8.")?;
    let web = text.starts_with("http://") || text.starts_with("https://");
    // `open localhost:8000` is a server on this device, unless a file really has that name.
    let local =
        crate::browser::local_server_address(text).filter(|_| !directory.join(text).exists());
    let absolute = if let Some(address) = local {
        address
    } else if web {
        text.to_string()
    } else {
        directory.join(text).to_string_lossy().into_owned()
    };
    if absolute.chars().any(char::is_control) {
        return Err("That is not something that can be opened.".into());
    }
    let mut stream = UnixStream::connect(socket)
        .map_err(|_| "The ANGI desktop is not running for this account.".to_string())?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let action = if preview { "preview" } else { "open" };
    writeln!(stream, "{action} {absolute}").map_err(|error| error.to_string())?;
    let mut reply = String::new();
    BufReader::new(stream)
        .read_line(&mut reply)
        .map_err(|error| format!("The desktop did not answer: {error}"))?;
    match reply.trim_end().split_once(' ') {
        None if reply.trim_end() == "ok" => Ok(()),
        Some(("error", message)) => Err(message.to_string()),
        _ => Err("The desktop gave an unexpected answer.".into()),
    }
}

/// The `open` command: exit status 0 when every target was handed to the desktop.
pub fn client(targets: Vec<OsString>) -> ExitCode {
    if targets.is_empty() || targets[0] == "--help" || targets[0] == "-h" {
        eprintln!(
            "Usage: open [--preview] FILE_OR_ADDRESS...\nOpens HTML files and http(s) addresses in ANGI Web on this account's desktop."
        );
        return ExitCode::from(2);
    }
    let Some(socket) = socket_path() else {
        eprintln!("open: cannot tell where this account's desktop listens (HOME is not set).");
        return ExitCode::FAILURE;
    };
    let directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let preview = targets[0] == "--preview";
    let targets = &targets[usize::from(preview)..];
    if targets.is_empty() {
        eprintln!("open: give an HTML file or loopback address to preview.");
        return ExitCode::from(2);
    }
    let mut failed = false;
    for target in targets {
        if let Err(message) = ask(&socket, &directory, target, preview) {
            eprintln!("open: {message}");
            failed = true;
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("angi-open-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        fs::canonicalize(directory).unwrap()
    }

    #[test]
    fn paths_are_percent_encoded_byte_for_byte() {
        assert_eq!(
            file_uri(Path::new("/home/dana/my game/index.html")),
            "file:///home/dana/my%20game/index.html"
        );
        assert_eq!(
            file_uri(Path::new("/a/b#c?d%e/é.html")),
            "file:///a/b%23c%3Fd%25e/%C3%A9.html"
        );
    }

    #[test]
    fn html_files_and_web_addresses_open_in_web() {
        let directory = scratch("html");
        for name in ["page.html", "page.HTM", "old.xhtml"] {
            fs::write(directory.join(name), "<p>hi").unwrap();
            let uri = resolve(directory.join(name).to_str().unwrap()).unwrap();
            assert!(uri.starts_with("file:///") && uri.ends_with(name), "{uri}");
        }
        assert_eq!(
            resolve("https://example.org/a").unwrap(),
            "https://example.org/a"
        );
        assert!(
            resolve("example.org").is_err(),
            "a bare word must not be guessed to be a website"
        );
    }

    #[test]
    fn other_files_folders_and_missing_paths_are_refused_with_a_reason() {
        let directory = scratch("refused");
        fs::write(directory.join("notes.txt"), "x").unwrap();
        fs::write(directory.join("noext"), "x").unwrap();
        assert!(
            resolve(directory.join("notes.txt").to_str().unwrap())
                .unwrap_err()
                .contains(".txt")
        );
        assert!(
            resolve(directory.join("noext").to_str().unwrap())
                .unwrap_err()
                .contains("what kind")
        );
        assert!(
            resolve(directory.to_str().unwrap())
                .unwrap_err()
                .contains("folder")
        );
        assert!(
            resolve(directory.join("gone.html").to_str().unwrap())
                .unwrap_err()
                .contains("no such file")
        );
        assert!(resolve("relative.html").unwrap_err().contains("full path"));
        assert!(resolve("javascript:alert(1)").is_err());
        assert!(resolve("file:///etc/passwd").is_err());
        assert!(resolve("/tmp/a\nb.html").is_err());
    }

    #[test]
    fn a_link_cannot_make_another_kind_of_file_look_like_html() {
        let directory = scratch("link");
        fs::write(directory.join("secret.txt"), "x").unwrap();
        std::os::unix::fs::symlink(
            directory.join("secret.txt"),
            directory.join("innocent.html"),
        )
        .unwrap();
        assert!(
            resolve(directory.join("innocent.html").to_str().unwrap())
                .unwrap_err()
                .contains(".txt")
        );
    }

    #[test]
    fn the_desktop_answers_the_open_command_over_its_private_socket() {
        let directory = scratch("socket");
        fs::write(directory.join("game.html"), "<p>hi").unwrap();
        let socket = directory.join("run/angi-open.sock");
        let receiver = listen(&socket).unwrap();
        assert_eq!(
            fs::metadata(&socket).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(socket.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        // A relative name is made absolute by the client, from its own working directory.
        ask(&socket, &directory, OsStr::new("game.html"), false).unwrap();
        let uri = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(uri, Request::Open(uri) if uri == file_uri(&directory.join("game.html"))));
        let refused = ask(&socket, &directory, OsStr::new("missing.html"), false).unwrap_err();
        assert!(refused.contains("no such file"), "{refused}");
        assert!(
            receiver.try_recv().is_err(),
            "a refused request must not reach the desktop"
        );
        assert!(
            ask(
                &directory.join("nobody.sock"),
                &directory,
                OsStr::new("game.html"),
                false,
            )
            .unwrap_err()
            .contains("not running")
        );
    }

    #[test]
    fn servers_on_this_device_open_over_plain_http() {
        let directory = scratch("local");
        let socket = directory.join("angi-open.sock");
        let receiver = listen(&socket).unwrap();
        for (typed, loaded) in [
            ("localhost:8000", "http://localhost:8000"),
            ("127.0.0.1:5000/app", "http://127.0.0.1:5000/app"),
            (
                "http://localhost:8000/a.html",
                "http://localhost:8000/a.html",
            ),
        ] {
            ask(&socket, &directory, OsStr::new(typed), false).unwrap();
            assert!(
                matches!(receiver.recv_timeout(Duration::from_secs(2)).unwrap(), Request::Open(uri) if uri == loaded)
            );
        }
        // Other hosts are not guessed to be websites, and a file that really has that name wins.
        assert!(ask(&socket, &directory, OsStr::new("example.org"), false).is_err());
        fs::write(directory.join("localhost:9000"), "x").unwrap();
        let refused = ask(&socket, &directory, OsStr::new("localhost:9000"), false).unwrap_err();
        assert!(
            refused.contains("no app") || refused.contains("what kind"),
            "{refused}"
        );
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn preview_requests_are_distinct_and_loopback_only() {
        let directory = scratch("preview");
        let socket = directory.join("angi-open.sock");
        let receiver = listen(&socket).unwrap();
        ask(&socket, &directory, OsStr::new("localhost:8000/app"), true).unwrap();
        let request = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            matches!(request, Request::Preview(web_preview::Target::Address(uri)) if uri == "http://localhost:8000/app")
        );
        assert!(ask(&socket, &directory, OsStr::new("https://example.org"), true).is_err());
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn a_stale_socket_is_replaced_but_a_stranger_file_is_not() {
        let directory = scratch("stale");
        let socket = directory.join("angi-open.sock");
        drop(UnixListener::bind(&socket).unwrap());
        assert!(
            listen(&socket).is_ok(),
            "a socket left by a crashed desktop is replaced"
        );
        let other = directory.join("other/angi-open.sock");
        fs::create_dir_all(other.parent().unwrap()).unwrap();
        fs::write(&other, "not a socket").unwrap();
        assert!(listen(&other).is_err(), "an ordinary file is never deleted");
        assert_eq!(fs::read_to_string(&other).unwrap(), "not a socket");
    }

    #[test]
    fn the_client_is_recognised_by_its_name_or_its_first_argument() {
        let args = |list: &[&str]| client_arguments_from(list.iter().map(OsString::from));
        assert_eq!(
            args(&["/usr/bin/open", "a.html", "b.html"]),
            Some(vec!["a.html".into(), "b.html".into()])
        );
        assert_eq!(args(&["open"]), Some(vec![]));
        assert_eq!(
            args(&["/usr/bin/agx-gui", "open", "a.html"]),
            Some(vec!["a.html".into()])
        );
        assert_eq!(
            args(&["/usr/bin/agx-gui"]),
            None,
            "the desktop itself must still start normally"
        );
        assert_eq!(args(&["/usr/bin/agx-gui", "--login"]), None);
        assert_eq!(args(&["/usr/bin/agx-gui", "a.html"]), None);
    }
}
