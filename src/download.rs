//! Explicit installation of prebuilt packs. Lookup and pack generation do not call this module.
//!
//! F4 ← S2: the installer copies verified prebuilt bytes and never calls the data
//! generator; scripts/test_boundaries.py fails if it references a pack encoder.
use crate::{
    Dictionary,
    model::{Manifest, RANKING, SCHEMA_VERSION},
};
use anyhow::{Context, Result, ensure};
use directories::ProjectDirs;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
use ureq::unversioned::{
    resolver::DefaultResolver,
    transport::{Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport},
};

// Bound each stalled read rather than the duration of the whole 190 MiB download.
#[derive(Debug)]
struct IdleConnector(Duration);
#[derive(Debug)]
struct IdleTransport {
    inner: Box<dyn Transport>,
    idle: Duration,
}
impl Connector for IdleConnector {
    type Out = IdleTransport;
    fn connect(
        &self,
        details: &ConnectionDetails,
        chained: Option<()>,
    ) -> std::result::Result<Option<Self::Out>, ureq::Error> {
        Ok(DefaultConnector::default()
            .connect(details, chained)?
            .map(|inner| IdleTransport {
                inner,
                idle: self.0,
            }))
    }
}
impl Transport for IdleTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }
    fn transmit_output(
        &mut self,
        amount: usize,
        timeout: NextTimeout,
    ) -> std::result::Result<(), ureq::Error> {
        self.inner.transmit_output(amount, timeout)
    }
    fn await_input(&mut self, mut timeout: NextTimeout) -> std::result::Result<bool, ureq::Error> {
        timeout.after = timeout
            .after
            .min(ureq::unversioned::transport::time::Duration::Exact(
                self.idle,
            ));
        self.inner.await_input(timeout)
    }
    fn is_open(&mut self) -> bool {
        self.inner.is_open()
    }
    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}

const MAX_METADATA: u64 = 1024 * 1024;
const MAX_ARCHIVE: u64 = 4 * 1024 * 1024 * 1024;
const MAX_EXPANDED: u64 = 16 * 1024 * 1024 * 1024;
const FILES: [&str; 5] = [
    "manifest.json",
    "entries.sqlite",
    "words.fst",
    "lexicon.bin",
    "THIRD_PARTY.md",
];

struct Source {
    api_url: String,
    asset_prefix: String,
    https_only: bool,
    timeout: Duration,
}
impl Source {
    fn official() -> Self {
        Self {
            api_url: "https://api.github.com/repos/starfield17/wordglide/releases/latest".into(),
            asset_prefix: "https://github.com/starfield17/wordglide/releases/download/".into(),
            https_only: true,
            timeout: Duration::from_secs(10),
        }
    }
    fn agent(&self) -> ureq::Agent {
        let config = ureq::Agent::config_builder()
            .https_only(self.https_only)
            .timeout_connect(Some(self.timeout))
            .timeout_resolve(Some(self.timeout))
            .timeout_recv_response(Some(self.timeout))
            .timeout_send_request(Some(self.timeout))
            .max_redirects(5)
            .user_agent(concat!("wordglide/", env!("CARGO_PKG_VERSION")))
            .build();
        ureq::Agent::with_parts(
            config,
            IdleConnector(self.timeout),
            DefaultResolver::default(),
        )
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
    browser_download_url: String,
}
impl Release {
    fn asset(&self, name: &str, source: &Source, limit: u64) -> Result<&Asset> {
        let mut matches = self.assets.iter().filter(|a| a.name == name);
        let asset = matches
            .next()
            .with_context(|| format!("Latest release {} has no {name}", self.tag_name))?;
        ensure!(matches.next().is_none(), "Duplicate release asset: {name}");
        ensure!(
            asset.size > 0 && asset.size <= limit,
            "Release asset size exceeds the supported limit: {name}"
        );
        ensure!(
            asset.browser_download_url
                == format!("{}{}/{name}", source.asset_prefix, self.tag_name),
            "Unexpected download location for {name}"
        );
        Ok(asset)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Receipt {
    directory: String,
    archive_sha256: String,
    release: String,
    snapshot: String,
}
fn read_receipt(root: &Path) -> Result<Option<Receipt>> {
    let path = root.join("current.json");
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("Cannot read {}", path.display())),
    };
    ensure!(
        bytes.len() <= MAX_METADATA as usize,
        "Oversized installation record"
    );
    let receipt: Receipt =
        serde_json::from_slice(&bytes).context("Invalid dictionary installation record")?;
    let name = receipt
        .directory
        .strip_prefix("packs/pack-")
        .context("Invalid installed directory")?;
    ensure!(
        !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric()),
        "Invalid installed directory"
    );
    ensure!(
        receipt.archive_sha256.len() == 64
            && receipt
                .archive_sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit()),
        "Invalid installed archive checksum"
    );
    Ok(Some(receipt))
}
fn locate(root: &Path) -> Result<Option<PathBuf>> {
    Ok(read_receipt(root)?.map(|r| root.join(r.directory).join("english-pack")))
}
pub(crate) fn root() -> Result<PathBuf> {
    ProjectDirs::from("org", "wordglide", "dict")
        .map(|d| d.data_dir().join("downloads"))
        .context("Cannot determine download directory")
}

/// Find an explicitly installed dictionary without making network requests.
/// CLI data overrides take precedence over this location.
pub fn downloaded_data_path() -> Result<Option<PathBuf>> {
    locate(&root()?)
}

#[derive(Debug, Clone)]
pub(crate) enum Progress {
    Info(String),
    Bytes { downloaded: u64, total: u64 },
    Finished(Result<Installed, String>),
}
#[derive(Debug, Clone)]
pub(crate) struct Installed {
    pub(crate) path: PathBuf,
    pub(crate) snapshot: String,
    pub(crate) already_current: bool,
}

pub(crate) struct Task {
    receiver: mpsc::Receiver<Progress>,
    cancel: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Task {
    pub(crate) fn start() -> Result<Self> {
        Self::start_with_cancel(Arc::new(AtomicBool::new(false)))
    }
    pub(crate) fn start_with_cancel(cancel: Arc<AtomicBool>) -> Result<Self> {
        let root = root()?;
        let (sender, receiver) = mpsc::channel();
        let worker_cancel = Arc::clone(&cancel);
        let worker = thread::spawn(move || {
            let result = install(&root, &Source::official(), &worker_cancel, |p| {
                let _ = sender.send(p);
            })
            .map_err(|e| format!("{e:#}"));
            let _ = sender.send(Progress::Finished(result));
        });
        Ok(Self {
            receiver,
            cancel,
            worker: Some(worker),
        })
    }
    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub(crate) fn poll(&self) -> impl Iterator<Item = Progress> + '_ {
        self.receiver.try_iter()
    }
}
impl Drop for Task {
    fn drop(&mut self) {
        self.cancel();
        // Finish cleanup before process exit; network waits have bounded timeouts.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn cancelled(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "Download cancelled");
    Ok(())
}
fn fetch(agent: &ureq::Agent, url: &str, limit: u64, cancel: &AtomicBool) -> Result<Vec<u8>> {
    cancelled(cancel)?;
    let mut response = agent
        .get(url)
        .call()
        .with_context(|| format!("Cannot fetch {url}"))?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= limit, "Response exceeds size limit");
    cancelled(cancel)?;
    Ok(bytes)
}
fn copy_hashed(
    mut reader: impl Read,
    mut writer: impl Write,
    expected: u64,
    cancel: &AtomicBool,
    mut report: impl FnMut(u64),
) -> Result<String> {
    let mut hash = Sha256::new();
    let mut total = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        cancelled(cancel)?;
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        ensure!(
            total <= expected,
            "Downloaded or extracted file exceeds expected size"
        );
        writer.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
        report(total);
    }
    ensure!(
        total == expected,
        "Incomplete file: expected {expected} bytes, received {total}"
    );
    Ok(format!("{:x}", hash.finalize()))
}
fn checksum(bytes: &[u8]) -> Result<String> {
    let text = std::str::from_utf8(bytes).context("Invalid SHA256SUMS.txt")?;
    let mut found = None;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if let (Some(hash), Some("english-pack.tar.gz"), None) =
            (parts.next(), parts.next(), parts.next())
        {
            ensure!(
                hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid archive checksum"
            );
            ensure!(found.is_none(), "Duplicate archive checksum");
            found = Some(hash.to_ascii_lowercase());
        }
    }
    found.context("SHA256SUMS.txt has no dictionary archive checksum")
}

fn install(
    root: &Path,
    source: &Source,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<Installed> {
    cancelled(cancel)?;
    fs::create_dir_all(root)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("install.lock"))?;
    FileExt::try_lock_exclusive(&lock)
        .context("Another dictionary installation is already running")?;
    let agent = source.agent();
    progress(Progress::Info("Checking latest dictionary release…".into()));
    let release: Release =
        serde_json::from_slice(&fetch(&agent, &source.api_url, MAX_METADATA, cancel)?)
            .context("Invalid release response")?;
    ensure!(
        !release.tag_name.is_empty()
            && release.tag_name.len() <= 100
            && release
                .tag_name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
        "Invalid release tag"
    );
    let asset = release.asset("english-pack.tar.gz", source, MAX_ARCHIVE)?;
    let sums = release.asset("SHA256SUMS.txt", source, MAX_METADATA)?;
    let hash = checksum(&fetch(
        &agent,
        &sums.browser_download_url,
        MAX_METADATA,
        cancel,
    )?)?;
    // A damaged receipt cannot qualify for reuse, but must not block repair.
    // Keep its bytes until a fully verified replacement is atomically activated.
    if let Ok(Some(receipt)) = read_receipt(root) {
        let path = root.join(&receipt.directory).join("english-pack");
        if receipt.archive_sha256 == hash && Dictionary::open(&path).is_ok() {
            return Ok(Installed {
                path,
                snapshot: receipt.snapshot,
                already_current: true,
            });
        }
    }
    let packs = root.join("packs");
    fs::create_dir_all(&packs)?;
    let stage = tempfile::Builder::new()
        .prefix("pack-")
        .tempdir_in(&packs)?;
    let archive_path = stage.path().join("archive.tar.gz");
    progress(Progress::Info(format!(
        "Downloading {} · {:.1} MiB → {}",
        release.tag_name,
        asset.size as f64 / 1048576.0,
        root.display()
    )));
    let mut response = agent
        .get(&asset.browser_download_url)
        .call()
        .context("Cannot download dictionary archive")?;
    let mut archive_file = fs::File::create(&archive_path)?;
    let actual = copy_hashed(
        response.body_mut().as_reader(),
        &mut archive_file,
        asset.size,
        cancel,
        |downloaded| {
            progress(Progress::Bytes {
                downloaded,
                total: asset.size,
            })
        },
    )?;
    ensure!(actual == hash, "Dictionary archive SHA-256 mismatch");
    archive_file.sync_all()?;
    drop(archive_file);
    progress(Progress::Info(
        "Verifying and extracting dictionary…".into(),
    ));
    let pack = stage.path().join("english-pack");
    fs::create_dir(&pack)?;
    let gzip = flate2::read::GzDecoder::new(fs::File::open(&archive_path)?);
    let mut tar = tar::Archive::new(gzip.take(MAX_EXPANDED + 1));
    let mut seen = HashSet::new();
    let mut hashes = std::collections::HashMap::new();
    let mut sizes = std::collections::HashMap::new();
    let mut expanded = 0u64;
    for entry in tar.entries()?.raw(true) {
        cancelled(cancel)?;
        let mut entry = entry?;
        let path = entry.path_bytes();
        let name = std::str::from_utf8(&path)?
            .strip_prefix("english-pack/")
            .context("Unexpected archive member")?
            .to_string();
        ensure!(
            entry.header().entry_type().is_file()
                && FILES.contains(&name.as_str())
                && seen.insert(name.clone()),
            "Unsafe, unexpected, or duplicate archive member"
        );
        let size = entry.size();
        expanded = expanded
            .checked_add(size)
            .context("Archive size overflow")?;
        ensure!(
            expanded <= MAX_EXPANDED && (name != "manifest.json" || size <= MAX_METADATA),
            "Extracted data exceeds size limit"
        );
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(pack.join(&name))?;
        let digest = copy_hashed(&mut entry, &mut file, size, cancel, |_| {})?;
        file.sync_all()?;
        hashes.insert(name.clone(), digest);
        sizes.insert(name, size);
    }
    // Consume the gzip footer as well, checking CRC/truncation before installation.
    let mut remainder = tar.into_inner();
    let mut remaining = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        cancelled(cancel)?;
        let count = remainder.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        remaining += count as u64;
    }
    ensure!(
        expanded + remaining <= MAX_EXPANDED,
        "Extracted data exceeds size limit"
    );
    ensure!(seen.len() == FILES.len(), "Incomplete dictionary archive");
    let manifest: Manifest = serde_json::from_slice(&fs::read(pack.join("manifest.json"))?)?;
    ensure!(
        manifest.schema_version == SCHEMA_VERSION && manifest.ranking == RANKING,
        "Incompatible dictionary format; update Wordglide"
    );
    for name in ["entries.sqlite", "words.fst", "lexicon.bin"] {
        ensure!(
            manifest.files.get(name) == hashes.get(name)
                && manifest.sizes.get(name) == sizes.get(name),
            "Corrupt dictionary file: {name}"
        );
    }
    Dictionary::open(&pack).context("Downloaded dictionary failed structural verification")?;
    cancelled(cancel)?;
    let snapshot = manifest
        .source
        .get("snapshot")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown")
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect::<String>();
    let directory = format!(
        "packs/{}",
        stage
            .path()
            .file_name()
            .context("Missing staging name")?
            .to_string_lossy()
    );
    let receipt = Receipt {
        directory,
        archive_sha256: hash,
        release: release.tag_name,
        snapshot: snapshot.clone(),
    };
    fs::remove_file(archive_path)?;
    fs::File::open(&pack)?.sync_all()?;
    fs::File::open(stage.path())?.sync_all()?;
    fs::File::open(&packs)?.sync_all()?;
    let mut pointer = tempfile::NamedTempFile::new_in(root)?;
    serde_json::to_writer_pretty(&mut pointer, &receipt)?;
    pointer.write_all(b"\n")?;
    pointer.as_file().sync_all()?;
    progress(Progress::Info("Activating verified dictionary…".into()));
    cancelled(cancel)?;
    pointer
        .persist(root.join("current.json"))
        .context("Cannot activate downloaded dictionary")?;
    let directory = stage.keep();
    // Once the pointer is committed, the verified pack must survive even if syncing fails.
    let _ = fs::File::open(root).and_then(|f| f.sync_all());
    Ok(Installed {
        path: directory.join("english-pack"),
        snapshot,
        already_current: false,
    })
}

#[cfg(test)]
mod tests;
