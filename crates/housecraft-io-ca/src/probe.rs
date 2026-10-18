//! Identify what a file is made of before trying to read it.

use std::io::{Cursor, Read};

use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Container {
    Zip {
        entries: Vec<ZipEntry>,
    },
    Sqlite,
    Gzip,
    Xml,
    Json,
    /// No recognised container; see `zlib_streams` and the entropy map.
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct ZipEntry {
    pub name: String,
    pub size: u64,
    pub compressed_size: u64,
}

/// A zlib stream found inside the file.
#[derive(Debug, Clone, Serialize)]
pub struct Stream {
    pub offset: usize,
    pub compressed_len: usize,
    pub inflated_len: usize,
    pub inflated_sha256: String,
    /// First bytes of the inflated data, hex.
    pub inflated_head: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FoundString {
    pub offset: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeReport {
    pub size: usize,
    pub sha256: String,
    /// First 32 bytes, hex.
    pub head: String,
    pub container: Container,
    /// Shannon entropy (bits per byte, 0–8) of each `ENTROPY_BLOCK` bytes.
    /// Near 8 means compressed or encrypted; low values mean structured data.
    pub entropy: Vec<f32>,
    pub zlib_streams: Vec<Stream>,
    pub ascii_strings: Vec<FoundString>,
    pub utf16_strings: Vec<FoundString>,
}

pub const ENTROPY_BLOCK: usize = 4096;
const MAX_STRINGS: usize = 400;
const MIN_STRING: usize = 5;
const MAX_INFLATE: u64 = 256 << 20;

pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hex(data: &[u8]) -> String {
    data.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn entropy(block: &[u8]) -> f32 {
    if block.is_empty() {
        return 0.0;
    }
    let mut counts = [0u32; 256];
    for b in block {
        counts[*b as usize] += 1;
    }
    let n = block.len() as f32;
    counts
        .iter()
        .filter(|c| **c > 0)
        .map(|c| {
            let p = *c as f32 / n;
            -p * p.log2()
        })
        .sum()
}

fn container(data: &[u8]) -> Container {
    if (data.starts_with(b"PK\x03\x04") || data.starts_with(b"PK\x05\x06"))
        && let Ok(mut zip) = zip::ZipArchive::new(Cursor::new(data))
    {
        let entries = (0..zip.len())
            .filter_map(|i| {
                let f = zip.by_index(i).ok()?;
                Some(ZipEntry {
                    name: f
                        .name()
                        .map(|n| n.into_owned())
                        .unwrap_or_else(|_| format!("entry-{i}")),
                    size: f.size(),
                    compressed_size: f.compressed_size(),
                })
            })
            .collect();
        return Container::Zip { entries };
    }
    if data.starts_with(b"SQLite format 3\0") {
        return Container::Sqlite;
    }
    if data.starts_with(&[0x1f, 0x8b]) {
        return Container::Gzip;
    }
    let text_start = data
        .strip_prefix(&[0xef, 0xbb, 0xbf])
        .unwrap_or(data)
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .map(|i| &data[i..])
        .unwrap_or(&[]);
    if text_start.starts_with(b"<?xml")
        || text_start.starts_with(b"<") && data.len() > 1 && data.is_ascii()
    {
        return Container::Xml;
    }
    if (text_start.starts_with(b"{") || text_start.starts_with(b"["))
        && serde_json::from_slice::<serde_json::Value>(data).is_ok()
    {
        return Container::Json;
    }
    Container::Unknown
}

/// Try to inflate a zlib stream starting at `offset`. Returns (consumed, inflated).
pub fn inflate_at(data: &[u8], offset: usize) -> Option<(usize, Vec<u8>)> {
    let input = &data[offset..];
    if input.len() < 2 {
        return None;
    }
    let (cmf, flg) = (input[0], input[1]);
    // zlib header: deflate method, window <= 32K, and a valid check value.
    if cmf & 0x0f != 8
        || cmf >> 4 > 7
        || (u16::from(cmf) << 8 | u16::from(flg)) % 31 != 0
        || flg & 0x20 != 0
    {
        return None;
    }
    let mut decoder = flate2::bufread::ZlibDecoder::new(input);
    let mut out = Vec::new();
    decoder
        .by_ref()
        .take(MAX_INFLATE)
        .read_to_end(&mut out)
        .ok()?;
    let consumed = decoder.total_in() as usize;
    (consumed > 2 && !out.is_empty()).then_some((consumed, out))
}

/// Every zlib stream in the file, scanning byte by byte and skipping over
/// streams once found.
pub fn find_zlib_streams(data: &[u8]) -> Vec<(usize, usize, Vec<u8>)> {
    let mut found = Vec::new();
    let mut i = 0;
    while i + 2 <= data.len() {
        if let Some((consumed, out)) = inflate_at(data, i)
            && out.len() >= 8
        {
            found.push((i, consumed, out));
            i += consumed;
            continue;
        }
        i += 1;
    }
    found
}

fn ascii_strings(data: &[u8]) -> Vec<FoundString> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, b) in data.iter().chain(std::iter::once(&0)).enumerate() {
        let printable = b.is_ascii_graphic() || *b == b' ';
        match (printable, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                if i - s >= MIN_STRING && out.len() < MAX_STRINGS {
                    out.push(FoundString {
                        offset: s,
                        text: String::from_utf8_lossy(&data[s..i]).into_owned(),
                    });
                }
                start = None;
            }
            _ => {}
        }
    }
    out
}

fn utf16_strings(data: &[u8]) -> Vec<FoundString> {
    let mut out = Vec::new();
    for parity in 0..2 {
        let mut i = parity;
        let mut run: Vec<u16> = Vec::new();
        let mut start = i;
        while i + 1 < data.len() {
            let c = u16::from_le_bytes([data[i], data[i + 1]]);
            let ok = (0x20..0x7f).contains(&c);
            if ok {
                if run.is_empty() {
                    start = i;
                }
                run.push(c);
            } else {
                if run.len() >= MIN_STRING && out.len() < MAX_STRINGS {
                    out.push(FoundString {
                        offset: start,
                        text: String::from_utf16_lossy(&run),
                    });
                }
                run.clear();
            }
            i += 2;
        }
        if run.len() >= MIN_STRING && out.len() < MAX_STRINGS {
            out.push(FoundString {
                offset: start,
                text: String::from_utf16_lossy(&run),
            });
        }
    }
    out.sort_by_key(|s| s.offset);
    out
}

pub fn probe(data: &[u8]) -> ProbeReport {
    let container = container(data);
    let zlib_streams = if matches!(container, Container::Unknown) {
        find_zlib_streams(data)
            .into_iter()
            .map(|(offset, consumed, out)| Stream {
                offset,
                compressed_len: consumed,
                inflated_len: out.len(),
                inflated_sha256: sha256_hex(&out),
                inflated_head: hex(&out[..out.len().min(32)]),
            })
            .collect()
    } else {
        Vec::new()
    };
    ProbeReport {
        size: data.len(),
        sha256: sha256_hex(data),
        head: hex(&data[..data.len().min(32)]),
        container,
        entropy: data.chunks(ENTROPY_BLOCK).map(entropy).collect(),
        zlib_streams,
        ascii_strings: ascii_strings(data),
        utf16_strings: utf16_strings(data),
    }
}

/// The payloads worth analysing: ZIP entries, inflated zlib streams, or the
/// file itself. Returned as (name, bytes).
pub fn payloads(data: &[u8]) -> Vec<(String, Vec<u8>)> {
    match container(data) {
        Container::Zip { .. } => {
            let mut out = Vec::new();
            if let Ok(mut zip) = zip::ZipArchive::new(Cursor::new(data)) {
                for i in 0..zip.len() {
                    if let Ok(mut f) = zip.by_index(i) {
                        let mut buf = Vec::new();
                        if f.is_file() && f.read_to_end(&mut buf).is_ok() {
                            out.push((
                                f.name()
                                    .map(|n| n.into_owned())
                                    .unwrap_or_else(|_| format!("entry-{i}")),
                                buf,
                            ));
                        }
                    }
                }
            }
            out
        }
        Container::Gzip => {
            let mut buf = Vec::new();
            let ok = flate2::read::GzDecoder::new(data)
                .take(MAX_INFLATE)
                .read_to_end(&mut buf)
                .is_ok();
            if ok {
                vec![("gzip-payload".into(), buf)]
            } else {
                vec![("file".into(), data.to_vec())]
            }
        }
        Container::Unknown => {
            let streams = find_zlib_streams(data);
            let mut out: Vec<(String, Vec<u8>)> = streams
                .into_iter()
                .map(|(offset, _, bytes)| (format!("zlib@{offset:08x}"), bytes))
                .collect();
            out.insert(0, ("file".into(), data.to_vec()));
            out
        }
        _ => vec![("file".into(), data.to_vec())],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn finds_embedded_zlib_streams() {
        let payload = b"HouseCraft wall record, length 3000mm, layer Walls".repeat(10);
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&payload).unwrap();
        let compressed = enc.finish().unwrap();
        let mut file = b"HDR\x00\x01\x02\x03 some header".to_vec();
        let offset = file.len();
        file.extend_from_slice(&compressed);
        file.extend_from_slice(b"\x00\x00TRAILER");
        let r = probe(&file);
        assert!(matches!(r.container, Container::Unknown));
        assert_eq!(r.zlib_streams.len(), 1);
        assert_eq!(r.zlib_streams[0].offset, offset);
        assert_eq!(r.zlib_streams[0].inflated_len, payload.len());
        let p = payloads(&file);
        assert_eq!(p[1].1, payload);
    }

    #[test]
    fn identifies_zip_and_strings() {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            z.start_file("plan.xml", zip::write::SimpleFileOptions::default())
                .unwrap();
            z.write_all(b"<plan/>").unwrap();
            z.finish().unwrap();
        }
        let data = buf.into_inner();
        let r = probe(&data);
        let Container::Zip { entries } = r.container else {
            panic!("not detected as zip")
        };
        assert_eq!(entries[0].name, "plan.xml");
        let utf16: Vec<u8> = "Exterior Wall"
            .encode_utf16()
            .flat_map(|c| c.to_le_bytes())
            .collect();
        assert!(
            probe(&utf16)
                .utf16_strings
                .iter()
                .any(|s| s.text == "Exterior Wall")
        );
    }

    #[test]
    fn entropy_separates_text_from_noise() {
        assert!(entropy(&[0u8; 4096]) < 0.01);
        let noise: Vec<u8> = (0..4096u32)
            .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
            .collect();
        assert!(entropy(&noise) > 7.5);
    }
}
