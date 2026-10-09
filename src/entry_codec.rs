//! Schema-3 independent zlib entries. Only the builder writes these bytes.
use crate::Entry;
use anyhow::{Context, Result, ensure};
use flate2::{Compression, Decompress, FlushDecompress, Status, write::ZlibEncoder};
use std::io::Write;

pub(crate) const MAX_RAW: usize = 1024 * 1024;
pub(crate) const MAX_COMPRESSED: usize = 2 * 1024 * 1024;

pub(crate) fn encode(entry: &Entry) -> Result<(usize, Vec<u8>)> {
    let raw = serde_json::to_vec(entry)?;
    ensure!(raw.len() <= MAX_RAW, "Dictionary entry exceeds 1 MiB");
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(6));
    encoder.write_all(&raw)?;
    let payload = encoder.finish()?;
    ensure!(
        payload.len() <= MAX_COMPRESSED,
        "Compressed entry exceeds 2 MiB"
    );
    Ok((raw.len(), payload))
}

pub(crate) fn decode(key: &str, raw_len: i64, payload: &[u8]) -> Result<Entry> {
    ensure!(
        (1..=MAX_RAW as i64).contains(&raw_len),
        "Corrupt entry length: {key}"
    );
    ensure!(
        !payload.is_empty() && payload.len() <= MAX_COMPRESSED,
        "Corrupt compressed entry length: {key}"
    );
    // One extra byte detects output longer than its declared length without
    // allowing an untrusted stream to grow a buffer or consume unbounded memory.
    let mut raw = vec![0; raw_len as usize + 1];
    let mut decoder = Decompress::new(true);
    let status = decoder
        .decompress(payload, &mut raw, FlushDecompress::Finish)
        .with_context(|| format!("Corrupt compressed entry: {key}"))?;
    ensure!(
        status == Status::StreamEnd
            && decoder.total_in() == payload.len() as u64
            && decoder.total_out() == raw_len as u64,
        "Corrupt entry stream or length: {key}"
    );
    raw.truncate(raw_len as usize);
    let entry: Entry =
        serde_json::from_slice(&raw).with_context(|| format!("Corrupt entry JSON: {key}"))?;
    ensure!(entry.key == key, "Corrupt entry key: {key}");
    Ok(entry)
}

#[cfg(test)]
mod tests {
    // F3 ← S2: bounded raw/compressed length, stream completion, and key agreement.
    use super::*;

    fn fixture() -> Entry {
        serde_json::from_value(serde_json::json!({
            "key":"café", "headword":"café", "score":100,
            "groups":[{"headword":"café", "pos":"noun", "senses":[{
                "glosses":["source definition"], "examples":[{"text":"source example", "reference":"source"}]
            }]}], "source_url":"source"
        })).unwrap()
    }

    #[test]
    fn roundtrip_preserves_every_source_field() {
        let entry = fixture();
        let (len, bytes) = encode(&entry).unwrap();
        let decoded = decode("café", len as i64, &bytes).unwrap();
        assert_eq!(
            serde_json::to_value(decoded).unwrap(),
            serde_json::to_value(entry).unwrap()
        );
    }

    #[test]
    fn corrupt_streams_lengths_and_keys_are_rejected() {
        let (len, bytes) = encode(&fixture()).unwrap();
        assert!(decode("wrong", len as i64, &bytes).is_err());
        for n in [0, -1, len as i64 - 1, len as i64 + 1, MAX_RAW as i64 + 1] {
            assert!(decode("café", n, &bytes).is_err());
        }
        for n in [0, 1, bytes.len() - 1] {
            assert!(decode("café", len as i64, &bytes[..n]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode("café", len as i64, &trailing).is_err());
        let mut concatenated = bytes.clone();
        concatenated.extend_from_slice(&bytes);
        assert!(decode("café", len as i64, &concatenated).is_err());
        let mut damaged = bytes;
        let end = damaged.len() - 1;
        damaged[end] ^= 1;
        assert!(decode("café", len as i64, &damaged).is_err());
        assert!(decode("café", 1, &vec![0; MAX_COMPRESSED + 1]).is_err());
    }

    #[test]
    fn invalid_json_and_oversized_entries_are_rejected() {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(6));
        encoder.write_all(b"not json").unwrap();
        assert!(decode("café", 8, &encoder.finish().unwrap()).is_err());
        let mut entry = fixture();
        entry.groups[0].senses[0].glosses = vec!["x".repeat(MAX_RAW)];
        assert!(encode(&entry).is_err());
    }

    #[test]
    fn maximum_length_and_high_expansion_have_bounded_output() {
        let mut entry = fixture();
        let overhead =
            serde_json::to_vec(&entry).unwrap().len() - entry.groups[0].senses[0].glosses[0].len();
        entry.groups[0].senses[0].glosses[0] = "x".repeat(MAX_RAW - overhead);
        let (len, payload) = encode(&entry).unwrap();
        assert_eq!(len, MAX_RAW);
        assert_eq!(
            decode("café", len as i64, &payload).unwrap().groups[0].senses[0].glosses,
            entry.groups[0].senses[0].glosses
        );
        assert!(decode("café", 1, &payload).is_err());
        entry.groups[0].senses[0].glosses[0].push('x');
        assert!(encode(&entry).is_err());
    }
}
