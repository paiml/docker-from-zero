use crate::image::Image;
use std::collections::BTreeSet;

#[derive(Debug, Default, PartialEq)]
pub struct Delta {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

/// Final visible file set of an image: layers applied in order, honouring OCI whiteouts
/// (`.wh.<name>` deletes `<name>`; `.wh..wh..opq` is not supported here and is kept as a file).
pub fn visible(img: &Image) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for l in &img.layers {
        for f in &l.files {
            let (dir, base) = match f.rsplit_once('/') {
                Some((d, b)) => (format!("{d}/"), b),
                None => (String::new(), f.as_str()),
            };
            if let Some(target) = base.strip_prefix(".wh.") {
                set.remove(&format!("{dir}{target}"));
            } else {
                set.insert(f.clone());
            }
        }
    }
    set
}

pub fn diff(a: &Image, b: &Image) -> Delta {
    let (va, vb) = (visible(a), visible(b));
    Delta {
        added: vb.difference(&va).cloned().collect(),
        removed: va.difference(&vb).cloned().collect(),
    }
}
