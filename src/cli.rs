use crate::{diff, image};
use std::io::Write;

const USAGE: &str =
    "usage: imgscope layers <image.tar> | verify <image.tar> | diff <a.tar> <b.tar>";

fn load(path: &str) -> Result<image::Image, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    image::parse(&bytes)
}

/// Run the CLI. Returns the process exit code: 0 ok, 1 verification failed, 2 usage or read error.
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match a.as_slice() {
        ["layers", p] => load(p).map(|img| layers(&img, out)),
        ["verify", p] => load(p).map(|img| verify(&img, out)),
        ["diff", x, y] => load(x).and_then(|ia| load(y).map(|ib| compare(&ia, &ib, out))),
        _ => Err(USAGE.to_string()),
    };
    result.unwrap_or_else(|msg| {
        let _ = writeln!(err, "{msg}");
        2
    })
}

fn layers(img: &image::Image, out: &mut dyn Write) -> i32 {
    for (i, l) in img.layers.iter().enumerate() {
        let _ = writeln!(out, "{i} {} {} {}", l.digest, l.size, l.files.len());
    }
    0
}

fn verify(img: &image::Image, out: &mut dyn Write) -> i32 {
    let bad = image::verify(img);
    for i in &bad {
        let _ = writeln!(out, "MISMATCH layer {i}");
    }
    let _ = writeln!(
        out,
        "contract: layer-digests-match-diff-ids {}",
        status(bad.is_empty())
    );
    i32::from(!bad.is_empty())
}

fn compare(ia: &image::Image, ib: &image::Image, out: &mut dyn Write) -> i32 {
    let d = diff::diff(ia, ib);
    for f in &d.added {
        let _ = writeln!(out, "+ {f}");
    }
    for f in &d.removed {
        let _ = writeln!(out, "- {f}");
    }
    let leaked = diff::visible(ia)
        .iter()
        .chain(diff::visible(ib).iter())
        .any(|f| f.rsplit('/').next().is_some_and(|b| b.starts_with(".wh.")));
    let _ = writeln!(
        out,
        "contract: whiteout-markers-never-visible {}",
        status(!leaked)
    );
    0
}

fn status(ok: bool) -> &'static str {
    if ok {
        "OK"
    } else {
        "FAILED"
    }
}
