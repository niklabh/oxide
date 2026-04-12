//! Download manager for non-WASM resources.
//!
//! When the Oxide browser navigates to a URL that does not point to a `.wasm`
//! module it downloads the file to the system Downloads folder instead.
//! Multiple downloads can run in parallel, each reporting progress (bytes
//! received, total size, speed) via shared state polled by the UI.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Identifies a single download; monotonically increasing.
pub type DownloadId = u64;

/// Snapshot of a single download's progress at a point in time.
#[derive(Clone, Debug)]
pub struct DownloadProgress {
    pub id: DownloadId,
    pub url: String,
    pub filename: String,
    pub state: DownloadState,
    pub bytes_downloaded: u64,
    /// `None` when the server omits `Content-Length`.
    pub total_bytes: Option<u64>,
    pub speed_bytes_per_sec: f64,
    pub destination: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DownloadState {
    InProgress,
    Completed,
    Failed(String),
    Cancelled,
}

impl DownloadProgress {
    pub fn percent(&self) -> Option<f64> {
        self.total_bytes
            .map(|total| (self.bytes_downloaded as f64 / total as f64) * 100.0)
    }

    pub fn is_finished(&self) -> bool {
        !matches!(self.state, DownloadState::InProgress)
    }
}

/// Thread-safe handle shared between the download worker threads and the UI.
pub type SharedDownloads = Arc<Mutex<Vec<DownloadProgress>>>;

/// Manages parallel file downloads. Owns a shared list of [`DownloadProgress`]
/// entries that the UI polls each frame.
impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct DownloadManager {
    downloads: SharedDownloads,
    next_id: Arc<Mutex<u64>>,
}

impl DownloadManager {
    pub fn new() -> Self {
        Self {
            downloads: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(Mutex::new(1)),
        }
    }

    pub fn downloads(&self) -> SharedDownloads {
        self.downloads.clone()
    }

    /// Kick off a background download for `url`.  Returns immediately.
    /// The file is saved into the system Downloads directory.
    pub fn start_download(&self, url: String) {
        let dest_dir = dirs::download_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")));
        self.start_download_to(url, &dest_dir);
    }

    /// Kick off a background download for `url` into a specific directory.
    pub fn start_download_to(&self, url: String, dest_dir: &std::path::Path) {
        let id = {
            let mut next = self.next_id.lock().unwrap();
            let id = *next;
            *next += 1;
            id
        };

        let filename = filename_from_url(&url);
        let dest = unique_path(dest_dir, &filename);

        // Touch the file so a second concurrent download picks a different name.
        let _ = std::fs::File::create(&dest);

        let progress = DownloadProgress {
            id,
            url: url.clone(),
            filename: dest
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            state: DownloadState::InProgress,
            bytes_downloaded: 0,
            total_bytes: None,
            speed_bytes_per_sec: 0.0,
            destination: dest.clone(),
        };

        self.downloads.lock().unwrap().push(progress);

        let downloads = self.downloads.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(run_download(id, url, dest, downloads));
        });
    }

    /// Cancel an in-progress download.
    pub fn cancel(&self, id: DownloadId) {
        let mut list = self.downloads.lock().unwrap();
        if let Some(dl) = list.iter_mut().find(|d| d.id == id) {
            if dl.state == DownloadState::InProgress {
                dl.state = DownloadState::Cancelled;
            }
        }
    }

    /// Remove a finished (or cancelled/failed) download entry from the list.
    pub fn dismiss(&self, id: DownloadId) {
        let mut list = self.downloads.lock().unwrap();
        list.retain(|d| d.id != id);
    }

    pub fn has_active(&self) -> bool {
        self.downloads
            .lock()
            .unwrap()
            .iter()
            .any(|d| d.state == DownloadState::InProgress)
    }
}

async fn run_download(id: DownloadId, url: String, dest: PathBuf, downloads: SharedDownloads) {
    use tokio::io::AsyncWriteExt;

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            set_state(&downloads, id, DownloadState::Failed(e.to_string()));
            return;
        }
    };

    let response = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            set_state(&downloads, id, DownloadState::Failed(e.to_string()));
            return;
        }
    };

    if !response.status().is_success() {
        set_state(
            &downloads,
            id,
            DownloadState::Failed(format!("HTTP {}", response.status())),
        );
        return;
    }

    let total = response.content_length();
    {
        let mut list = downloads.lock().unwrap();
        if let Some(dl) = list.iter_mut().find(|d| d.id == id) {
            dl.total_bytes = total;
        }
    }

    let file = match tokio::fs::File::create(&dest).await {
        Ok(f) => f,
        Err(e) => {
            set_state(&downloads, id, DownloadState::Failed(e.to_string()));
            return;
        }
    };
    let mut writer = tokio::io::BufWriter::new(file);
    let mut stream = response.bytes_stream();
    let started = Instant::now();
    let mut downloaded: u64 = 0;

    use futures_util::StreamExt;
    while let Some(chunk_result) = stream.next().await {
        let cancelled = {
            let list = downloads.lock().unwrap();
            list.iter()
                .any(|d| d.id == id && d.state == DownloadState::Cancelled)
        };
        if cancelled {
            let _ = tokio::fs::remove_file(&dest).await;
            return;
        }

        let chunk: bytes::Bytes = match chunk_result {
            Ok(c) => c,
            Err(e) => {
                set_state(&downloads, id, DownloadState::Failed(e.to_string()));
                return;
            }
        };
        if let Err(e) = writer.write_all(&chunk).await {
            set_state(&downloads, id, DownloadState::Failed(e.to_string()));
            return;
        }
        downloaded += chunk.len() as u64;
        let elapsed = started.elapsed().as_secs_f64().max(0.001);
        let speed = downloaded as f64 / elapsed;

        {
            let mut list = downloads.lock().unwrap();
            if let Some(dl) = list.iter_mut().find(|d| d.id == id) {
                dl.bytes_downloaded = downloaded;
                dl.speed_bytes_per_sec = speed;
            }
        }
    }

    if let Err(e) = writer.flush().await {
        set_state(&downloads, id, DownloadState::Failed(e.to_string()));
        return;
    }

    set_state(&downloads, id, DownloadState::Completed);
}

fn set_state(downloads: &SharedDownloads, id: DownloadId, state: DownloadState) {
    let mut list = downloads.lock().unwrap();
    if let Some(dl) = list.iter_mut().find(|d| d.id == id) {
        dl.state = state;
    }
}

/// Extract a reasonable filename from a URL, falling back to `"download"`.
fn filename_from_url(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.path_segments()
                .and_then(|mut segs| segs.next_back().map(|s| s.to_string()))
                .filter(|s| !s.is_empty())
        })
        .map(|name| {
            percent_encoding::percent_decode_str(&name)
                .decode_utf8_lossy()
                .into_owned()
        })
        .unwrap_or_else(|| "download".to_string())
}

/// If `dir/name` exists, try `name (1)`, `name (2)`, etc.
fn unique_path(dir: &std::path::Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = std::path::Path::new(name)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let ext = std::path::Path::new(name)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for i in 1u32.. {
        let try_name = format!("{stem} ({i}){ext}");
        let p = dir.join(&try_name);
        if !p.exists() {
            return p;
        }
    }
    dir.join(name)
}

/// Format byte count for display: "1.2 MB", "340 KB", etc.
pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_extraction() {
        assert_eq!(
            filename_from_url("https://github.com/robots.txt"),
            "robots.txt"
        );
        assert_eq!(
            filename_from_url("https://example.com/path/to/file.zip"),
            "file.zip"
        );
        assert_eq!(filename_from_url("https://example.com/"), "download");
        assert_eq!(filename_from_url("https://example.com"), "download");
        assert_eq!(
            filename_from_url("https://example.com/hello%20world.pdf"),
            "hello world.pdf"
        );
    }

    #[test]
    fn unique_path_no_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let p = unique_path(dir.path(), "test.txt");
        assert_eq!(p, dir.path().join("test.txt"));
    }

    #[test]
    fn unique_path_with_conflict() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "existing").unwrap();
        let p = unique_path(dir.path(), "test.txt");
        assert_eq!(p, dir.path().join("test (1).txt"));
    }

    #[test]
    fn format_bytes_display() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1 KB");
        assert_eq!(format_bytes(1_500_000), "1.4 MB");
        assert_eq!(format_bytes(2_000_000_000), "1.9 GB");
    }

    #[test]
    fn download_github_robots_txt() {
        let dir = tempfile::tempdir().unwrap();
        let dm = DownloadManager::new();

        dm.start_download_to("https://github.com/robots.txt".to_string(), dir.path());

        // Poll until complete (up to 30 s).
        let deadline = Instant::now() + std::time::Duration::from_secs(30);
        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let list = dm.downloads().lock().unwrap().clone();
            assert_eq!(list.len(), 1);
            let dl = &list[0];
            if dl.is_finished() {
                assert_eq!(dl.state, DownloadState::Completed);
                assert!(dl.bytes_downloaded > 0, "should have downloaded some bytes");
                break;
            }
            assert!(
                Instant::now() < deadline,
                "download did not complete within 30 seconds"
            );
        }

        let saved = dir.path().join("robots.txt");
        assert!(saved.exists(), "robots.txt should exist on disk");
        let content = std::fs::read_to_string(&saved).unwrap();
        assert!(
            content.contains("User-agent"),
            "robots.txt should contain 'User-agent'"
        );
    }

    #[test]
    fn parallel_downloads() {
        let dir = tempfile::tempdir().unwrap();
        let dm = DownloadManager::new();

        dm.start_download_to("https://github.com/robots.txt".to_string(), dir.path());
        dm.start_download_to("https://github.com/robots.txt".to_string(), dir.path());

        assert_eq!(dm.downloads().lock().unwrap().len(), 2);

        let deadline = Instant::now() + std::time::Duration::from_secs(30);
        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let list = dm.downloads().lock().unwrap().clone();
            if list.iter().all(|d| d.is_finished()) {
                assert!(list.iter().all(|d| d.state == DownloadState::Completed));
                // Second download gets a deduplicated filename.
                let names: Vec<_> = list.iter().map(|d| d.filename.clone()).collect();
                assert!(names.contains(&"robots.txt".to_string()));
                assert!(names.contains(&"robots (1).txt".to_string()));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "parallel downloads did not complete within 30 seconds"
            );
        }
    }

    #[test]
    fn cancel_download() {
        let dm = DownloadManager::new();
        let dir = tempfile::tempdir().unwrap();

        dm.start_download_to("https://github.com/robots.txt".to_string(), dir.path());

        let id = dm.downloads().lock().unwrap()[0].id;
        dm.cancel(id);

        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let list = dm.downloads().lock().unwrap().clone();
            let dl = &list[0];
            if dl.is_finished() {
                assert_eq!(dl.state, DownloadState::Cancelled);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "cancelled download did not finish within 10 seconds"
            );
        }
    }

    #[test]
    fn dismiss_removes_entry() {
        let dm = DownloadManager::new();
        let dir = tempfile::tempdir().unwrap();

        dm.start_download_to("https://github.com/robots.txt".to_string(), dir.path());

        let id = dm.downloads().lock().unwrap()[0].id;
        dm.cancel(id);

        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if dm.downloads().lock().unwrap()[0].is_finished() {
                break;
            }
            assert!(Instant::now() < deadline);
        }

        dm.dismiss(id);
        assert!(dm.downloads().lock().unwrap().is_empty());
    }
}
