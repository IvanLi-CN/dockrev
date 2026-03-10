use super::meta::{SupervisorMeta, trimmed_non_empty};

fn escape_html_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn escape_html_attr(input: &str) -> String {
    escape_html_text(input)
}

const THEME_STORAGE_KEY: &str = "dockrev:theme";

pub(crate) fn render_ui(base_path: &str, meta: &SupervisorMeta) -> String {
    let version_html = if let Some(release_url) = trimmed_non_empty(meta.release_url.as_deref()) {
        format!(
            r#"<a href="{url}" target="_blank" rel="noopener noreferrer"><code>{value}</code></a>"#,
            url = escape_html_attr(release_url),
            value = escape_html_text(&meta.version)
        )
    } else {
        format!("<code>{}</code>", escape_html_text(&meta.version))
    };
    let repository_html = format!(
        r#"<a href="{url}" target="_blank" rel="noopener noreferrer">{value}</a>"#,
        url = escape_html_attr(&meta.repository),
        value = escape_html_text(&meta.repository)
    );
    let developer_html = format!(
        r#"<a href="{url}" target="_blank" rel="noopener noreferrer">{value}</a>"#,
        url = escape_html_attr(&meta.developer_url),
        value = escape_html_text(&meta.developer_name)
    );

    // Minimal, dependency-free console. Uses same-origin fetch under base_path.
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Dockrev Supervisor</title>
    <link rel="icon" type="image/png" href="{base_path}/favicon.png" />
    <script>
      (() => {{
        const STORAGE_KEY = {theme_storage_key_json};
        const root = document.documentElement;
        const media = window.matchMedia('(prefers-color-scheme: dark)');

        function normalizeTheme(value) {{
          return value === 'light' || value === 'dark' ? value : null;
        }}

        function preferredTheme() {{
          return media.matches ? 'dark' : 'light';
        }}

        function readStoredTheme() {{
          try {{
            return normalizeTheme(window.localStorage.getItem(STORAGE_KEY));
          }} catch (_error) {{
            return null;
          }}
        }}

        function applyTheme(theme) {{
          root.dataset.theme = theme;
          root.style.colorScheme = theme;
        }}

        function syncThemeFromPreference() {{
          const storedTheme = readStoredTheme();
          const theme = storedTheme || preferredTheme();
          applyTheme(theme);
          return storedTheme;
        }}

        window.__dockrevSupervisorTheme = {{
          storageKey: STORAGE_KEY,
          mediaQuery: media,
          applyTheme,
          syncThemeFromPreference,
          hasStoredTheme() {{
            return !!readStoredTheme();
          }},
        }};

        syncThemeFromPreference();
      }})();
    </script>
    <style>
      :root {{
        color-scheme: dark;
        --bg: #070b10;
        --bg-layered:
          linear-gradient(rgba(255, 255, 255, 0.025) 1px, transparent 1px),
          linear-gradient(90deg, rgba(255, 255, 255, 0.025) 1px, transparent 1px),
          radial-gradient(72% 80% at 10% 0%, rgba(79, 209, 255, 0.18) 0%, rgba(79, 209, 255, 0) 60%),
          radial-gradient(56% 64% at 100% 0%, rgba(255, 157, 106, 0.14) 0%, rgba(255, 157, 106, 0) 50%),
          linear-gradient(180deg, #05080c 0%, #0a1118 42%, #0d151d 100%);
        --text: rgba(238, 244, 249, 0.96);
        --muted: rgba(193, 205, 217, 0.78);
        --panel: rgba(10, 17, 24, 0.84);
        --panel-strong: rgba(12, 21, 30, 0.98);
        --panel-border: rgba(134, 154, 176, 0.22);
        --panel-shadow: 0 18px 42px rgba(0, 0, 0, 0.34);
        --surface: rgba(255, 255, 255, 0.035);
        --surface-strong: rgba(255, 255, 255, 0.055);
        --surface-accent: rgba(79, 209, 255, 0.11);
        --button-bg: rgba(255, 255, 255, 0.04);
        --button-hover: rgba(79, 209, 255, 0.12);
        --button-border: rgba(167, 189, 212, 0.2);
        --button-text: var(--text);
        --input-bg: rgba(255, 255, 255, 0.04);
        --input-border: rgba(167, 189, 212, 0.2);
        --input-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.015);
        --console-bg: rgba(4, 10, 15, 0.94);
        --pop-bg: rgba(9, 16, 22, 0.98);
        --code-bg: rgba(255, 255, 255, 0.08);
        --link: #9ae6ff;
        --link-hover: #c8f1ff;
        --accent: #4fd1ff;
        --accent-strong: #22b8f5;
        --accent-warm: #ff9d6a;
        --spinner-track: rgba(238, 244, 249, 0.2);
        --spinner-head: rgba(238, 244, 249, 0.88);
        --ok: #32c96c;
        --bad: #ff7b7b;
        --warn: #ffb155;
        --info: #53a9ff;
        --selection: rgba(79, 209, 255, 0.2);
      }}
      html[data-theme='light'] {{
        color-scheme: light;
        --bg: #f5f7fa;
        --bg-layered:
          linear-gradient(rgba(8, 23, 39, 0.035) 1px, transparent 1px),
          linear-gradient(90deg, rgba(8, 23, 39, 0.035) 1px, transparent 1px),
          radial-gradient(72% 80% at 10% 0%, rgba(79, 209, 255, 0.12) 0%, rgba(79, 209, 255, 0) 60%),
          radial-gradient(56% 64% at 100% 0%, rgba(255, 157, 106, 0.1) 0%, rgba(255, 157, 106, 0) 50%),
          linear-gradient(180deg, #fbfdff 0%, #f5f8fb 42%, #eef3f7 100%);
        --text: rgba(14, 24, 37, 0.96);
        --muted: rgba(71, 89, 110, 0.82);
        --panel: rgba(255, 255, 255, 0.88);
        --panel-strong: rgba(255, 255, 255, 0.98);
        --panel-border: rgba(12, 34, 58, 0.13);
        --panel-shadow: 0 18px 40px rgba(15, 23, 42, 0.08);
        --surface: rgba(12, 34, 58, 0.035);
        --surface-strong: rgba(12, 34, 58, 0.055);
        --surface-accent: rgba(34, 184, 245, 0.09);
        --button-bg: rgba(255, 255, 255, 0.86);
        --button-hover: rgba(34, 184, 245, 0.08);
        --button-border: rgba(12, 34, 58, 0.13);
        --button-text: rgba(14, 24, 37, 0.96);
        --input-bg: rgba(255, 255, 255, 0.92);
        --input-border: rgba(12, 34, 58, 0.14);
        --input-shadow: inset 0 0 0 1px rgba(34, 184, 245, 0.02);
        --console-bg: rgba(243, 247, 251, 0.98);
        --pop-bg: rgba(255, 255, 255, 0.98);
        --code-bg: rgba(12, 34, 58, 0.07);
        --link: #0f87bb;
        --link-hover: #0b668f;
        --accent: #22b8f5;
        --accent-strong: #0f87bb;
        --accent-warm: #d97745;
        --spinner-track: rgba(14, 24, 37, 0.18);
        --spinner-head: rgba(14, 24, 37, 0.8);
        --ok: #15803d;
        --bad: #c24141;
        --warn: #a16207;
        --info: #2563eb;
        --selection: rgba(34, 184, 245, 0.16);
      }}
      * {{ box-sizing: border-box; }}
      html {{ background: var(--bg); }}
      body {{
        font-family: 'IBM Plex Sans', 'Avenir Next', 'Segoe UI', 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', sans-serif;
        min-height: 100vh;
        margin: 0 auto;
        padding: 0 18px 44px;
        max-width: 1240px;
        background: var(--bg-layered);
        background-size: 24px 24px, 24px 24px, auto, auto, auto;
        background-attachment: fixed;
        color: var(--text);
      }}
      ::selection {{ background: var(--selection); }}
      a {{ color: var(--link); text-decoration: none; }}
      a:hover {{ color: var(--link-hover); text-decoration: underline; }}
      code {{
        background: var(--code-bg);
        padding: 2px 7px;
        border-radius: 999px;
        word-break: break-all;
      }}
      button,
      input,
      textarea,
      select {{ font: inherit; }}
      button {{
        min-height: 38px;
        padding: 8px 12px;
        border-radius: 12px;
        border: 1px solid var(--button-border);
        background: var(--button-bg);
        color: var(--button-text);
        cursor: pointer;
        transition: transform 160ms ease, background-color 160ms ease, border-color 160ms ease, box-shadow 160ms ease, opacity 160ms ease;
      }}
      button:hover:not([disabled]) {{
        background: var(--button-hover);
        border-color: rgba(79, 209, 255, 0.34);
        box-shadow: inset 0 0 0 1px rgba(79, 209, 255, 0.08);
        transform: translateY(-1px);
      }}
      button:focus-visible,
      input:focus-visible {{
        outline: 2px solid rgba(79, 209, 255, 0.38);
        outline-offset: 2px;
      }}
      button[disabled] {{
        opacity: 0.5;
        cursor: not-allowed;
        box-shadow: none;
        transform: none;
      }}
      button.primary {{
        background: linear-gradient(135deg, rgba(79, 209, 255, 0.22), rgba(255, 157, 106, 0.18));
        border-color: rgba(79, 209, 255, 0.42);
      }}
      html[data-theme='light'] button.primary {{
        background: linear-gradient(135deg, rgba(34, 184, 245, 0.16), rgba(217, 119, 69, 0.12));
        border-color: rgba(15, 135, 187, 0.24);
      }}
      button.btnRunning {{
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: 7px;
      }}
      button.btnRunning::before {{
        content: '';
        width: 11px;
        height: 11px;
        border-radius: 999px;
        border: 2px solid var(--spinner-track);
        border-top-color: var(--spinner-head);
        animation: supervisorButtonSpin 0.8s linear infinite;
      }}
      input {{
        width: 100%;
        min-width: 0;
        padding: 10px 12px;
        border-radius: 12px;
        border: 1px solid var(--input-border);
        background: var(--input-bg);
        color: var(--text);
        box-shadow: var(--input-shadow);
      }}
      pre {{
        margin: 0;
        padding: 12px 14px;
        border-radius: 16px;
        overflow: auto;
        white-space: pre-wrap;
        word-break: break-word;
        color: var(--text);
        line-height: 1.6;
        font-family: 'IBM Plex Mono', 'JetBrains Mono', 'SFMono-Regular', 'Menlo', monospace;
      }}
      .shell {{
        display: grid;
        gap: 18px;
        padding-top: 20px;
      }}
      .panel {{
        position: relative;
        border: 1px solid var(--panel-border);
        border-radius: 22px;
        padding: 18px;
        background: var(--panel);
        box-shadow: var(--panel-shadow);
        overflow: hidden;
      }}
      .panel::before {{
        content: '';
        position: absolute;
        inset: 0;
        pointer-events: none;
        background: linear-gradient(180deg, rgba(255, 255, 255, 0.03), transparent 28%);
      }}
      .panel > * {{
        position: relative;
        z-index: 1;
      }}
      .muted {{
        color: var(--muted);
        font-size: 12px;
        line-height: 1.5;
      }}
      .eyebrow,
      .sectionEyebrow {{
        color: var(--accent);
        font-size: 11px;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.14em;
      }}
      .sectionHeadingRow {{
        display: flex;
        align-items: flex-end;
        justify-content: space-between;
        gap: 14px;
        flex-wrap: wrap;
        margin-bottom: 14px;
      }}
      .sectionHeadingRow h2 {{
        margin: 6px 0 0;
        font-size: 20px;
        line-height: 1.05;
        letter-spacing: -0.03em;
      }}
      .sectionHeadingNote {{
        max-width: 40ch;
        text-align: right;
      }}
      .masthead {{
        padding-top: 22px;
      }}
      .mastheadGrid {{
        display: grid;
        grid-template-columns: minmax(0, 1.35fr) minmax(320px, 0.85fr);
        gap: 18px;
        align-items: start;
      }}
      .brandRow {{
        display: flex;
        gap: 14px;
        align-items: flex-start;
      }}
      .brandMark {{
        width: 44px;
        height: 44px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        border-radius: 14px;
        background: linear-gradient(145deg, rgba(79, 209, 255, 0.16), rgba(255, 157, 106, 0.08));
        border: 1px solid rgba(79, 209, 255, 0.22);
        flex: 0 0 auto;
      }}
      .brandMark img {{
        display: block;
        width: 24px;
        height: 24px;
      }}
      .masthead h1 {{
        margin: 6px 0 0;
        font-size: clamp(30px, 3.6vw, 44px);
        line-height: 0.95;
        letter-spacing: -0.05em;
      }}
      .intro {{
        max-width: 52ch;
        margin: 12px 0 0;
        color: var(--muted);
        font-size: 14px;
        line-height: 1.62;
      }}
      .mastheadAside {{
        display: grid;
        gap: 12px;
        align-content: start;
      }}
      .linkButton {{
        display: inline-flex;
        align-items: center;
        justify-content: center;
        min-height: 38px;
        padding: 8px 12px;
        border-radius: 12px;
        border: 1px solid var(--button-border);
        background: var(--surface);
        color: var(--link);
        justify-self: end;
      }}
      .linkButton:hover {{
        background: var(--button-hover);
        text-decoration: none;
      }}
      .metaStack {{
        display: grid;
        gap: 10px;
      }}
      .metaRow {{
        display: grid;
        grid-template-columns: 104px minmax(0, 1fr);
        gap: 12px;
        align-items: start;
        padding: 12px 14px;
        border-radius: 16px;
        border: 1px solid var(--panel-border);
        background: var(--surface);
      }}
      .metaLabel {{
        color: var(--muted);
        font-size: 11px;
        text-transform: uppercase;
        letter-spacing: 0.12em;
      }}
      .metaValue {{
        font-size: 13px;
        line-height: 1.5;
        word-break: break-word;
      }}
      .actionDeckShell {{
        display: grid;
        grid-template-columns: minmax(240px, 280px) minmax(0, 1fr);
        gap: 14px;
        align-items: start;
      }}
      .fieldBlock {{
        display: grid;
        gap: 8px;
        padding: 14px;
        border-radius: 18px;
        border: 1px solid var(--panel-border);
        background: var(--surface);
      }}
      .fieldLabel {{
        font-size: 11px;
        color: var(--muted);
        text-transform: uppercase;
        letter-spacing: 0.12em;
      }}
      .fieldHint {{
        font-size: 11px;
        color: var(--muted);
        line-height: 1.45;
      }}
      .actionRail {{
        display: grid;
        gap: 12px;
      }}
      .buttonRow {{
        display: flex;
        flex-wrap: wrap;
        gap: 10px;
        align-items: center;
      }}
      .buttonGroup {{
        display: flex;
        flex-wrap: wrap;
        gap: 10px;
        align-items: center;
      }}
      .buttonGroup > * {{
        flex: 0 0 auto;
      }}
      .buttonGroup-aux {{
        justify-content: flex-end;
      }}
      .actionNote {{
        padding: 12px 14px;
        border-radius: 16px;
        border: 1px dashed var(--panel-border);
        background: linear-gradient(135deg, var(--surface-accent), transparent 70%);
      }}
      .statusBoard {{
        display: grid;
        grid-template-columns: minmax(0, 1.18fr) minmax(320px, 0.82fr);
        gap: 14px;
      }}
      .statusHero,
      .statusFact,
      .progressCard,
      .artifactCard {{
        border: 1px solid var(--panel-border);
        border-radius: 18px;
        background: var(--surface);
      }}
      .statusHero {{
        padding: 18px;
        background: linear-gradient(145deg, rgba(79, 209, 255, 0.12), rgba(255, 157, 106, 0.08) 50%, transparent 100%);
      }}
      html[data-theme='light'] .statusHero {{
        background: linear-gradient(145deg, rgba(34, 184, 245, 0.1), rgba(217, 119, 69, 0.06) 50%, rgba(255, 255, 255, 0.65) 100%);
      }}
      .statusHeroTop {{
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 14px;
      }}
      .statusValue {{
        font-size: 15px;
        line-height: 1.38;
        font-weight: 600;
        word-break: break-word;
      }}
      .statusValue-lg {{
        margin-top: 8px;
        font-size: clamp(24px, 3vw, 34px);
        line-height: 0.95;
        letter-spacing: -0.05em;
      }}
      .statusLabel {{
        color: var(--muted);
        font-size: 11px;
        text-transform: uppercase;
        letter-spacing: 0.12em;
      }}
      .statusMeta {{
        color: var(--muted);
        font-size: 12px;
        line-height: 1.5;
      }}
      .statusHeroSummary {{
        margin-top: 14px;
        max-width: 40ch;
      }}
      .statusFacts {{
        display: grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: 12px;
      }}
      .statusFact {{
        padding: 14px;
        display: grid;
        gap: 8px;
        align-content: start;
      }}
      .statusFact-wide {{
        grid-column: 1 / -1;
      }}
      .statusTone {{
        display: inline-flex;
        align-items: center;
        justify-content: center;
        min-height: 30px;
        padding: 5px 10px;
        border-radius: 999px;
        border: 1px solid var(--panel-border);
        background: var(--surface-strong);
        font-size: 11px;
        text-transform: uppercase;
        letter-spacing: 0.12em;
        white-space: nowrap;
      }}
      .statusTone.state-running,
      .stateBadge-running {{
        color: var(--info);
        border-color: rgba(83, 169, 255, 0.32);
        background: rgba(83, 169, 255, 0.12);
      }}
      .statusTone.state-succeeded,
      .stateBadge-succeeded {{
        color: var(--ok);
        border-color: rgba(50, 201, 108, 0.28);
        background: rgba(50, 201, 108, 0.12);
      }}
      .statusTone.state-failed,
      .statusTone.state-rolled_back,
      .statusTone.state-offline,
      .stateBadge-failed,
      .stateBadge-rolled_back,
      .stateBadge-offline {{
        color: var(--bad);
        border-color: rgba(255, 123, 123, 0.28);
        background: rgba(255, 123, 123, 0.12);
      }}
      .statusTone.state-idle,
      .statusTone.state-unknown,
      .stateBadge-idle,
      .stateBadge-unknown {{
        color: var(--muted);
        border-color: var(--panel-border);
        background: var(--surface-strong);
      }}
      .progressCard {{
        grid-column: 1 / -1;
        padding: 14px;
        display: grid;
        gap: 8px;
      }}
      .artifactGrid {{
        grid-column: 1 / -1;
        display: grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: 12px;
      }}
      .artifactCard {{
        padding: 14px;
        display: grid;
        gap: 8px;
      }}
      .statusCode,
      .statusCodeInline {{
        background: var(--console-bg);
        border: 1px solid var(--panel-border);
        border-radius: 14px;
        padding: 10px 12px;
        color: var(--text);
        font-family: 'IBM Plex Mono', 'JetBrains Mono', 'SFMono-Regular', 'Menlo', monospace;
        font-size: 12px;
        line-height: 1.58;
        white-space: pre-wrap;
        word-break: break-word;
      }}
      .statusCodeInline {{
        min-height: 44px;
      }}
      .workspaceGrid {{
        display: grid;
        grid-template-columns: minmax(250px, 310px) minmax(0, 1fr);
        gap: 14px;
        align-items: stretch;
      }}
      .workspaceGrid.workspaceGrid-logsOnly {{
        grid-template-columns: minmax(0, 1fr);
      }}
      .historyRail,
      .logPanel {{
        min-height: 340px;
        border-radius: 18px;
        border: 1px solid var(--panel-border);
        background: var(--surface);
      }}
      .historyRail {{
        padding: 12px;
        max-height: min(560px, calc(100vh - 190px));
        overflow: auto;
        overscroll-behavior: contain;
      }}
      .historyList {{
        display: grid;
        gap: 10px;
      }}
      .historyCard {{
        width: 100%;
        min-height: 78px;
        padding: 12px;
        border-radius: 16px;
        border: 1px solid var(--panel-border);
        background: var(--panel-strong);
        color: var(--text);
        text-align: left;
        display: grid;
        gap: 10px;
      }}
      .historyCard.active {{
        border-color: rgba(79, 209, 255, 0.34);
        box-shadow: inset 3px 0 0 var(--accent), 0 0 0 1px rgba(79, 209, 255, 0.08);
        background: linear-gradient(180deg, rgba(79, 209, 255, 0.08), rgba(255, 255, 255, 0.02));
      }}
      html[data-theme='light'] .historyCard.active {{
        background: linear-gradient(180deg, rgba(34, 184, 245, 0.09), rgba(255, 255, 255, 0.65));
      }}
      .historyTop,
      .historyBottom {{
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 10px;
        flex-wrap: wrap;
      }}
      .historyTime {{
        font-size: 14px;
        font-weight: 600;
      }}
      .historyMeta,
      .historyTail {{
        color: var(--muted);
        font-size: 12px;
        line-height: 1.45;
      }}
      .historyBadges {{
        display: flex;
        flex-wrap: wrap;
        gap: 6px;
        justify-content: flex-end;
      }}
      .stateBadge {{
        display: inline-flex;
        align-items: center;
        gap: 5px;
        padding: 2px 7px;
        border-radius: 999px;
        border: 1px solid var(--panel-border);
        font-size: 10px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
      }}
      .stateDot {{
        width: 8px;
        height: 8px;
        border-radius: 999px;
        flex: 0 0 auto;
      }}
      .stateDot-running {{ background: var(--info); }}
      .stateDot-succeeded {{ background: var(--ok); }}
      .stateDot-failed,
      .stateDot-rolled_back,
      .stateDot-offline {{ background: var(--bad); }}
      .stateDot-idle,
      .stateDot-unknown {{ background: #7f93a8; }}
      .newBadge {{
        color: var(--warn);
        border: 1px solid rgba(255, 177, 85, 0.26);
        background: rgba(255, 177, 85, 0.14);
        border-radius: 999px;
        padding: 3px 8px;
        font-size: 10px;
      }}
      .logPanel {{
        display: grid;
        grid-template-rows: auto 1fr;
        padding: 14px;
      }}
      .logHeader {{
        display: flex;
        justify-content: space-between;
        align-items: flex-start;
        gap: 12px;
        flex-wrap: wrap;
        margin-bottom: 12px;
      }}
      .logTitle {{
        margin-top: 4px;
        font-size: 18px;
        line-height: 1.1;
        font-weight: 600;
        letter-spacing: -0.02em;
      }}
      .logSummary {{
        max-width: 340px;
        text-align: right;
      }}
      #logs {{
        height: 100%;
        min-height: 250px;
        background: var(--console-bg);
        border: 1px solid var(--panel-border);
      }}
      .popWrap {{
        position: relative;
        display: inline-flex;
        align-items: center;
      }}
      .popCard {{
        position: absolute;
        top: calc(100% + 10px);
        right: 0;
        left: auto;
        width: min(320px, calc(100vw - 48px));
        border: 1px solid var(--button-border);
        border-radius: 16px;
        background: var(--pop-bg);
        box-shadow: var(--panel-shadow);
        padding: 12px;
        z-index: 20;
      }}
      .popTitle {{
        margin: 0 0 6px;
        font-size: 13px;
        font-weight: 700;
      }}
      .popActions {{
        display: flex;
        gap: 8px;
        justify-content: flex-end;
        margin-top: 12px;
      }}
      .danger {{
        border-color: rgba(220, 38, 38, 0.84);
        background: rgba(220, 38, 38, 0.92);
        color: #fff;
      }}
      @keyframes supervisorButtonSpin {{
        from {{ transform: rotate(0deg); }}
        to {{ transform: rotate(360deg); }}
      }}
      @media (prefers-reduced-motion: reduce) {{
        button,
        .linkButton {{ transition: none; }}
        button.btnRunning::before {{ animation: none; }}
      }}
      @media (max-width: 1024px) {{
        body {{
          padding-left: 16px;
          padding-right: 16px;
        }}
        .panel {{
          padding: 16px;
        }}
        .mastheadGrid,
        .actionDeckShell,
        .statusBoard,
        .workspaceGrid {{
          grid-template-columns: 1fr;
        }}
        .statusFacts {{
          grid-template-columns: repeat(3, minmax(0, 1fr));
        }}
        .artifactGrid {{
          grid-template-columns: 1fr;
        }}
        .linkButton {{
          justify-self: start;
        }}
        .historyRail,
        .logPanel {{
          min-height: auto;
        }}
        .historyRail {{
          max-height: min(360px, 44vh);
        }}
        .logSummary,
        .sectionHeadingNote {{
          text-align: left;
          max-width: none;
        }}
      }}
      @media (max-width: 720px) {{
        body {{
          padding-left: 12px;
          padding-right: 12px;
        }}
        .shell {{
          gap: 14px;
          padding-top: 14px;
        }}
        .panel {{
          padding: 14px;
          border-radius: 18px;
        }}
        .brandRow,
        .sectionHeadingRow,
        .statusHeroTop,
        .historyTop,
        .historyBottom,
        .logHeader {{
          align-items: flex-start;
        }}
        .brandRow,
        .metaRow {{
          grid-template-columns: 1fr;
        }}
        .brandRow {{
          flex-direction: column;
        }}
        .statusFacts {{
          grid-template-columns: 1fr;
        }}
        .buttonGroup,
        .buttonGroup-aux {{
          width: 100%;
          justify-content: flex-start;
        }}
        .buttonGroup > *,
        .buttonGroup-aux > *,
        .linkButton,
        .popWrap,
        .popWrap > button {{
          width: 100%;
        }}
        .logTitle {{
          font-size: 16px;
        }}
        .popCard {{
          left: 0;
          right: auto;
          width: min(320px, calc(100vw - 36px));
        }}
      }}
    </style>
  </head>
  <body>
    <main class="shell">
      <section class="panel masthead" data-panel="masthead">
        <div class="mastheadGrid">
          <div>
            <div class="eyebrow">Supervisor Console</div>
            <div class="brandRow">
              <div class="brandMark">
                <img src="{base_path}/favicon.png" alt="" aria-hidden="true" width="26" height="26" />
              </div>
              <div>
                <h1>Dockrev 自我升级（Supervisor）</h1>
                <p class="intro">该页面独立于 Dockrev 生命周期；Dockrev 重启期间仍可用，并保留最近 operation 上下文供排障。</p>
              </div>
            </div>
          </div>
          <div class="mastheadAside">
            <a class="linkButton" href="/">返回 Dockrev</a>
            <div class="metaStack">
              <div class="metaRow">
                <div class="metaLabel">Supervisor 版本</div>
                <div class="metaValue">{version_html}</div>
              </div>
              <div class="metaRow">
                <div class="metaLabel">开源仓库</div>
                <div class="metaValue">{repository_html}</div>
              </div>
              <div class="metaRow">
                <div class="metaLabel">开发者</div>
                <div class="metaValue">{developer_html}</div>
              </div>
            </div>
          </div>
        </div>
      </section>

      <section class="panel actionDeck" data-panel="action-deck">
        <div class="sectionHeadingRow">
          <div>
            <div class="sectionEyebrow">Action deck</div>
            <h2>升级控制台</h2>
          </div>
          <div class="muted sectionHeadingNote">失败会尝试回滚到 previous digest（如可用）；操作过程会持续留在当前页。</div>
        </div>
        <div class="actionDeckShell">
          <label class="fieldBlock" for="tag">
            <span class="fieldLabel">Target tag</span>
            <input id="tag" value="latest" />
            <span class="fieldHint">默认使用 <code>latest</code>，也支持输入固定 tag 进行验证或升级。</span>
          </label>
          <div class="actionRail">
            <div class="buttonGroup buttonGroup-main">
              <button id="dry">预览（dry-run）</button>
              <button id="apply" class="primary">开始升级（apply）</button>
            </div>
            <div class="buttonGroup buttonGroup-aux">
              <div id="rollbackWrap" class="popWrap">
                <button id="rollback" aria-haspopup="dialog" aria-expanded="false">回滚</button>
                <div id="rollbackPop" class="popCard" role="dialog" aria-modal="false" hidden>
                  <div class="popTitle">确认手动回滚？</div>
                  <div class="muted">将尝试回滚到 previous digest，并可能触发容器重启。</div>
                  <div class="muted">opId: <code id="rollbackOpId">-</code></div>
                  <div class="popActions">
                    <button id="rollbackCancel">取消</button>
                    <button id="rollbackConfirm" class="danger">确认回滚</button>
                  </div>
                </div>
              </div>
              <button id="refresh">刷新</button>
            </div>
            <div class="actionNote muted">建议先用 dry-run 确认 tag 与 digest，再执行 apply；operation 结束后可直接回滚。</div>
          </div>
        </div>
      </section>

      <section class="panel statusPanel" data-panel="status-grid">
        <div class="sectionHeadingRow">
          <div>
            <div class="sectionEyebrow">Live status</div>
            <h2>当前状态</h2>
          </div>
          <div id="statusTone" class="statusTone" aria-live="polite">loading…</div>
        </div>
        <div class="statusBoard">
          <article class="statusHero">
            <div class="statusLabel">State</div>
            <div class="statusHeroTop">
              <div>
                <div id="statusState" class="statusValue statusValue-lg">loading…</div>
              </div>
              <div class="statusMeta">operation stream</div>
            </div>
            <div id="statusSummary" class="statusMeta statusHeroSummary">等待首次轮询结果…</div>
          </article>
          <div class="statusFacts">
            <article class="statusFact">
              <div class="statusLabel">Current opId</div>
              <div id="statusOpId" class="statusValue">-</div>
              <div class="statusMeta">当前正在追踪的 operation 标识。</div>
            </article>
            <article class="statusFact">
              <div class="statusLabel">Current step</div>
              <div id="statusStep" class="statusValue">-</div>
              <div id="statusMode" class="statusMeta">mode -</div>
            </article>
            <article class="statusFact statusFact-wide">
              <div class="statusLabel">Timestamps</div>
              <div id="statusStartedAt" class="statusValue">-</div>
              <div id="statusUpdatedAt" class="statusMeta">updated -</div>
            </article>
          </div>
          <article class="progressCard">
            <div class="statusLabel">Progress message</div>
            <div id="statusProgressMessage" class="statusCodeInline">-</div>
          </article>
          <div class="artifactGrid">
            <article class="artifactCard">
              <div class="statusLabel">Target</div>
              <pre id="statusTarget" class="statusCode">-</pre>
            </article>
            <article class="artifactCard">
              <div class="statusLabel">Previous</div>
              <pre id="statusPrevious" class="statusCode">-</pre>
            </article>
          </div>
        </div>
      </section>

      <section class="panel workspacePanel" data-panel="workspace">
        <div class="sectionHeadingRow">
          <div>
            <div class="sectionEyebrow">History & logs</div>
            <h2>操作历史与日志</h2>
          </div>
          <div id="historyHint" class="muted sectionHeadingNote">loading…</div>
        </div>
        <div id="workspaceGrid" class="workspaceGrid">
          <aside id="historyRail" class="historyRail">
            <div id="historyList" class="historyList"></div>
          </aside>
          <section class="logPanel">
            <div class="logHeader">
              <div>
                <div class="sectionEyebrow">Console</div>
                <div id="logTitle" class="logTitle">loading…</div>
              </div>
              <div id="logSummary" class="logSummary muted">等待日志…</div>
            </div>
            <pre id="logs"></pre>
          </section>
        </div>
      </section>
    </main>

    <script>
      const base = {base_path_json};
      const themeController = window.__dockrevSupervisorTheme;
      const themeMedia = themeController?.mediaQuery || window.matchMedia('(prefers-color-scheme: dark)');
      let activeOpId = null;
      let latestOpId = null;
      let latestOpMarker = null;
      let latestHasNewer = false;
      let lastKnownSelfUpgradeState = null;

      const dryBtn = document.getElementById('dry');
      const applyBtn = document.getElementById('apply');
      const rollbackBtn = document.getElementById('rollback');
      const rollbackWrap = document.getElementById('rollbackWrap');
      const rollbackPop = document.getElementById('rollbackPop');
      const rollbackOpId = document.getElementById('rollbackOpId');
      const rollbackConfirmBtn = document.getElementById('rollbackConfirm');
      const rollbackCancelBtn = document.getElementById('rollbackCancel');
      const statusToneEl = document.getElementById('statusTone');
      const statusStateEl = document.getElementById('statusState');
      const statusSummaryEl = document.getElementById('statusSummary');
      const statusOpIdEl = document.getElementById('statusOpId');
      const statusStepEl = document.getElementById('statusStep');
      const statusModeEl = document.getElementById('statusMode');
      const statusStartedAtEl = document.getElementById('statusStartedAt');
      const statusUpdatedAtEl = document.getElementById('statusUpdatedAt');
      const statusProgressMessageEl = document.getElementById('statusProgressMessage');
      const statusTargetEl = document.getElementById('statusTarget');
      const statusPreviousEl = document.getElementById('statusPrevious');
      const historyRailEl = document.getElementById('historyRail');
      const historyListEl = document.getElementById('historyList');
      const historyHintEl = document.getElementById('historyHint');
      const workspaceGridEl = document.getElementById('workspaceGrid');
      const logTitleEl = document.getElementById('logTitle');
      const logSummaryEl = document.getElementById('logSummary');
      const logsEl = document.getElementById('logs');
      const toUrl = (p) => base.replace(/\/$/, '') + '/' + p.replace(/^\//, '');
      let rollbackPopOpen = false;
      let rollbackPendingOpId = null;

      function syncTheme() {{
        if (themeController?.syncThemeFromPreference) {{
          themeController.syncThemeFromPreference();
        }}
      }}

      function handleSystemThemeChange() {{
        if (themeController?.hasStoredTheme && themeController.hasStoredTheme()) return;
        syncTheme();
      }}

      async function fetchJson(path, init = {{}}) {{
        const res = await fetch(toUrl(path), {{
          headers: {{ 'content-type': 'application/json' }},
          ...init,
        }});
        if (!res.ok) {{
          const text = await res.text();
          throw new Error(`${{res.status}} ${{text}}`);
        }}
        return res.json();
      }}

      function canRollback(st) {{
        return !!st.opId && (st.state === 'failed' || st.state === 'rolled_back' || st.state === 'succeeded');
      }}

      function setRollbackPopOpen(open) {{
        rollbackPopOpen = open;
        rollbackPop.hidden = !open;
        rollbackBtn.setAttribute('aria-expanded', open ? 'true' : 'false');
        if (!open) rollbackPendingOpId = null;
      }}

      function syncRollbackState(st) {{
        const allowed = canRollback(st);
        rollbackBtn.disabled = !allowed;
        if (!allowed) {{
          setRollbackPopOpen(false);
          rollbackOpId.textContent = '-';
          return;
        }}
        if (rollbackPopOpen) rollbackOpId.textContent = st.opId || '-';
      }}

      function setRunningButton(button, running) {{
        button.classList.toggle('btnRunning', running);
        button.setAttribute('aria-busy', running ? 'true' : 'false');
      }}

      function syncUpgradeActionState(st) {{
        const running = !!st && st.state === 'running';
        const runningUpgrade = running && st?.progress?.step !== 'rollback';
        const mode = st?.request?.mode;
        dryBtn.disabled = running;
        applyBtn.disabled = running;
        setRunningButton(dryBtn, runningUpgrade && mode === 'dry-run');
        setRunningButton(applyBtn, runningUpgrade && mode === 'apply');
      }}

      function normalizeState(value) {{
        return value || 'unknown';
      }}

      function statusToneClass(state) {{
        const normalized = normalizeState(state);
        if (normalized === 'failed' || normalized === 'rolled_back' || normalized === 'offline') return `statusTone state-${{normalized}}`;
        if (normalized === 'running' || normalized === 'succeeded' || normalized === 'idle' || normalized === 'unknown') return `statusTone state-${{normalized}}`;
        return 'statusTone state-unknown';
      }}

      function stateBadgeClass(state) {{
        const normalized = normalizeState(state);
        if (normalized === 'failed' || normalized === 'rolled_back') return `stateBadge stateBadge-${{normalized}}`;
        if (normalized === 'running' || normalized === 'succeeded' || normalized === 'idle' || normalized === 'unknown') return `stateBadge stateBadge-${{normalized}}`;
        return 'stateBadge stateBadge-unknown';
      }}

      function stateDotClass(state) {{
        const normalized = normalizeState(state);
        if (normalized === 'failed' || normalized === 'rolled_back') return `stateDot stateDot-${{normalized}}`;
        if (normalized === 'running' || normalized === 'succeeded' || normalized === 'idle' || normalized === 'unknown') return `stateDot stateDot-${{normalized}}`;
        return 'stateDot stateDot-unknown';
      }}

      function pad2(v) {{
        return String(v).padStart(2, '0');
      }}

      function formatTimestamp(ts) {{
        const d = new Date(ts || '');
        if (Number.isNaN(d.getTime())) return '-';
        return `${{d.getFullYear()}}-${{pad2(d.getMonth() + 1)}}-${{pad2(d.getDate())}} ${{pad2(d.getHours())}}:${{pad2(d.getMinutes())}}:${{pad2(d.getSeconds())}}`;
      }}

      function formatHistoryTime(ts) {{
        const d = new Date(ts || '');
        if (Number.isNaN(d.getTime())) return '-- --:--';
        return `${{pad2(d.getMonth() + 1)}}-${{pad2(d.getDate())}} ${{pad2(d.getHours())}}:${{pad2(d.getMinutes())}}`;
      }}

      function shortOpId(opId) {{
        return String(opId || '-').slice(-6);
      }}

      function operationMarker(op) {{
        if (!op?.opId) return null;
        return `${{op.opId}}:${{op.updatedAt || ''}}:${{(op.logs || []).length}}`;
      }}

      function formatTargetRef(target) {{
        const image = target?.image || '-';
        const tag = target?.tag ? `:${{target.tag}}` : '';
        const digest = target?.digest ? `@${{target.digest}}` : '';
        return `${{image}}${{tag}}${{digest}}`;
      }}

      function formatPreviousRef(previous) {{
        const tag = previous?.tag || '-';
        return `${{tag}}${{previous?.digest ? '@' + previous.digest : ''}}`;
      }}

      function formatLogs(logs) {{
        return (logs || []).map((line) => `[${{line.ts}}] ${{line.level}} ${{line.msg}}`).join('\n');
      }}

      function renderStatus(st) {{
        statusToneEl.className = statusToneClass(st?.state);
        statusToneEl.textContent = normalizeState(st?.state);
        statusStateEl.textContent = normalizeState(st?.state);
        statusSummaryEl.textContent = `${{st?.request?.mode || 'mode -'}} · auto-refresh 1.5s`;
        statusOpIdEl.textContent = st?.opId || '-';
        statusStepEl.textContent = st?.progress?.step || '-';
        statusModeEl.textContent = `mode ${{st?.request?.mode || '-'}}`;
        statusStartedAtEl.textContent = formatTimestamp(st?.startedAt);
        statusUpdatedAtEl.textContent = `updated ${{formatTimestamp(st?.updatedAt)}}`;
        statusProgressMessageEl.textContent = st?.progress?.message || '-';
        statusTargetEl.textContent = formatTargetRef(st?.target);
        statusPreviousEl.textContent = formatPreviousRef(st?.previous);
      }}

      function renderOffline(error) {{
        const cached = lastKnownSelfUpgradeState;
        const hasCachedState = !!cached;
        const lastSeen = cached?.updatedAt ? formatTimestamp(cached.updatedAt) : '-';
        statusToneEl.className = 'statusTone state-offline';
        statusToneEl.textContent = 'offline';
        statusStateEl.textContent = 'offline';
        statusSummaryEl.textContent = `poll failed · ${{String(error.message || error)}}`;
        if (!hasCachedState) {{
          statusOpIdEl.textContent = 'unavailable';
          statusStepEl.textContent = 'waiting for reconnect';
          statusModeEl.textContent = 'no cached state yet';
          statusStartedAtEl.textContent = '-';
          statusUpdatedAtEl.textContent = 'last seen -';
          statusProgressMessageEl.textContent = 'supervisor unreachable on first poll; retrying…';
          statusTargetEl.textContent = 'unavailable while offline';
          statusPreviousEl.textContent = 'unavailable while offline';
          return;
        }}
        statusOpIdEl.textContent = cached.opId ? `${{cached.opId}} · stale` : 'stale';
        statusStepEl.textContent = cached.progress?.step ? `${{cached.progress.step}} · stale` : 'stale';
        statusModeEl.textContent = `last seen ${{lastSeen}} · cached`;
        statusStartedAtEl.textContent = cached.startedAt ? `${{formatTimestamp(cached.startedAt)}} · stale` : '-';
        statusUpdatedAtEl.textContent = `last seen ${{lastSeen}}`;
        statusProgressMessageEl.textContent = cached.progress?.message
          ? `${{cached.progress.message}} · stale while offline`
          : 'offline; waiting for supervisor to respond again';
        statusTargetEl.textContent = cached.target
          ? `${{formatTargetRef(cached.target)}} · stale`
          : 'stale while offline';
        statusPreviousEl.textContent = cached.previous
          ? `${{formatPreviousRef(cached.previous)}} · stale`
          : 'stale while offline';
      }}

      function syncWorkspaceMode(hasOperations) {{
        workspaceGridEl.classList.toggle('workspaceGrid-logsOnly', !hasOperations);
        historyRailEl.hidden = !hasOperations;
      }}

      function createStateBadge(state) {{
        const badge = document.createElement('span');
        badge.className = stateBadgeClass(state);

        const dot = document.createElement('span');
        dot.className = stateDotClass(state);
        badge.appendChild(dot);

        const text = document.createElement('span');
        text.textContent = normalizeState(state);
        badge.appendChild(text);
        return badge;
      }}

      function renderOperations(st) {{
        const operations = Array.isArray(st.operations) ? st.operations : [];
        historyListEl.textContent = '';

        if (!operations.length) {{
          syncWorkspaceMode(false);
          activeOpId = null;
          latestOpId = null;
          latestOpMarker = null;
          latestHasNewer = false;
          historyHintEl.textContent = '暂无 operation 历史，已回退到扁平日志视图。';
          logTitleEl.textContent = '当前日志';
          logSummaryEl.textContent = '未发现按 operation 分组的历史记录。';
          logsEl.textContent = formatLogs(st.logs || []) || '暂无日志';
          return;
        }}

        syncWorkspaceMode(true);
        const previousLatest = latestOpId;
        const previousLatestMarker = latestOpMarker;
        const nextLatestOp = operations[0] || null;
        const nextLatest = nextLatestOp?.opId || null;
        const nextLatestMarker = operationMarker(nextLatestOp);
        const wasViewingLatest = !activeOpId || (previousLatest && activeOpId === previousLatest);
        if (nextLatest && wasViewingLatest) {{
          activeOpId = nextLatest;
        }} else if (!operations.some((op) => op.opId === activeOpId)) {{
          activeOpId = nextLatest;
        }}
        if (!wasViewingLatest && previousLatestMarker && nextLatestMarker && previousLatestMarker !== nextLatestMarker) {{
          latestHasNewer = true;
        }}
        latestOpId = nextLatest;
        latestOpMarker = nextLatestMarker;
        if (activeOpId && activeOpId === latestOpId) {{
          latestHasNewer = false;
        }}

        for (let i = 0; i < operations.length; i += 1) {{
          const op = operations[i];
          const button = document.createElement('button');
          button.type = 'button';
          button.className = 'historyCard';
          if (op.opId === activeOpId) {{
            button.classList.add('active');
          }}
          button.onclick = () => {{
            activeOpId = op.opId;
            if (activeOpId === latestOpId) {{
              latestHasNewer = false;
            }}
            renderOperations(st);
          }};

          const top = document.createElement('div');
          top.className = 'historyTop';

          const time = document.createElement('div');
          time.className = 'historyTime';
          time.textContent = formatHistoryTime(op.startedAt);
          top.appendChild(time);

          const badges = document.createElement('div');
          badges.className = 'historyBadges';
          badges.appendChild(createStateBadge(op.state));
          if (i === 0) {{
            const latest = document.createElement('span');
            latest.className = 'newBadge';
            latest.textContent = '最新';
            badges.appendChild(latest);
          }}
          if (i === 0 && latestHasNewer && activeOpId !== op.opId) {{
            const newer = document.createElement('span');
            newer.className = 'newBadge';
            newer.textContent = '新日志';
            badges.appendChild(newer);
          }}
          top.appendChild(badges);
          button.appendChild(top);

          const bottom = document.createElement('div');
          bottom.className = 'historyBottom';

          const meta = document.createElement('div');
          meta.className = 'historyMeta';
          meta.textContent = `${{shortOpId(op.opId)}} · ${{(op.logs || []).length}} lines`;
          bottom.appendChild(meta);

          const tail = document.createElement('div');
          tail.className = 'historyTail';
          tail.textContent = `updated ${{formatHistoryTime(op.updatedAt)}}`;
          bottom.appendChild(tail);
          button.appendChild(bottom);

          historyListEl.appendChild(button);
        }}

        const active = operations.find((op) => op.opId === activeOpId) || operations[0];
        historyHintEl.textContent = `最近 ${{operations.length}} 次 operation · 当前 ${{active.opId}}`;
        logTitleEl.textContent = `${{formatHistoryTime(active.startedAt)}} · ${{shortOpId(active.opId)}}`;
        logSummaryEl.textContent = `${{normalizeState(active.state)}} · updated ${{formatTimestamp(active.updatedAt)}} · ${{(active.logs || []).length}} lines`;
        logsEl.textContent = formatLogs(active.logs || []) || '暂无日志';
      }}

      async function refresh() {{
        try {{
          const st = await fetchJson('self-upgrade');
          lastKnownSelfUpgradeState = st;
          renderStatus(st);
          syncUpgradeActionState(st);
          renderOperations(st);
          syncRollbackState(st);
        }} catch (error) {{
          renderOffline(error);
          if (lastKnownSelfUpgradeState) {{
            syncUpgradeActionState(lastKnownSelfUpgradeState);
            renderOperations(lastKnownSelfUpgradeState);
            syncRollbackState(lastKnownSelfUpgradeState);
          }} else {{
            syncWorkspaceMode(false);
            historyListEl.textContent = '';
            historyHintEl.textContent = 'Supervisor 暂时离线，尚未拿到可展示的 operation 历史。';
            logTitleEl.textContent = '等待 supervisor 响应';
            logSummaryEl.textContent = '首次轮询失败；暂无可复用的缓存日志。';
            logsEl.textContent = '等待日志…';
            syncRollbackState({{}});
          }}
          setRollbackPopOpen(false);
        }}
      }}

      window.addEventListener('storage', (evt) => {{
        if (evt.key && evt.key !== themeController?.storageKey) return;
        syncTheme();
      }});
      if (typeof themeMedia.addEventListener === 'function') {{
        themeMedia.addEventListener('change', handleSystemThemeChange);
      }} else if (typeof themeMedia.addListener === 'function') {{
        themeMedia.addListener(handleSystemThemeChange);
      }}

      document.getElementById('refresh').onclick = () => refresh();
      dryBtn.onclick = async () => {{
        const tag = document.getElementById('tag').value || 'latest';
        await fetchJson('self-upgrade', {{ method: 'POST', body: JSON.stringify({{ target: {{ tag }}, mode: 'dry-run', rollbackOnFailure: true }}) }});
        await refresh();
      }};
      applyBtn.onclick = async () => {{
        const tag = document.getElementById('tag').value || 'latest';
        await fetchJson('self-upgrade', {{ method: 'POST', body: JSON.stringify({{ target: {{ tag }}, mode: 'apply', rollbackOnFailure: true }}) }});
        await refresh();
      }};
      document.getElementById('rollback').onclick = async (evt) => {{
        evt.preventDefault();
        if (rollbackBtn.disabled) return;
        const st = await fetchJson('self-upgrade');
        syncRollbackState(st);
        if (!canRollback(st)) {{
          await refresh();
          return;
        }}
        rollbackPendingOpId = st.opId || null;
        rollbackOpId.textContent = rollbackPendingOpId || '-';
        setRollbackPopOpen(true);
      }};
      rollbackCancelBtn.onclick = () => {{
        setRollbackPopOpen(false);
      }};
      document.getElementById('rollbackConfirm').onclick = async () => {{
        if (!rollbackPendingOpId) {{
          setRollbackPopOpen(false);
          await refresh();
          return;
        }}
        const st = await fetchJson('self-upgrade');
        syncRollbackState(st);
        if (!canRollback(st)) {{
          setRollbackPopOpen(false);
          await refresh();
          return;
        }}
        if (!st.opId || st.opId !== rollbackPendingOpId) {{
          setRollbackPopOpen(false);
          await refresh();
          return;
        }}
        rollbackConfirmBtn.disabled = true;
        rollbackCancelBtn.disabled = true;
        try {{
          await fetchJson('self-upgrade/rollback', {{ method: 'POST', body: JSON.stringify({{ opId: rollbackPendingOpId }}) }});
          setRollbackPopOpen(false);
          await refresh();
        }} finally {{
          rollbackConfirmBtn.disabled = false;
          rollbackCancelBtn.disabled = false;
        }}
      }};
      document.addEventListener('click', (evt) => {{
        if (!rollbackPopOpen) return;
        const target = evt.target;
        if (!rollbackWrap.contains(target)) setRollbackPopOpen(false);
      }});
      document.addEventListener('keydown', (evt) => {{
        if (evt.key === 'Escape' && rollbackPopOpen) {{
          evt.preventDefault();
          setRollbackPopOpen(false);
          rollbackBtn.focus();
        }}
      }});

      refresh();
      setInterval(refresh, 1500);
    </script>
  </body>
</html>"#,
        base_path = base_path,
        version_html = version_html,
        repository_html = repository_html,
        developer_html = developer_html,
        base_path_json =
            serde_json::to_string(base_path).unwrap_or_else(|_| "\"/supervisor\"".to_string()),
        theme_storage_key_json = serde_json::to_string(THEME_STORAGE_KEY)
            .unwrap_or_else(|_| "\"dockrev:theme\"".to_string())
    )
}
