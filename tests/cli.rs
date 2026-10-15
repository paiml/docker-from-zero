use imgscope::{diff, image, run};
use std::io::Write;

fn tar_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut b = tar::Builder::new(Vec::new());
    for (n, d) in files {
        let mut h = tar::Header::new_gnu();
        h.set_size(d.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        b.append_data(&mut h, n, *d).unwrap();
    }
    b.into_inner().unwrap()
}

fn gz(d: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    e.write_all(d).unwrap();
    e.finish().unwrap()
}

/// Build an image tarball; `lie` corrupts the recorded diff_id; `gzip` compresses layers.
fn image_tar(layers: &[Vec<u8>], lie: bool, gzip: bool) -> Vec<u8> {
    let ids: Vec<String> = layers
        .iter()
        .enumerate()
        .map(|(i, l)| {
            if lie && i == 0 {
                "sha256:00".to_string()
            } else {
                image::sha256_hex(l)
            }
        })
        .collect();
    let cfg = serde_json::json!({"rootfs": {"type": "layers", "diff_ids": ids}}).to_string();
    let names: Vec<String> = (0..layers.len())
        .map(|i| format!("l{i}/layer.tar"))
        .collect();
    let man = serde_json::json!([{"Config": "cfg.json", "Layers": names}]).to_string();
    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("manifest.json".into(), man.into_bytes()),
        ("cfg.json".into(), cfg.into_bytes()),
    ];
    for (n, l) in names.iter().zip(layers) {
        files.push((n.clone(), if gzip { gz(l) } else { l.clone() }));
    }
    let refs: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(n, d)| (n.as_str(), d.as_slice()))
        .collect();
    tar_of(&refs)
}

fn base() -> Vec<Vec<u8>> {
    vec![
        tar_of(&[("etc/os", b"x"), ("bin/sh", b"s")]),
        tar_of(&[("app/main", b"m"), ("bin/.wh.sh", b"")]),
    ]
}

fn go(args: &[&str]) -> (i32, String, String) {
    let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (mut o, mut e) = (Vec::new(), Vec::new());
    let c = run(&a, &mut o, &mut e);
    (
        c,
        String::from_utf8(o).unwrap(),
        String::from_utf8(e).unwrap(),
    )
}

fn tmp(name: &str, bytes: &[u8]) -> String {
    let p = std::env::temp_dir().join(format!("imgscope-{}-{name}", std::process::id()));
    std::fs::write(&p, bytes).unwrap();
    p.to_string_lossy().into_owned()
}

#[test]
fn parse_and_verify_plain_and_gzip() {
    for gzip in [false, true] {
        let img = image::parse(&image_tar(&base(), false, gzip)).unwrap();
        assert_eq!(img.layers.len(), 2);
        assert_eq!(img.layers[0].files, vec!["etc/os", "bin/sh"]);
        assert!(image::verify(&img).is_empty());
    }
}

#[test]
fn verify_detects_tampered_diff_id() {
    let img = image::parse(&image_tar(&base(), true, false)).unwrap();
    assert_eq!(image::verify(&img), vec![0]);
}

#[test]
fn whiteout_hides_file_and_diff_reports_it() {
    let one = image::parse(&image_tar(&base()[..1], false, false)).unwrap();
    let two = image::parse(&image_tar(&base(), false, false)).unwrap();
    let v = diff::visible(&two);
    assert!(v.contains("app/main") && !v.contains("bin/sh"));
    let d = diff::diff(&one, &two);
    assert_eq!(d.added, vec!["app/main"]);
    assert_eq!(d.removed, vec!["bin/sh"]);
}

#[test]
fn top_level_whiteout_and_plain_names() {
    let l = vec![tar_of(&[("a", b"1")]), tar_of(&[(".wh.a", b"")])];
    let img = image::parse(&image_tar(&l, false, false)).unwrap();
    assert!(diff::visible(&img).is_empty());
}

#[test]
fn parse_errors() {
    assert!(image::parse(b"not a tar").is_err());
    assert!(image::parse(&tar_of(&[("x", b"1")])).is_err());
    let bad = |m: &str, cfg: &str, extra: bool| {
        let mut f: Vec<(&str, &[u8])> = vec![("manifest.json", m.as_bytes())];
        if extra {
            f.push(("c.json", cfg.as_bytes()));
        }
        image::parse(&tar_of(&f))
    };
    assert!(bad("not json", "", false).is_err());
    assert!(bad("[]", "", false).is_err());
    assert!(bad("[{}]", "", false).is_err());
    assert!(bad(r#"[{"Config":"c.json"}]"#, "", false).is_err());
    assert!(bad(r#"[{"Config":"c.json"}]"#, "nope", true).is_err());
    assert!(bad(r#"[{"Config":"c.json"}]"#, "{}", true).is_err());
    assert!(bad(
        r#"[{"Config":"c.json"}]"#,
        r#"{"rootfs":{"diff_ids":[]}}"#,
        true
    )
    .is_err());
    assert!(bad(
        r#"[{"Config":"c.json","Layers":[1]}]"#,
        r#"{"rootfs":{"diff_ids":[]}}"#,
        true
    )
    .is_err());
    assert!(bad(
        r#"[{"Config":"c.json","Layers":["gone"]}]"#,
        r#"{"rootfs":{"diff_ids":[]}}"#,
        true
    )
    .is_err());
    // corrupt gzip layer and corrupt layer tar
    let mut f = vec![
        (
            "manifest.json",
            br#"[{"Config":"c.json","Layers":["l"]}]"#.to_vec(),
        ),
        ("c.json", br#"{"rootfs":{"diff_ids":[]}}"#.to_vec()),
        ("l", vec![0x1f, 0x8b, 1, 2, 3]),
    ];
    let r = |f: &Vec<(&str, Vec<u8>)>| {
        let refs: Vec<(&str, &[u8])> = f.iter().map(|(n, d)| (*n, d.as_slice())).collect();
        image::parse(&tar_of(&refs))
    };
    assert!(r(&f).is_err());
    f[2].1 = vec![7u8; 600];
    assert!(r(&f).is_err());
}

#[test]
fn cli_commands() {
    let ok = tmp("ok.tar", &image_tar(&base(), false, false));
    let lie = tmp("lie.tar", &image_tar(&base(), true, false));
    let one = tmp("one.tar", &image_tar(&base()[..1], false, false));
    let (c, o, _) = go(&["layers", &ok]);
    assert_eq!(c, 0);
    assert_eq!(o.lines().count(), 2);
    let (c, o, _) = go(&["verify", &ok]);
    assert_eq!(
        (c, o.trim()),
        (0, "contract: layer-digests-match-diff-ids OK")
    );
    let (c, o, _) = go(&["verify", &lie]);
    assert_eq!(c, 1);
    assert!(o.contains("MISMATCH layer 0") && o.contains("FAILED"));
    let (c, o, _) = go(&["diff", &one, &ok]);
    assert_eq!(c, 0);
    assert_eq!(
        o,
        "+ app/main\n- bin/sh\ncontract: whiteout-markers-never-visible OK\n"
    );
}

#[test]
fn cli_errors() {
    assert_eq!(go(&[]).0, 2);
    assert_eq!(go(&["layers"]).0, 2);
    let (c, _, e) = go(&["layers", "/nonexistent/x.tar"]);
    assert_eq!(c, 2);
    assert!(e.contains("nonexistent"));
    assert_eq!(go(&["verify", "/nonexistent/x.tar"]).0, 2);
    let ok = tmp("e.tar", &image_tar(&base(), false, false));
    assert_eq!(go(&["diff", &ok, "/nonexistent/x.tar"]).0, 2);
    assert_eq!(go(&["diff", "/nonexistent/x.tar", &ok]).0, 2);
}

/// Falsifier: a test that calls nothing must not count as coverage. Run by scripts/falsify.sh.
#[test]
#[ignore]
fn calls_nothing() {}
