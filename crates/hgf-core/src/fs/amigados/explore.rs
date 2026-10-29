// SPDX-License-Identifier: GPL-3.0-or-later

//! Read-only AmigaDOS explorer: boot block, root block and root directory of an ADF.

use super::{bcpl_name, boot_checksum_ok, header_checksum_ok};
use crate::fs::bytes::be32;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdfEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u32,
    pub block: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdfInfo {
    pub filesystem: String,
    pub bootable: bool,
    pub boot_checksum_ok: bool,
    pub root_block: u32,
    /// Block type 2 (header) and secondary type 1 (root).
    pub root_valid: bool,
    pub root_checksum_ok: bool,
    pub volume: String,
    pub entries: Vec<AdfEntry>,
}

/// Read-only AmigaDOS explorer: boot block, root block and root directory.
pub fn inspect_adf(data: &[u8]) -> Result<AdfInfo, String> {
    if data.len() != 901_120 && data.len() != 1_802_240 {
        return Err(format!("Not a standard 880K/1.76M ADF ({} bytes)", data.len()));
    }
    if &data[0..3] != b"DOS" {
        return Err("No AmigaDOS boot block (NDOS or non-DOS disk)".into());
    }
    let filesystem = match data[3] {
        0 => "OFS",
        1 => "FFS",
        2 => "OFS International",
        3 => "FFS International",
        4 => "OFS DirCache",
        5 => "FFS DirCache",
        6 => "OFS Long names",
        7 => "FFS Long names",
        _ => "Unknown DOS type",
    }
    .to_string();
    let boot_checksum_ok = boot_checksum_ok(data);
    // A valid checksum is what makes Kickstart run the boot code.
    let bootable = boot_checksum_ok && data[12..1024].iter().any(|&b| b != 0);
    let root_block = (data.len() / 512 / 2) as u32;
    let root = &data[root_block as usize * 512..][..512];
    let root_valid = be32(root, 0) == 2 && be32(root, 508) == 1;
    let root_checksum_ok = root_valid && header_checksum_ok(root);
    let volume = bcpl_name(root, 432).unwrap_or_else(|| "<unknown>".into());

    // Walk the root hash table (72 entries for 512-byte blocks) and its hash chains.
    let mut entries = Vec::new();
    let nblocks = (data.len() / 512) as u32;
    for i in 0..72 {
        let mut blk = be32(root, 24 + i * 4);
        let mut guard = 0;
        while blk != 0 && blk < nblocks && guard < 1000 {
            guard += 1;
            let b = &data[blk as usize * 512..][..512];
            if be32(b, 0) != 2 {
                break;
            }
            let sec = be32(b, 508) as i32;
            if let Some(name) = bcpl_name(b, 432) {
                entries.push(AdfEntry {
                    name,
                    is_dir: sec == 2,
                    size: if sec == -3 { be32(b, 324) } else { 0 },
                    block: blk,
                });
            }
            blk = be32(b, 496);
        }
    }
    entries.sort_by_key(|e| (!e.is_dir, e.name.to_lowercase()));
    Ok(AdfInfo {
        filesystem,
        bootable,
        boot_checksum_ok,
        root_block,
        root_valid,
        root_checksum_ok,
        volume,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::amigados::set_header_checksum;
    use crate::fs::bytes::put_be32 as set32;

    #[test]
    fn explores_synthetic_adf() {
        let mut d = vec![0u8; 901_120];
        d[0..4].copy_from_slice(b"DOS\x01");
        let root = 880 * 512;
        set32(&mut d, root, 2);
        set32(&mut d, root + 508, 1);
        d[root + 432] = 4;
        d[root + 433..root + 437].copy_from_slice(b"Test");
        // One file header in block 882, hashed at slot 5.
        set32(&mut d, root + 24 + 5 * 4, 882);
        let f = 882 * 512;
        set32(&mut d, f, 2);
        set32(&mut d, f + 508, (-3i32) as u32);
        set32(&mut d, f + 324, 1234);
        d[f + 432] = 6;
        d[f + 433..f + 439].copy_from_slice(b"readme");
        set_header_checksum(&mut d[root..root + 512], 20);
        let info = inspect_adf(&d).unwrap();
        assert_eq!(info.filesystem, "FFS");
        assert_eq!(info.volume, "Test");
        assert!(info.root_checksum_ok);
        assert!(!info.boot_checksum_ok);
        assert_eq!(
            info.entries,
            vec![AdfEntry { name: "readme".into(), is_dir: false, size: 1234, block: 882 }]
        );
    }

    #[test]
    fn empty_root_block_is_not_valid() {
        let mut d = vec![0u8; 901_120];
        d[0..4].copy_from_slice(b"DOS\x00");
        let info = inspect_adf(&d).unwrap();
        assert!(!info.root_valid);
        assert!(!info.root_checksum_ok);
    }

    #[test]
    fn rejects_non_adf() {
        assert!(inspect_adf(&[0u8; 100]).is_err());
        assert!(inspect_adf(&vec![0u8; 901_120]).is_err());
    }
}
