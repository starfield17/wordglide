use super::*;
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
};

struct Server {
    source: Source,
    routes: Arc<Mutex<HashMap<String, (u16, Vec<u8>, Duration)>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Server {
    fn new(archive: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let routes = Arc::new(Mutex::new(HashMap::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_routes = Arc::clone(&routes);
        let worker_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                let Ok((mut stream, _)) = listener.accept() else {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                while !request.ends_with(b"\r\n\r\n") && request.len() < 16384 {
                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => request.extend_from_slice(&buffer[..n]),
                    }
                }
                let request = String::from_utf8_lossy(&request);
                let path = request.split_whitespace().nth(1).unwrap_or("");
                let (status, body, delay) = worker_routes
                    .lock()
                    .unwrap()
                    .get(path)
                    .cloned()
                    .unwrap_or((404, Vec::new(), Duration::ZERO));
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                thread::sleep(delay);
                let _ = stream.write_all(&body);
            }
        });
        let server = Self {
            source: Source {
                api_url: format!("{base}/latest"),
                asset_prefix: format!("{base}/assets/"),
                https_only: false,
                timeout: Duration::from_millis(150),
            },
            routes,
            stop,
            worker: Some(worker),
        };
        let release = serde_json::json!({"tag_name":"v1.0.0", "assets":[
            {"name":"english-pack.tar.gz","size":archive.len(),"browser_download_url":format!("{base}/assets/english-pack.tar.gz")},
            {"name":"SHA256SUMS.txt","size":84,"browser_download_url":format!("{base}/assets/SHA256SUMS.txt")}
        ]});
        server.put("/latest", serde_json::to_vec(&release).unwrap());
        server.put(
            "/assets/SHA256SUMS.txt",
            format!("{:x}  english-pack.tar.gz\n", Sha256::digest(&archive)).into_bytes(),
        );
        server.put("/assets/english-pack.tar.gz", archive);
        server
    }
    fn put(&self, path: &str, body: Vec<u8>) {
        self.routes
            .lock()
            .unwrap()
            .insert(path.into(), (200, body, Duration::ZERO));
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}

fn pack() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/sample");
    crate::build_pack(
        &sample.join("entries.jsonl"),
        &sample.join("source.json"),
        &dir.path().join("pack"),
    )
    .unwrap();
    dir
}
fn archive(pack: &Path, extra: Option<(&str, tar::EntryType)>) -> Vec<u8> {
    let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut builder = tar::Builder::new(gzip);
    for name in [
        "manifest.json",
        "entries.sqlite",
        "words.fst",
        "lexicon.bin",
        "THIRD_PARTY.md",
    ] {
        let bytes = if name == "THIRD_PARTY.md" {
            b"fixture attribution".to_vec()
        } else {
            fs::read(pack.join(name)).unwrap()
        };
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(
                &mut header,
                format!("english-pack/{name}"),
                bytes.as_slice(),
            )
            .unwrap();
    }
    if let Some((name, kind)) = extra {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(kind);
        header.set_size(0);
        header.set_mode(0o644);
        if kind.is_symlink() {
            header.set_link_name("/tmp/outside").unwrap();
        }
        header.set_cksum();
        builder.append_data(&mut header, name, &[][..]).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}
fn run(root: &Path, source: &Source) -> Result<Installed> {
    install(root, source, &AtomicBool::new(false), |_| {})
}

#[test]
fn installs_verified_pack_and_reuses_same_archive_without_downloading() {
    let fixture = pack();
    let server = Server::new(archive(&fixture.path().join("pack"), None));
    let root = tempfile::tempdir().unwrap();
    assert!(locate(root.path()).unwrap().is_none());
    let installed = run(root.path(), &server.source).unwrap();
    assert_eq!(locate(root.path()).unwrap(), Some(installed.path.clone()));
    assert_eq!(crate::verify_pack(&installed.path).unwrap(), 89);
    server
        .routes
        .lock()
        .unwrap()
        .remove("/assets/english-pack.tar.gz");
    assert!(run(root.path(), &server.source).unwrap().already_current);
}

#[test]
fn new_archive_switches_pointer_and_keeps_old_pack_readable() {
    let fixture = pack();
    let root = tempfile::tempdir().unwrap();
    let first = Server::new(archive(&fixture.path().join("pack"), None));
    let old = run(root.path(), &first.source).unwrap();
    let mut dictionary = Dictionary::open(&old.path).unwrap();
    let manifest = fixture.path().join("pack/manifest.json");
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    json["source"]["snapshot"] = "new-snapshot".into();
    fs::write(manifest, serde_json::to_vec(&json).unwrap()).unwrap();
    let second = Server::new(archive(&fixture.path().join("pack"), None));
    let new = run(root.path(), &second.source).unwrap();
    assert_ne!(old.path, new.path);
    assert_eq!(locate(root.path()).unwrap(), Some(new.path));
    assert_eq!(dictionary.search("fist").unwrap()[0].key, "fist");
    assert_eq!(crate::verify_pack(&old.path).unwrap(), 89);
}

#[test]
fn failures_never_replace_existing_pointer() {
    let fixture = pack();
    let bytes = archive(&fixture.path().join("pack"), None);
    let root = tempfile::tempdir().unwrap();
    let server = Server::new(bytes.clone());
    run(root.path(), &server.source).unwrap();
    let before = fs::read(root.path().join("current.json")).unwrap();
    for body in [
        b"not json".to_vec(),
        b"{\"tag_name\":\"v2\",\"assets\":[]}".to_vec(),
    ] {
        let broken = Server::new(bytes.clone());
        broken.put("/latest", body);
        assert!(run(root.path(), &broken.source).is_err());
        assert_eq!(fs::read(root.path().join("current.json")).unwrap(), before);
    }
    let broken = Server::new(bytes);
    broken.put(
        "/assets/SHA256SUMS.txt",
        format!("{}  english-pack.tar.gz\n", "0".repeat(64)).into_bytes(),
    );
    assert!(run(root.path(), &broken.source).is_err());
    assert_eq!(fs::read(root.path().join("current.json")).unwrap(), before);
}

#[test]
fn rejects_unsafe_duplicate_and_incompatible_archives() {
    let fixture = pack();
    for extra in [
        Some(("english-pack/words.fst", tar::EntryType::Regular)),
        Some(("outside", tar::EntryType::Regular)),
        Some(("english-pack/link", tar::EntryType::Symlink)),
    ] {
        let root = tempfile::tempdir().unwrap();
        let server = Server::new(archive(&fixture.path().join("pack"), extra));
        assert!(run(root.path(), &server.source).is_err());
        assert!(!root.path().join("current.json").exists());
    }
    let manifest = fixture.path().join("pack/manifest.json");
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    json["schema_version"] = 999.into();
    fs::write(manifest, serde_json::to_vec(&json).unwrap()).unwrap();
    let server = Server::new(archive(&fixture.path().join("pack"), None));
    let root = tempfile::tempdir().unwrap();
    assert!(run(root.path(), &server.source).is_err());
    assert!(!root.path().join("current.json").exists());
}

#[test]
fn handles_cancellation_timeout_lock_contention_and_commit_failure() {
    let fixture = pack();
    let server = Server::new(archive(&fixture.path().join("pack"), None));
    let root = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(true);
    assert!(install(root.path(), &server.source, &cancel, |_| {}).is_err());
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.path().join("install.lock"))
        .unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    assert!(run(root.path(), &server.source).is_err());
    fs2::FileExt::unlock(&lock).unwrap();
    server
        .routes
        .lock()
        .unwrap()
        .get_mut("/assets/english-pack.tar.gz")
        .unwrap()
        .2 = Duration::from_millis(350);
    assert!(run(root.path(), &server.source).is_err());
    server
        .routes
        .lock()
        .unwrap()
        .get_mut("/assets/english-pack.tar.gz")
        .unwrap()
        .2 = Duration::ZERO;
    fs::create_dir(root.path().join("current.json")).unwrap();
    assert!(run(root.path(), &server.source).is_err());
    assert!(root.path().join("current.json").is_dir());
}

#[test]
fn installed_pointer_cannot_escape_managed_storage() {
    let root = tempfile::tempdir().unwrap();
    for directory in ["../../outside", "/outside", "packs/../outside"] {
        fs::write(root.path().join("current.json"), serde_json::to_vec(&serde_json::json!({"directory":directory,"archive_sha256":"0".repeat(64),"release":"v1","snapshot":"x"})).unwrap()).unwrap();
        assert!(locate(root.path()).is_err());
    }
}
