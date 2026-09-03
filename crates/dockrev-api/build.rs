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

const PLACEHOLDER_ROUTE_CONTRACT: &str = r#"{"version":1,"basePath":"/","dynamicSegmentPattern":"[A-Za-z0-9][A-Za-z0-9_-]{0,127}","staticPagePaths":["/","/queue","/queue/version-inference","/queue/ghcr-webhooks","/queue/ghcr-webhook-inbox","/settings/ghcr-webhooks","/services","/cleanup","/version-inference","/deploy-check","/settings","/settings/account","/settings/maintenance","/settings/backup","/settings/monitoring","/settings/schedules","/settings/release-notes","/settings/notifications","/settings/integrations"],"dynamicPageTemplates":["/queue/:jobId","/services/:stackId","/services/:stackId/:serviceId","/services/:stackId/:serviceId/overview","/services/:stackId/:serviceId/versions","/services/:stackId/:serviceId/history","/services/:stackId/:serviceId/monitoring","/services/:stackId/:serviceId/backup","/services/:stackId/:serviceId/logs","/services/:stackId/:serviceId/settings"],"reservedPrefixes":["/api","/supervisor","/assets"]}"#;

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
        let contract = dist_src.join(".dockrev-route-contract.json");
        let raw = fs::read_to_string(&contract).expect("web build route contract");
        validate_route_contract(&raw);
        fs::write(dist_out.join(".dockrev-route-contract.json"), raw).expect("copy route contract");
    } else {
        fs::write(dist_out.join("index.html"), PLACEHOLDER_INDEX_HTML)
            .expect("write placeholder index.html");
        validate_route_contract(PLACEHOLDER_ROUTE_CONTRACT);
        fs::write(
            dist_out.join(".dockrev-route-contract.json"),
            PLACEHOLDER_ROUTE_CONTRACT,
        )
        .expect("write placeholder route contract");
        fs::copy(&favicon_png_src, dist_out.join("favicon.png")).expect("copy favicon.png");
        if favicon_ico_src.is_file() {
            fs::copy(&favicon_ico_src, dist_out.join("favicon.ico")).expect("copy favicon.ico");
        }
    }
}

fn validate_route_contract(raw: &str) {
    let value: serde_json::Value = serde_json::from_str(raw).expect("valid route contract JSON");
    let object = value.as_object().expect("route contract object");

    assert_eq!(
        object.get("version").and_then(serde_json::Value::as_u64),
        Some(1),
        "route contract version"
    );

    let base_path = required_contract_string(object, "basePath");
    assert!(
        base_path.starts_with('/') && base_path.ends_with('/') && is_contract_path(base_path),
        "route contract basePath must be an absolute directory path"
    );

    let dynamic_segment_pattern = required_contract_string(object, "dynamicSegmentPattern");
    regex::Regex::new(&format!("^(?:{dynamic_segment_pattern})$"))
        .expect("route contract dynamicSegmentPattern");

    let static_paths = required_contract_paths(object, "staticPagePaths");
    assert!(
        static_paths.contains(&"/"),
        "route contract must include the root page"
    );
    assert!(
        static_paths.iter().all(|path| is_contract_path(path)),
        "route contract staticPagePaths"
    );

    let dynamic_templates = required_contract_paths(object, "dynamicPageTemplates");
    assert!(
        dynamic_templates.iter().all(|path| {
            is_contract_path(path)
                && path
                    .trim_matches('/')
                    .split('/')
                    .all(|segment| !segment.is_empty())
        }),
        "route contract dynamicPageTemplates"
    );

    let reserved_prefixes = required_contract_paths(object, "reservedPrefixes");
    assert!(
        reserved_prefixes
            .iter()
            .all(|path| is_contract_path(path) && *path != "/"),
        "route contract reservedPrefixes"
    );
}

fn required_contract_string<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> &'a str {
    object
        .get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("route contract {field}"))
}

fn required_contract_paths<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Vec<&'a str> {
    object
        .get(field)
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("route contract {field}"))
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|path| !path.is_empty())
                .unwrap_or_else(|| panic!("route contract {field}"))
        })
        .collect()
}

fn is_contract_path(path: &str) -> bool {
    path.starts_with('/') && !path.contains("//") && !path.split('/').any(|segment| segment == "..")
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
