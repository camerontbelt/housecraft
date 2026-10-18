//! Chief Architect undo snapshots (`%TEMP%\\undo_<project>_<pid>_<n>.undoplan`).
//!
//! Each holds the plan as it was just before the named edit, in the same
//! object encoding as a `.plan`'s data region (see `docs/formats/plan.md` 2a).
//! The X18 trial writes them even though it cannot save, which makes them the
//! controlled "change one thing" samples the format work needs.

use serde::Serialize;

pub const MAGIC: &[u8] = b"Chief Architect Undo Archive";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UndoHeader {
    /// File format version, the same number a `.plan` of that release carries.
    pub version: u16,
    /// The edit this snapshot undoes, e.g. "EditBehavior: Create Wall".
    pub action: String,
    /// Where the body (the plan state) starts.
    pub body_offset: usize,
}

fn u32_at(b: &[u8], o: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?) as usize)
}

/// Parse the header, or `None` if `bytes` is not an undo snapshot.
pub fn header(bytes: &[u8]) -> Option<UndoHeader> {
    let n = u32_at(bytes, 0)?;
    if bytes.get(4..4 + n)? != MAGIC {
        return None;
    }
    let o = 4 + n;
    // One zero byte, the version, then two zero bytes.
    let version = u16::from_le_bytes(bytes.get(o + 1..o + 3)?.try_into().ok()?);
    let len = u32_at(bytes, o + 5)?;
    if len > 4096 {
        return None;
    }
    let start = o + 9;
    let action = String::from_utf8_lossy(bytes.get(start..start + len)?).into_owned();
    Some(UndoHeader {
        version,
        action,
        body_offset: start + len,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_snapshot_header() {
        let mut b = Vec::new();
        b.extend_from_slice(&(MAGIC.len() as u32).to_le_bytes());
        b.extend_from_slice(MAGIC);
        b.push(0);
        b.extend_from_slice(&0x101Au16.to_le_bytes());
        b.extend_from_slice(&[0, 0]);
        let action = b"EditBehavior: Create Wall";
        b.extend_from_slice(&(action.len() as u32).to_le_bytes());
        b.extend_from_slice(action);
        b.extend_from_slice(&[1, 2, 3]);
        let h = header(&b).unwrap();
        assert_eq!(h.version, 0x101A);
        assert_eq!(h.action, "EditBehavior: Create Wall");
        assert_eq!(&b[h.body_offset..], &[1, 2, 3]);
        assert!(header(b"01 CA not an undo file").is_none());
    }
}
