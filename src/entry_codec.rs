//! Schema-4 positional MessagePack entries and bounded independent Zstd frames.
use crate::{Entry, Example, Group, Sense};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

pub(crate) const MAX_RAW: usize = 1024 * 1024;
pub(crate) const MAX_COMPRESSED: usize = 2 * MAX_RAW;
pub(crate) const MAX_DICTIONARY: usize = 64 * 1024;

// The wire types freeze schema-4 field order independently of the public JSON types.
#[derive(Serialize, Deserialize)]
struct WireEntry(
    String,
    String,
    i32,
    Vec<WireGroup>,
    Vec<String>,
    bool,
    String,
);
#[derive(Serialize, Deserialize)]
struct WireGroup(String, String, Vec<String>, Vec<WireSense>, Option<u32>);
#[derive(Serialize, Deserialize)]
struct WireSense(Vec<String>, Vec<String>, Vec<WireExample>, Vec<String>);
#[derive(Serialize, Deserialize)]
struct WireExample(String, String, String);

pub(crate) fn encode(entry: &Entry) -> Result<Vec<u8>> {
    ensure!(
        serde_json::to_vec(entry)?.len() <= MAX_RAW,
        "Dictionary entry exceeds 1 MiB"
    );
    let groups = entry
        .groups
        .iter()
        .map(|g| {
            WireGroup(
                g.headword.clone(),
                g.pos.clone(),
                g.ipa.clone(),
                g.senses
                    .iter()
                    .map(|s| {
                        WireSense(
                            s.glosses.clone(),
                            s.tags.clone(),
                            s.examples
                                .iter()
                                .map(|e| {
                                    WireExample(e.text.clone(), e.reference.clone(), e.kind.clone())
                                })
                                .collect(),
                            s.targets.clone(),
                        )
                    })
                    .collect(),
                g.etymology_number,
            )
        })
        .collect();
    let bytes = rmp_serde::to_vec(&WireEntry(
        entry.key.clone(),
        entry.headword.clone(),
        entry.score,
        groups,
        entry.lemmas.clone(),
        entry.preview_lemmas,
        entry.source_url.clone(),
    ))?;
    ensure!(bytes.len() <= MAX_RAW, "Binary entry exceeds 1 MiB");
    Ok(bytes)
}

// Preflight length declarations before Serde can reserve a collection. Only the
// schema's integer/string/array/nil/bool vocabulary is accepted, with bounded depth.
fn preflight(mut bytes: &[u8]) -> Result<()> {
    fn take<'a>(bytes: &mut &'a [u8], n: usize) -> Result<&'a [u8]> {
        ensure!(n <= bytes.len(), "Corrupt MessagePack length");
        let (head, tail) = bytes.split_at(n);
        *bytes = tail;
        Ok(head)
    }
    fn value(bytes: &mut &[u8], depth: usize) -> Result<()> {
        ensure!(depth <= 16, "Corrupt MessagePack nesting");
        let tag = take(bytes, 1)?[0];
        let (count, string) = match tag {
            0x00..=0x7f | 0xe0..=0xff | 0xc0 | 0xc2 | 0xc3 => return Ok(()),
            0xcc | 0xd0 => {
                take(bytes, 1)?;
                return Ok(());
            }
            0xcd | 0xd1 => {
                take(bytes, 2)?;
                return Ok(());
            }
            0xce | 0xd2 => {
                take(bytes, 4)?;
                return Ok(());
            }
            0xcf | 0xd3 => {
                take(bytes, 8)?;
                return Ok(());
            }
            0x90..=0x9f => ((tag & 15) as usize, false),
            0xa0..=0xbf => ((tag & 31) as usize, true),
            0xd9 => (take(bytes, 1)?[0] as usize, true),
            0xda | 0xdc => (
                u16::from_be_bytes(take(bytes, 2)?.try_into()?) as usize,
                tag == 0xda,
            ),
            0xdb | 0xdd => (
                usize::try_from(u32::from_be_bytes(take(bytes, 4)?.try_into()?))?,
                tag == 0xdb,
            ),
            _ => bail!("Unsupported MessagePack value"),
        };
        ensure!(count <= bytes.len(), "Corrupt MessagePack length");
        if string {
            std::str::from_utf8(take(bytes, count)?)?;
        } else {
            for _ in 0..count {
                value(bytes, depth + 1)?;
            }
        }
        Ok(())
    }
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_RAW,
        "Corrupt entry length"
    );
    value(&mut bytes, 0)?;
    ensure!(bytes.is_empty(), "Trailing MessagePack data");
    Ok(())
}

pub(crate) fn decode(key: &str, bytes: &[u8]) -> Result<Entry> {
    preflight(bytes).with_context(|| format!("Corrupt entry: {key}"))?;
    let WireEntry(k, headword, score, groups, lemmas, preview_lemmas, source_url) =
        rmp_serde::from_slice(bytes)?;
    ensure!(k == key, "Corrupt entry key: {key}");
    Ok(Entry {
        key: k,
        headword,
        score,
        groups: groups
            .into_iter()
            .map(
                |WireGroup(headword, pos, ipa, senses, etymology_number)| Group {
                    headword,
                    pos,
                    ipa,
                    etymology_number,
                    senses: senses
                        .into_iter()
                        .map(|WireSense(glosses, tags, examples, targets)| Sense {
                            glosses,
                            tags,
                            targets,
                            examples: examples
                                .into_iter()
                                .map(|WireExample(text, reference, kind)| Example {
                                    text,
                                    reference,
                                    kind,
                                })
                                .collect(),
                        })
                        .collect(),
                },
            )
            .collect(),
        lemmas,
        preview_lemmas,
        source_url,
    })
}

pub(crate) fn compress(encoder: &mut zstd::bulk::Compressor<'_>, raw: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        !raw.is_empty() && raw.len() <= MAX_RAW,
        "Invalid block length"
    );
    let bytes = encoder.compress(raw)?;
    ensure!(
        bytes.len() <= MAX_COMPRESSED,
        "Compressed block exceeds limit"
    );
    Ok(bytes)
}

pub(crate) fn decompress(
    decoder: &mut zstd::bulk::Decompressor<'_>,
    raw_len: usize,
    bytes: &[u8],
) -> Result<Vec<u8>> {
    ensure!((1..=MAX_RAW).contains(&raw_len), "Corrupt block raw length");
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_COMPRESSED,
        "Corrupt compressed block length"
    );
    ensure!(
        zstd::zstd_safe::find_frame_compressed_size(bytes)
            .map_err(|e| anyhow::anyhow!("Corrupt Zstd frame: {e}"))?
            == bytes.len(),
        "Trailing Zstd data"
    );
    // Frames must advertise their size and checksum; the builder always writes both.
    ensure!(
        bytes.len() >= 5 && bytes[4] & 4 != 0,
        "Missing Zstd checksum"
    );
    ensure!(
        zstd::zstd_safe::get_frame_content_size(bytes)
            .map_err(|_| anyhow::anyhow!("Corrupt Zstd header"))?
            == Some(raw_len as u64),
        "Corrupt Zstd output length"
    );
    let raw = decoder.decompress(bytes, raw_len)?;
    ensure!(raw.len() == raw_len, "Corrupt block output length");
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Entry {
        serde_json::from_value(
            serde_json::json!({"key":"café", "headword":"Café", "score":100,
            "groups":[{"headword":"café", "pos":"noun", "ipa":["ipa"], "etymology_number":2,
            "senses":[{"glosses":["source definition"], "tags":["archaic"], "targets":["coffee"],
            "examples":[{"text":"source example", "reference":"source", "kind":"quotation"}]}]}],
            "lemmas":["coffee"], "preview_lemmas":true, "source_url":"source"}),
        )
        .unwrap()
    }
    #[test]
    fn roundtrip_preserves_every_source_field() {
        let entry = fixture();
        assert_eq!(
            serde_json::to_value(decode("café", &encode(&entry).unwrap()).unwrap()).unwrap(),
            serde_json::to_value(entry).unwrap()
        );
    }
    #[test]
    fn cache_counts_owned_short_strings_instead_of_wire_bytes() {
        let mut entry = fixture();
        entry.groups[0].senses[0].tags = vec![String::new(); 10000];
        let wire = encode(&entry).unwrap();
        let decoded = decode("café", &wire).unwrap();
        assert!(decoded.cache_bytes() >= 10000 * std::mem::size_of::<String>());
        assert!(decoded.cache_bytes() > wire.len() * 8 + 512);
    }
    #[test]
    fn canonical_json_at_the_existing_size_limit_roundtrips() {
        let mut entry = fixture();
        let overhead =
            serde_json::to_vec(&entry).unwrap().len() - entry.groups[0].senses[0].glosses[0].len();
        entry.groups[0].senses[0].glosses[0] = "x".repeat(MAX_RAW - overhead);
        assert_eq!(serde_json::to_vec(&entry).unwrap().len(), MAX_RAW);
        let bytes = encode(&entry).unwrap();
        assert_eq!(
            decode("café", &bytes).unwrap().groups[0].senses[0].glosses,
            entry.groups[0].senses[0].glosses
        );
        entry.groups[0].senses[0].glosses[0].push('x');
        assert!(encode(&entry).is_err());
    }
    #[test]
    fn malformed_lengths_nesting_keys_and_tails_are_rejected() {
        let bytes = encode(&fixture()).unwrap();
        assert!(decode("wrong", &bytes).is_err());
        for n in 0..bytes.len() {
            assert!(decode("café", &bytes[..n]).is_err());
        }
        let mut tail = bytes;
        tail.push(0);
        assert!(decode("café", &tail).is_err());
        for b in [
            vec![0xdd, 255, 255, 255, 255],
            vec![0xdb, 255, 255, 255, 255],
            vec![0x91; 32],
            vec![0x81, 0, 0],
            vec![0; MAX_RAW + 1],
        ] {
            assert!(decode("café", &b).is_err());
        }
    }
    #[test]
    fn bounded_frames_reject_corruption_and_concatenation() {
        let mut encoder = zstd::bulk::Compressor::new(19).unwrap();
        encoder.include_checksum(true).unwrap();
        let mut decoder = zstd::bulk::Decompressor::new().unwrap();
        decoder.window_log_max(20).unwrap();
        let raw = vec![b'x'; MAX_RAW];
        let bytes = compress(&mut encoder, &raw).unwrap();
        assert_eq!(decompress(&mut decoder, MAX_RAW, &bytes).unwrap(), raw);
        for len in [0, MAX_RAW - 1, MAX_RAW + 1] {
            assert!(decompress(&mut decoder, len, &bytes).is_err());
        }
        for n in [0, 1, bytes.len() - 1] {
            assert!(decompress(&mut decoder, MAX_RAW, &bytes[..n]).is_err());
        }
        let mut tail = bytes.clone();
        tail.push(0);
        assert!(decompress(&mut decoder, MAX_RAW, &tail).is_err());
        let mut concat = bytes.clone();
        concat.extend_from_slice(&bytes);
        assert!(decompress(&mut decoder, MAX_RAW, &concat).is_err());
        let mut damaged = bytes;
        *damaged.last_mut().unwrap() ^= 1;
        assert!(decompress(&mut decoder, MAX_RAW, &damaged).is_err());
        assert!(compress(&mut encoder, &vec![0; MAX_RAW + 1]).is_err());
        encoder.include_checksum(false).unwrap();
        let unchecked = encoder.compress(b"a").unwrap();
        assert!(decompress(&mut decoder, 1, &unchecked).is_err());
        let mut entry = fixture();
        entry.groups[0].senses[0].glosses = vec!["x".repeat(MAX_RAW)];
        assert!(encode(&entry).is_err());
    }
}
