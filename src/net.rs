use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};

/// Serves `file:///…` URLs from `root`, so root-relative and relative hrefs in a
/// built site resolve against the site directory. Anything else resolves to an
/// empty body, because a fetch that never completes blocks the document's
/// render forever. Only failed stylesheets are reported: images and fonts do
/// not change computed values.
pub struct FileNetProvider {
    root: PathBuf,
    base: String,
    warnings: Arc<Mutex<Vec<String>>>,
}

impl FileNetProvider {
    /// `base` is the URL path prefix the site is served under (`/docs`), which
    /// is stripped before mapping a URL onto `root`.
    pub fn new(root: PathBuf, base: &str) -> (Self, Arc<Mutex<Vec<String>>>) {
        let warnings = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                root,
                base: base.trim_end_matches('/').to_string(),
                warnings: warnings.clone(),
            },
            warnings,
        )
    }

    fn read(&self, request: &Request) -> Result<Vec<u8>, String> {
        let url = &request.url;
        if url.scheme() != "file" {
            return Err(format!("not fetched (only local files are loaded): {url}"));
        }
        let path = url
            .to_file_path()
            .map_err(|_| format!("invalid file url: {url}"))?;
        let path = path.to_string_lossy();
        let path = path
            .strip_prefix(self.base.as_str())
            .filter(|rest| rest.starts_with('/'))
            .unwrap_or(&path);
        let full = self.root.join(path.trim_start_matches('/'));
        std::fs::read(&full).map_err(|e| format!("cannot read {}: {e}", full.display()))
    }
}

impl NetProvider for FileNetProvider {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let body = match self.read(&request) {
            Ok(bytes) => bytes,
            Err(warning) => {
                if looks_like_stylesheet(&request.url) {
                    self.warnings.lock().unwrap().push(warning);
                }
                Vec::new()
            }
        };
        handler.bytes(request.url.to_string(), Bytes::from(body));
    }
}

/// Fetches `http://` URLs. A failed stylesheet is reported and loads empty,
/// like a missing local file.
pub struct HttpNetProvider {
    agent: ureq::Agent,
    warnings: Arc<Mutex<Vec<String>>>,
}

impl HttpNetProvider {
    pub fn new(agent: ureq::Agent) -> (Self, Arc<Mutex<Vec<String>>>) {
        let warnings = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                agent,
                warnings: warnings.clone(),
            },
            warnings,
        )
    }
}

impl NetProvider for HttpNetProvider {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let url = request.url.to_string();
        let body = match get(&self.agent, &url) {
            Ok((200..=299, body, _)) => body,
            Ok((status, _, _)) => {
                if looks_like_stylesheet(&request.url) {
                    self.warnings
                        .lock()
                        .unwrap()
                        .push(format!("HTTP {status}: {url}"));
                }
                Vec::new()
            }
            Err(e) => {
                if looks_like_stylesheet(&request.url) {
                    self.warnings.lock().unwrap().push(e);
                }
                Vec::new()
            }
        };
        handler.bytes(url, Bytes::from(body));
    }
}

/// `(status, body, final url after redirects)`
pub fn get(agent: &ureq::Agent, url: &str) -> Result<(u16, Vec<u8>, String), String> {
    use ureq::ResponseExt;
    let mut response = agent
        .get(url)
        .call()
        .map_err(|e| format!("GET {url}: {e}"))?;
    let status = response.status().as_u16();
    let final_url = response.get_uri().to_string();
    let body = response
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| format!("GET {url}: {e}"))?;
    Ok((status, body, final_url))
}

pub fn agent(timeout: std::time::Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .into()
}

fn looks_like_stylesheet(url: &blitz_traits::net::Url) -> bool {
    match url.scheme() {
        "data" => url.path().starts_with("text/css"),
        _ => url.path().ends_with(".css") || url.path().contains("/css"),
    }
}
