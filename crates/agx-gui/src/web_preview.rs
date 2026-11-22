//! Resolve agent preview requests and own the local HTTP processes for the desktop session.
use crate::{App, AppWindow, Web, files::server::Server, open_request::file_uri};
use slint::ComponentHandle;
use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
};

#[derive(Debug)]
pub(crate) enum Target {
    Address(String),
    Html { root: PathBuf, page: PathBuf },
}

pub(crate) fn is_html(path: &Path) -> bool {
    path.extension()
        .and_then(|suffix| suffix.to_str())
        .is_some_and(|suffix| {
            ["html", "htm", "xhtml"]
                .iter()
                .any(|html| suffix.eq_ignore_ascii_case(html))
        })
}

pub(crate) fn resolve(target: &str) -> Result<Target, String> {
    if target.chars().any(char::is_control) {
        return Err("Enter a local web address or HTML file.".into());
    }
    if let Some(address) = crate::browser::loopback_preview_address(target) {
        return Ok(Target::Address(address));
    }
    let path = Path::new(target);
    if !path.is_absolute() {
        return Err("Give the full path of an HTML file or a loopback address.".into());
    }
    let page = fs::canonicalize(path).map_err(|_| "The preview file no longer exists.")?;
    if path != page || !is_html(&page) || !page.is_file() || fs::File::open(&page).is_err() {
        return Err("Choose a readable HTML file.".into());
    }
    let root = page
        .parent()
        .ok_or("The preview has no project folder.")?
        .to_owned();
    // A loose HTML file in Home can be shown directly without serving the home directory.
    let home = std::env::var_os("HOME").and_then(|path| fs::canonicalize(path).ok());
    if home.as_ref().is_some_and(|home| home.starts_with(&root)) || root == Path::new("/") {
        return Ok(Target::Address(file_uri(&page)));
    }
    Ok(Target::Html { root, page })
}

pub(crate) struct Previews {
    requests: Option<mpsc::Sender<Target>>,
    pub replies: mpsc::Receiver<Result<String, String>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Previews {
    pub(crate) fn new() -> Self {
        let (requests, inbox) = mpsc::channel();
        let (results, replies) = mpsc::channel();
        let worker = thread::spawn(move || {
            let mut servers = VecDeque::<(PathBuf, String, Server)>::new();
            for target in inbox {
                let result = match target {
                    Target::Address(uri) => Ok(uri),
                    Target::Html { root, page } => serve(&mut servers, root, &page),
                };
                if results.send(result).is_err() {
                    break;
                }
            }
        });
        Self {
            requests: Some(requests),
            replies,
            worker: Some(worker),
        }
    }

    pub(crate) fn start(&self, target: Target) {
        let _ = self.requests.as_ref().unwrap().send(target);
    }
}

impl Drop for Previews {
    fn drop(&mut self) {
        self.requests.take();
        if let Some(worker) = self.worker.take() {
            crate::session_refresh::spawn_cleanup(move || {
                let _ = worker.join();
            });
        }
    }
}

fn serve(
    servers: &mut VecDeque<(PathBuf, String, Server)>,
    root: PathBuf,
    page: &Path,
) -> Result<String, String> {
    servers.retain_mut(|(_, _, server)| server.running());
    let (token, server) =
        if let Some(index) = servers.iter().position(|(folder, _, _)| folder == &root) {
            let entry = servers.remove(index).unwrap();
            servers.push_back(entry);
            let (_, token, server) = servers.back().unwrap();
            (token, server)
        } else {
            let token = fs::read_to_string("/proc/sys/kernel/random/uuid")
                .map_err(|_| "Could not secure the HTML preview.")?
                .trim()
                .to_string();
            let mut command = Command::new("/usr/bin/python3");
            command
                .args(["-I", "-u", "/usr/libexec/agx-agent/preview_http.py"])
                .arg(&root)
                .env("AGX_PREVIEW_TOKEN", &token);
            let server = Server::launch(command)
                .map_err(|error| format!("Could not start the HTML preview: {error}"))?;
            if servers.len() >= 8 {
                servers.pop_front();
            }
            servers.push_back((root.clone(), token, server));
            let (_, token, server) = servers.back().unwrap();
            (token, server)
        };
    let file = file_uri(
        Path::new("/")
            .join(page.file_name().ok_or("The preview has no file name.")?)
            .as_path(),
    );
    Ok(format!(
        "{}{}?angi_preview={token}",
        server.url,
        file.trim_start_matches("file:///")
    ))
}

pub(crate) fn show(app: &mut App, ui: &AppWindow, uri: &str) {
    let web = ui.global::<Web>();
    let address = web.get_address();
    let clean = uri
        .split_once("?angi_preview=")
        .map_or(uri, |(plain, _)| plain);
    let new_tab = app.desk.is_open(crate::wm::Kind::Web)
        && address.as_str() != uri
        && address.as_str() != clean;
    app.desk.open_agent_preview();
    app.refresh(ui);
    if new_tab {
        app.browser_tab_command(ui, 0, 0);
    }
    app.browser_open(ui, uri);
}
