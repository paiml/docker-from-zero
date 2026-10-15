use flate2::read::GzDecoder;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;

/// One filesystem layer, uncompressed.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub path: String,
    pub size: usize,
    pub digest: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub layers: Vec<Layer>,
    pub diff_ids: Vec<String>,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let d = h.finalize();
    let mut s = String::from("sha256:");
    for b in d {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn entries(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    read_all(bytes).map_err(|e| e.to_string())
}

fn read_all(bytes: &[u8]) -> std::io::Result<Vec<(String, Vec<u8>)>> {
    let mut out = Vec::new();
    for e in tar::Archive::new(bytes).entries()? {
        let mut e = e?;
        let path = e.path()?.to_string_lossy().into_owned();
        let mut data = Vec::new();
        e.read_to_end(&mut data)?;
        out.push((path, data));
    }
    Ok(out)
}

fn gunzip_if_needed(data: Vec<u8>) -> Result<Vec<u8>, String> {
    if data.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        GzDecoder::new(&data[..])
            .read_to_end(&mut out)
            .map_err(|e| e.to_string())?;
        Ok(out)
    } else {
        Ok(data)
    }
}

/// Parse a `docker save` tarball (uses `manifest.json`, present in the legacy and OCI-layout forms).
pub fn parse(tarball: &[u8]) -> Result<Image, String> {
    let files: BTreeMap<String, Vec<u8>> = entries(tarball)?.into_iter().collect();
    let manifest = files.get("manifest.json").ok_or("missing manifest.json")?;
    let m: Value = serde_json::from_slice(manifest).map_err(|e| e.to_string())?;
    let first = m.get(0).ok_or("empty manifest.json")?;
    let cfg_path = first["Config"].as_str().ok_or("manifest has no Config")?;
    let cfg: Value = serde_json::from_slice(files.get(cfg_path).ok_or("missing config blob")?)
        .map_err(|e| e.to_string())?;
    let diff_ids: Vec<String> = cfg["rootfs"]["diff_ids"]
        .as_array()
        .ok_or("config has no rootfs.diff_ids")?
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    let layer_paths = first["Layers"].as_array().ok_or("manifest has no Layers")?;
    let mut layers = Vec::new();
    for p in layer_paths {
        let p = p.as_str().ok_or("non-string layer path")?;
        let raw = files.get(p).ok_or("missing layer blob")?.clone();
        let data = gunzip_if_needed(raw)?;
        let names = entries(&data)?.into_iter().map(|(n, _)| n).collect();
        layers.push(Layer {
            path: p.to_string(),
            size: data.len(),
            digest: sha256_hex(&data),
            files: names,
        });
    }
    Ok(Image { layers, diff_ids })
}

/// Layers whose recomputed digest differs from the config's diff_id at the same position.
pub fn verify(img: &Image) -> Vec<usize> {
    let mut bad = Vec::new();
    for (i, l) in img.layers.iter().enumerate() {
        if img.diff_ids.get(i) != Some(&l.digest) {
            bad.push(i);
        }
    }
    bad
}
