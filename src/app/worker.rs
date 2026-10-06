use crate::{Candidate, Dictionary, Preview};
use std::{sync::mpsc, thread};

pub(super) enum Request {
    Search(u64, String),
    Preview(u64, Candidate),
}
pub(super) enum Response {
    Search(u64, Result<(Vec<Candidate>, Option<Preview>), String>),
    Preview(u64, String, Result<Preview, String>),
}
/// Spawn the single worker thread and return its request and response channels.
pub(super) fn spawn(
    mut dictionary: Dictionary,
) -> (mpsc::Sender<Request>, mpsc::Receiver<Response>) {
    let (tx, requests) = mpsc::channel();
    let (responses, rx) = mpsc::channel();
    thread::spawn(move || {
        while let Ok(mut request) = requests.recv() {
            // Coalesce queued keystrokes/selection changes, never publish an old result in the UI.
            for newer in requests.try_iter() {
                request = newer;
            }
            let response = match request {
                Request::Search(id, text) => {
                    let result = (|| {
                        let candidates = dictionary.search(&text)?;
                        let preview = candidates
                            .first()
                            .map(|c| dictionary.preview(c))
                            .transpose()?;
                        Ok::<_, anyhow::Error>((candidates, preview))
                    })()
                    .map_err(|e| format!("{e:#}"));
                    Response::Search(id, result)
                }
                Request::Preview(id, candidate) => Response::Preview(
                    id,
                    candidate.key.clone(),
                    dictionary.preview(&candidate).map_err(|e| format!("{e:#}")),
                ),
            };
            if responses.send(response).is_err() {
                break;
            }
        }
    });
    (tx, rx)
}
