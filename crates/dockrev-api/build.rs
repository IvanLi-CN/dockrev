use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

const PLACEHOLDER_INDEX_HTML: &str = r#"<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Dockrev</title>
    <link rel="icon" type="image/png" href="/favicon.png" />
    <link rel="icon" href="/favicon.ico" sizes="any" />
    <style>
      :root { color-scheme: light dark; }
      body { font-family: ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial, "Noto Sans", "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif; margin: 0; padding: 24px; line-height: 1.45; }
      .card { max-width: 860px; margin: 0 auto; padding: 20px 18px; border: 1px solid rgba(127,127,127,.35); border-radius: 14px; background: rgba(127,127,127,.06); }
      .brand { display: flex; align-items: center; gap: 12px; margin-bottom: 12px; }
      .brand img { width: 28px; height: 28px; display: block; }
      h1 { margin: 0; font-size: 20px; }
      p { margin: 10px 0; }
      code, pre { font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace; }
      pre { padding: 12px; border-radius: 10px; overflow: auto; background: rgba(127,127,127,.12); }
      .muted { opacity: .85; font-size: 12px; }
    </style>
  </head>
  <body>
    <div class="card">
      <div class="brand">
        <img src="/favicon.png" alt="" aria-hidden="true" />
        <h1>Dockrev Web UI 未构建</h1>
      </div>
      <p class="muted">当前为后端 build.rs 的占位页面：没有找到 <code>web/dist</code>。</p>
      <p>请先构建前端资源，然后再构建/运行服务端：</p>
      <pre><code>cd web
bun install
bun run build</code></pre>
    </div>
  </body>
</html>
"#;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let dist_src = manifest_dir.join("../../web/dist");
    let favicon_png_src = manifest_dir.join("../../web/public/favicon.png");
    let favicon_ico_src = manifest_dir.join("../../web/public/favicon.ico");

    println!("cargo:rerun-if-changed={}", favicon_png_src.display());
    println!("cargo:rerun-if-changed={}", favicon_ico_src.display());

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
        fs::copy(&favicon_png_src, dist_out.join("favicon.png")).expect("copy favicon.png");
        if favicon_ico_src.is_file() {
            fs::copy(&favicon_ico_src, dist_out.join("favicon.ico")).expect("copy favicon.ico");
        }
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
