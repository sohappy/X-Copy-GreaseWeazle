// SPDX-License-Identifier: GPL-3.0-or-later

//! Image file inspection: size, checksum, guessed format; ADFs are explored with
//! [`crate::fs::amigados::inspect_adf`].

use crate::disk::formats;
use crate::fs::amigados::{inspect_adf, AdfInfo};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImageReport {
    pub file_name: String,
    pub size: u64,
    pub extension: String,
    pub sha256: String,
    pub guessed_format: Option<&'static str>,
    pub is_flux: bool,
    pub adf: Option<Result<AdfInfo, String>>,
}

/// Inspect any image file.
pub fn inspect(path: &Path) -> Result<ImageReport, String> {
    let data = std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let extension = formats::extension(path).unwrap_or_default();
    let adf = (extension == "adf").then(|| inspect_adf(&data));
    Ok(ImageReport {
        file_name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        size: data.len() as u64,
        guessed_format: formats::guess_format(path, data.len() as u64),
        is_flux: formats::FLUX_SUFFIXES.contains(&extension.as_str()),
        extension,
        sha256: hex::encode(Sha256::digest(&data)),
        adf,
    })
}
