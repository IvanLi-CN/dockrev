use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

const PLACEHOLDER_INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Dockrev</title>
  </head>
  <body>
    <h1>Dockrev</h1>
    <p>Web UI assets are not built.</p>
    <p>Run <code>cd web &amp;&amp; npm ci &amp;&amp; npm run build</code> before building the server.</p>
  </body>
</html>
"#;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let dist_src = manifest_dir.join("../../web/dist");

    if dist_src.is_dir() {
        emit_rerun_for_dir(&dist_src);
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let dist_out = out_dir.join("dockrev-ui-dist");

    let _ = fs::remove_dir_all(&dist_out);
    fs::create_dir_all(&dist_out).expect("create dockrev-ui-dist");

    if dist_src.is_dir() {
        copy_dir(&dist_src, &dist_out).expect("copy web/dist into OUT_DIR");
    } else {
        fs::write(dist_out.join("index.html"), PLACEHOLDER_INDEX_HTML)
            .expect("write placeholder index.html");
    }
}

fn emit_rerun_for_dir(dir: &Path) {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

fn copy_dir(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir(&src_path, &dst_path)?;
        } else if ty.is_file() {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
