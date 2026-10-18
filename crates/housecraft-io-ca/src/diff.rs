//! Byte-level differences between two versions of a file.
//!
//! The main reverse-engineering loop: save a plan, change one thing in
//! Chief Architect (move a wall 1"), save again, and diff. The changed bytes
//! show where and how that property is stored.

use serde::Serialize;
use similar::{Algorithm, DiffOp, capture_diff_deadline};

use crate::probe::hex;

#[derive(Debug, Clone, Serialize)]
pub struct Hunk {
    pub a_offset: usize,
    pub a_len: usize,
    pub b_offset: usize,
    pub b_len: usize,
    /// Up to 64 bytes of each side, hex.
    pub a_bytes: String,
    pub b_bytes: String,
    /// If both sides are 4 or 8 bytes, the values read as little-endian numbers.
    pub as_numbers: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    pub a_len: usize,
    pub b_len: usize,
    pub identical: bool,
    pub hunks: Vec<Hunk>,
    /// True when the diff hit its time budget and merged the remainder.
    pub approximate: bool,
}

fn numbers(a: &[u8], b: &[u8]) -> Option<String> {
    match (a.len(), b.len()) {
        (4, 4) => {
            let (fa, fb) = (
                f32::from_le_bytes(a.try_into().ok()?),
                f32::from_le_bytes(b.try_into().ok()?),
            );
            let (ia, ib) = (
                i32::from_le_bytes(a.try_into().ok()?),
                i32::from_le_bytes(b.try_into().ok()?),
            );
            Some(format!("i32 {ia} -> {ib}; f32 {fa} -> {fb}"))
        }
        (8, 8) => {
            let (fa, fb) = (
                f64::from_le_bytes(a.try_into().ok()?),
                f64::from_le_bytes(b.try_into().ok()?),
            );
            let (ia, ib) = (
                i64::from_le_bytes(a.try_into().ok()?),
                i64::from_le_bytes(b.try_into().ok()?),
            );
            Some(format!("i64 {ia} -> {ib}; f64 {fa} -> {fb}"))
        }
        _ => None,
    }
}

/// Diff two byte strings, merging hunks separated by fewer than `merge_gap` equal bytes.
pub fn diff(a: &[u8], b: &[u8], merge_gap: usize) -> DiffReport {
    if a == b {
        return DiffReport {
            a_len: a.len(),
            b_len: b.len(),
            identical: true,
            hunks: Vec::new(),
            approximate: false,
        };
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let ops = capture_diff_deadline(
        Algorithm::Myers,
        a,
        0..a.len(),
        b,
        0..b.len(),
        Some(deadline),
    );
    let approximate = std::time::Instant::now() >= deadline;
    let mut hunks: Vec<(usize, usize, usize, usize)> = Vec::new();
    for op in ops {
        let (ao, al, bo, bl) = match op {
            DiffOp::Equal { .. } => continue,
            DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => (old_index, old_len, new_index, 0),
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => (old_index, 0, new_index, new_len),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => (old_index, old_len, new_index, new_len),
        };
        match hunks.last_mut() {
            Some(h) if ao <= h.0 + h.1 + merge_gap && bo <= h.2 + h.3 + merge_gap => {
                h.1 = ao + al - h.0;
                h.3 = bo + bl - h.2;
            }
            _ => hunks.push((ao, al, bo, bl)),
        }
    }
    DiffReport {
        a_len: a.len(),
        b_len: b.len(),
        identical: false,
        approximate,
        hunks: hunks
            .into_iter()
            .map(|(ao, al, bo, bl)| {
                let (sa, sb) = (&a[ao..ao + al], &b[bo..bo + bl]);
                Hunk {
                    a_offset: ao,
                    a_len: al,
                    b_offset: bo,
                    b_len: bl,
                    a_bytes: hex(&sa[..sa.len().min(64)]),
                    b_bytes: hex(&sb[..sb.len().min(64)]),
                    as_numbers: numbers(sa, sb),
                }
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_changed_double() {
        let mut a = b"WALL".to_vec();
        a.extend_from_slice(&120.0f64.to_le_bytes());
        a.extend_from_slice(b"END-OF-RECORD");
        let mut b = b"WALL".to_vec();
        b.extend_from_slice(&121.0f64.to_le_bytes());
        b.extend_from_slice(b"END-OF-RECORD");
        let r = diff(&a, &b, 0);
        assert!(!r.identical);
        assert_eq!(r.hunks.len(), 1);
        assert!(r.hunks[0].a_offset >= 4 && r.hunks[0].a_offset < 12);
        assert!(diff(&a, &a, 0).identical);
    }
}
