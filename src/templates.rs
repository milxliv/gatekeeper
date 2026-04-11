use crate::models::*;
use std::cell::RefCell;

/// Escape HTML special characters to prevent XSS
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Pixel-quantization script for 2-color thermal printers (Brother QL-820NWB).
///
/// The printer has separate black and red thermal heads. Any pixel that isn't
/// exactly (0,0,0) gets routed to the red head. Anti-aliased text produces gray
/// edge pixels (e.g. #2a2a2a) which the printer classifies as "not black" → red.
///
/// This script intercepts window.print(), rasterizes the badge HTML to a canvas
/// via SVG foreignObject, then snaps every pixel to one of three values:
///   - Pure white  (255, 255, 255)
///   - Pure black  (0, 0, 0)
///   - Pure red    (255, 0, 0)
/// The quantized bitmap is then printed instead of the raw HTML.
/// Group badge variant — quantizes each badge-sheet div separately, then prints
/// all as a sequence of full-page images.
const QUANTIZE_GROUP_PRINT_JS: &str = r##"
async function quantizeGroupAndPrint() {
  try {
    var sheets = document.querySelectorAll('.badge-sheet');
    if (sheets.length === 0) { window.print(); return; }

    var styleEl = document.querySelector('style');
    var css = styleEl ? styleEl.textContent : '';
    css = css.replace(/@media\s+(screen|print)\s*\{[^}]*\}/g, '');

    // Convert images to base64
    var imgs = document.querySelectorAll('img');
    for (var j = 0; j < imgs.length; j++) {
      var im = imgs[j];
      if (im.complete && im.naturalWidth > 0 && !im.src.startsWith('data:')) {
        try {
          var tc = document.createElement('canvas');
          tc.width = im.naturalWidth; tc.height = im.naturalHeight;
          tc.getContext('2d').drawImage(im, 0, 0);
          im.setAttribute('src', tc.toDataURL('image/png'));
        } catch(e) {}
      }
    }

    var scale = 8;
    var canvases = [];

    for (var s = 0; s < sheets.length; s++) {
      var sheet = sheets[s];
      var w = sheet.scrollWidth;
      var h = sheet.scrollHeight;
      var cw = w * scale;
      var ch = h * scale;

      var xhtml = '<div xmlns="http://www.w3.org/1999/xhtml"'
        + ' style="width:' + w + 'px;height:' + h + 'px;overflow:hidden;'
        + 'font-family:-apple-system,BlinkMacSystemFont,Segoe UI,Helvetica,Arial,sans-serif;">'
        + '<style>' + css + '</style>'
        + sheet.outerHTML
        + '</div>';

      var svgStr = '<svg xmlns="http://www.w3.org/2000/svg" width="' + cw + '" height="' + ch + '">'
        + '<foreignObject width="' + w + '" height="' + h + '" transform="scale(' + scale + ')">'
        + xhtml
        + '</foreignObject></svg>';

      var cv = await new Promise(function(resolve, reject) {
        var canvas = document.createElement('canvas');
        canvas.width = cw; canvas.height = ch;
        var ctx = canvas.getContext('2d');
        var blob = new Blob([svgStr], {type: 'image/svg+xml;charset=utf-8'});
        var url = URL.createObjectURL(blob);
        var img = new Image();
        img.onload = function() {
          ctx.drawImage(img, 0, 0);
          URL.revokeObjectURL(url);
          var imageData = ctx.getImageData(0, 0, cw, ch);
          var d = imageData.data;
          for (var i = 0; i < d.length; i += 4) {
            var r = d[i], g = d[i+1], b = d[i+2], a = d[i+3];
            if (a < 128) { d[i]=255;d[i+1]=255;d[i+2]=255;d[i+3]=255; continue; }
            var luma = 0.299*r + 0.587*g + 0.114*b;
            var isRed = (r > 150 && g < 100 && b < 100 && r > g * 2);
            if (isRed) { d[i]=255;d[i+1]=0;d[i+2]=0; }
            else if (luma < 200) { d[i]=0;d[i+1]=0;d[i+2]=0; }
            else { d[i]=255;d[i+1]=255;d[i+2]=255; }
            d[i+3]=255;
          }
          ctx.putImageData(imageData, 0, 0);
          resolve(canvas);
        };
        img.onerror = function() { URL.revokeObjectURL(url); reject('render failed'); };
        img.src = url;
      });
      canvases.push(cv);
    }

    // Replace page with canvas elements directly
    document.head.innerHTML = '<style>'
      + '@page{size:4in 2.4in;margin:0;}'
      + '*{margin:0;padding:0;}'
      + 'body{width:4in;}'
      + 'canvas{display:block;width:4in;height:2.4in;image-rendering:pixelated;image-rendering:-moz-crisp-edges;image-rendering:crisp-edges;}'
      + '</style>';
    document.body.innerHTML = '';
    for (var c = 0; c < canvases.length; c++) {
      var cv = canvases[c];
      cv.style.width = '4in';
      cv.style.height = '2.4in';
      cv.style.display = 'block';
      cv.style.imageRendering = 'pixelated';
      if (c < canvases.length - 1) cv.style.pageBreakAfter = 'always';
      document.body.appendChild(cv);
    }
    setTimeout(function(){ window.print(); }, 300);
  } catch(e) {
    console.error('Group badge quantization failed:', e);
    window.print();
  }
}
quantizeGroupAndPrint();
"##;

const QUANTIZE_PRINT_JS: &str = r##"
async function quantizeAndPrint() {
  try {
    var body = document.body;
    var w = body.scrollWidth;
    var h = body.scrollHeight;
    var scale = 8;
    var cw = w * scale;
    var ch = h * scale;

    // Convert <img> elements to inline base64 so foreignObject can render them
    var imgs = document.querySelectorAll('img');
    for (var j = 0; j < imgs.length; j++) {
      var im = imgs[j];
      if (im.complete && im.naturalWidth > 0 && !im.src.startsWith('data:')) {
        try {
          var tc = document.createElement('canvas');
          tc.width = im.naturalWidth; tc.height = im.naturalHeight;
          tc.getContext('2d').drawImage(im, 0, 0);
          im.setAttribute('src', tc.toDataURL('image/png'));
        } catch(e) {}
      }
    }

    // Grab rendered CSS (strip @media screen/print blocks — they confuse foreignObject)
    var styleEl = document.querySelector('style');
    var css = styleEl ? styleEl.textContent : '';
    css = css.replace(/@media\s+(screen|print)\s*\{[^}]*\}/g, '');

    // Build XHTML wrapper for foreignObject
    var xhtml = '<div xmlns="http://www.w3.org/1999/xhtml"'
      + ' style="width:' + w + 'px;height:' + h + 'px;overflow:hidden;'
      + 'font-family:-apple-system,BlinkMacSystemFont,Segoe UI,Helvetica,Arial,sans-serif;">'
      + '<style>' + css + '</style>'
      + body.innerHTML
      + '</div>';

    var svgStr = '<svg xmlns="http://www.w3.org/2000/svg" width="' + cw + '" height="' + ch + '">'
      + '<foreignObject width="' + w + '" height="' + h + '" transform="scale(' + scale + ')">'
      + xhtml
      + '</foreignObject></svg>';

    var canvas = document.createElement('canvas');
    canvas.width = cw; canvas.height = ch;
    var ctx = canvas.getContext('2d');

    var blob = new Blob([svgStr], {type: 'image/svg+xml;charset=utf-8'});
    var url = URL.createObjectURL(blob);

    var img = new Image();
    img.onload = function() {
      ctx.drawImage(img, 0, 0);
      URL.revokeObjectURL(url);

      // ── Pixel quantization: snap to black / white / red ──
      // Threshold 200: anything even slightly dark → pure black.
      // This eliminates ALL anti-aliased gray edge pixels.
      var imageData = ctx.getImageData(0, 0, cw, ch);
      var d = imageData.data;
      for (var i = 0; i < d.length; i += 4) {
        var r = d[i], g = d[i+1], b = d[i+2], a = d[i+3];
        if (a < 128) { d[i]=255;d[i+1]=255;d[i+2]=255;d[i+3]=255; continue; }
        var luma = 0.299*r + 0.587*g + 0.114*b;
        // Detect intentionally red pixels (red channel dominant over green+blue)
        var isRed = (r > 150 && g < 100 && b < 100 && r > g * 2);
        if (isRed) { d[i]=255; d[i+1]=0; d[i+2]=0; }
        else if (luma < 200) { d[i]=0; d[i+1]=0; d[i+2]=0; }
        else { d[i]=255; d[i+1]=255; d[i+2]=255; }
        d[i+3] = 255;
      }
      ctx.putImageData(imageData, 0, 0);

      // Replace page with the canvas element directly (not an <img>).
      // Canvas prints at its native pixel resolution — no browser resampling.
      canvas.style.width = '4in';
      canvas.style.height = '2.4in';
      canvas.style.display = 'block';
      canvas.style.imageRendering = 'pixelated';
      document.head.innerHTML = '<style>'
        + '@page{size:4in 2.4in;margin:0;}'
        + '*{margin:0;padding:0;}'
        + 'body{width:4in;height:2.4in;overflow:hidden;}'
        + 'canvas{image-rendering:pixelated;image-rendering:-moz-crisp-edges;image-rendering:crisp-edges;}'
        + '</style>';
      document.body.innerHTML = '';
      document.body.appendChild(canvas);
      setTimeout(function(){ window.print(); }, 300);
    };
    img.onerror = function() {
      URL.revokeObjectURL(url);
      window.print();
    };
    img.src = url;
  } catch(e) {
    console.error('Badge quantization failed, falling back to raw print:', e);
    window.print();
  }
}
quantizeAndPrint();
"##;

/// Login page — standalone, no sidebar
pub fn login_page(error: Option<&str>) -> String {
    let error_html = match error {
        Some(msg) => format!(r#"<div style="color:#dc2626;background:#fee2e2;padding:0.75rem;border-radius:6px;margin-bottom:1rem;font-size:0.9rem;">{}</div>"#, msg),
        None => String::new(),
    };

    format!(r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>GateKeeper — Login</title>
<style>
* {{ margin:0; padding:0; box-sizing:border-box; }}
body {{
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
    background: #f3f4f6; display:flex; justify-content:center; align-items:center;
    min-height:100vh;
}}
.login-card {{
    background: #fff; border-radius: 12px; padding: 2.5rem;
    box-shadow: 0 4px 24px rgba(0,0,0,0.1); width: 100%; max-width: 380px;
}}
.login-card h1 {{ font-size:1.5rem; margin-bottom:0.25rem; }}
.login-card .sub {{ color:#666; font-size:0.9rem; margin-bottom:1.5rem; }}
.login-card label {{ display:block; font-weight:600; margin-bottom:0.4rem; font-size:0.9rem; }}
.login-card input[type="password"] {{
    width:100%; padding:0.75rem; border:1px solid #d1d5db; border-radius:6px;
    font-size:1rem; margin-bottom:1rem;
}}
.login-card button {{
    width:100%; padding:0.75rem; background:#1a56db; color:#fff; border:none;
    border-radius:6px; font-size:1rem; font-weight:600; cursor:pointer;
}}
.login-card button:hover {{ background:#1e40af; }}
</style>
</head>
<body>
<div class="login-card">
    <h1>GateKeeper</h1>
    <p class="sub">Visitor Management System</p>
    {error}
    <form method="POST" action="/login">
        <label for="password">Password</label>
        <input type="password" name="password" id="password" placeholder="Enter admin password" autofocus required>
        <button type="submit">Sign In</button>
    </form>
</div>
</body>
</html>"#, error = error_html)
}

thread_local! {
    static CURRENT_THEME: RefCell<String> = RefCell::new("system".to_string());
    static CURRENT_ROLE: RefCell<String> = RefCell::new("admin".to_string());
}

/// Set the theme for the current request (call before rendering)
pub fn set_theme(theme: &str) {
    CURRENT_THEME.with(|t| *t.borrow_mut() = theme.to_string());
}

/// Set the user role for the current request
pub fn set_role(role: &str) {
    CURRENT_ROLE.with(|r| *r.borrow_mut() = role.to_string());
}

fn get_theme() -> String {
    CURRENT_THEME.with(|t| t.borrow().clone())
}

fn is_admin() -> bool {
    CURRENT_ROLE.with(|r| r.borrow().as_str() == "admin")
}

/// Wraps content in the base layout shell
pub fn layout_with_theme(title: &str, content: &str, theme: &str) -> String {
    let theme_attr = match theme {
        "light" => r#" data-theme="light""#,
        "dark" => r#" data-theme="dark""#,
        _ => "", // system — no attribute, uses prefers-color-scheme
    };
    format!(r##"<!DOCTYPE html>
<html lang="en"{theme_attr}>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{title} — GateKeeper</title>
    <script src="/static/htmx.min.js"></script>
    <style>
        /* ── Dark theme (default) ── */
        :root {{
            --bg: #0f1117;
            --surface: #1a1d27;
            --surface2: #252836;
            --border: #2e3347;
            --text: #e1e4ed;
            --text-dim: #8b90a5;
            --accent: #4f8cff;
            --accent-hover: #6ba0ff;
            --green: #34d399;
            --yellow: #fbbf24;
            --red: #f87171;
            --orange: #fb923c;
        }}
        /* ── Light theme ── */
        [data-theme="light"] {{
            --bg: #f5f6fa;
            --surface: #ffffff;
            --surface2: #eef0f5;
            --border: #d1d5e0;
            --text: #1a1d27;
            --text-dim: #6b7085;
            --accent: #2563eb;
            --accent-hover: #1d4ed8;
            --green: #16a34a;
            --yellow: #ca8a04;
            --red: #dc2626;
            --orange: #ea580c;
        }}
        /* ── System theme (no data-theme attr): follow OS preference ── */
        @media (prefers-color-scheme: light) {{
            html:not([data-theme]) {{
                --bg: #f5f6fa;
                --surface: #ffffff;
                --surface2: #eef0f5;
                --border: #d1d5e0;
                --text: #1a1d27;
                --text-dim: #6b7085;
                --accent: #2563eb;
                --accent-hover: #1d4ed8;
                --green: #16a34a;
                --yellow: #ca8a04;
                --red: #dc2626;
                --orange: #ea580c;
            }}
        }}
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
            background: var(--bg);
            color: var(--text);
            line-height: 1.5;
        }}
        .shell {{ display: flex; min-height: 100vh; }}
        /* ── Sidebar ── */
        .sidebar {{
            width: 240px;
            background: var(--surface);
            border-right: 1px solid var(--border);
            padding: 1.5rem 1rem;
            flex-shrink: 0;
        }}
        .sidebar h1 {{
            font-size: 1.25rem;
            font-weight: 700;
            margin-bottom: 0.25rem;
            color: var(--accent);
        }}
        .sidebar .subtitle {{ font-size: 0.75rem; color: var(--text-dim); margin-bottom: 2rem; }}
        .sidebar nav a {{
            display: block;
            padding: 0.6rem 0.75rem;
            margin-bottom: 0.25rem;
            border-radius: 6px;
            color: var(--text-dim);
            text-decoration: none;
            font-size: 0.9rem;
            transition: all 0.15s;
        }}
        .sidebar nav a:hover, .sidebar nav a.active {{
            background: var(--surface2);
            color: var(--text);
        }}
        /* ── Main content ── */
        .main {{ flex: 1; padding: 2rem; max-width: 1200px; }}
        .main h2 {{ font-size: 1.5rem; font-weight: 600; margin-bottom: 1.5rem; }}
        /* ── Cards / panels ── */
        .card {{
            background: var(--surface);
            border: 1px solid var(--border);
            border-radius: 8px;
            padding: 1.5rem;
            margin-bottom: 1.5rem;
        }}
        .card h3 {{ font-size: 1.1rem; margin-bottom: 1rem; }}
        /* ── Stats row ── */
        .stats {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 1rem; margin-bottom: 2rem; }}
        .stat {{
            background: var(--surface);
            border: 1px solid var(--border);
            border-radius: 8px;
            padding: 1rem 1.25rem;
        }}
        .stat .label {{ font-size: 0.8rem; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.5px; }}
        .stat .value {{ font-size: 1.75rem; font-weight: 700; margin-top: 0.25rem; }}
        /* ── Table ── */
        table {{ width: 100%; border-collapse: collapse; }}
        th, td {{ padding: 0.75rem 1rem; text-align: left; border-bottom: 1px solid var(--border); }}
        th {{ font-size: 0.8rem; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.5px; font-weight: 600; }}
        tr:hover {{ background: var(--surface2); }}
        /* ── Badges ── */
        .badge {{
            display: inline-block;
            padding: 0.2rem 0.6rem;
            border-radius: 999px;
            font-size: 0.75rem;
            font-weight: 600;
            text-transform: uppercase;
        }}
        .badge.pending {{ background: rgba(139,144,165,0.15); color: var(--text-dim); }}
        .badge.approved {{ background: rgba(79,140,255,0.15); color: var(--accent); }}
        .badge.checked_in {{ background: rgba(52,211,153,0.1); color: var(--green); }}
        .badge.checked_out {{ background: rgba(139,144,165,0.15); color: var(--text-dim); }}
        .badge.denied {{ background: rgba(248,113,113,0.15); color: var(--red); }}
        .badge.walk_in {{ background: rgba(251,146,60,0.15); color: var(--orange); }}
        .badge.running_late {{ background: rgba(79,140,255,0.1); color: var(--accent); }}
        .badge.rescheduled {{ background: rgba(139,144,165,0.15); color: var(--text-dim); }}
        /* ── Forms ── */
        .form-grid {{ display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; }}
        .form-group {{ margin-bottom: 1rem; }}
        .form-group.full {{ grid-column: 1 / -1; }}
        label {{ display: block; font-size: 0.85rem; color: var(--text-dim); margin-bottom: 0.35rem; }}
        input, select, textarea {{
            width: 100%;
            padding: 0.6rem 0.75rem;
            background: var(--bg);
            border: 1px solid var(--border);
            border-radius: 6px;
            color: var(--text);
            font-size: 0.9rem;
        }}
        input:focus, select:focus, textarea:focus {{
            outline: none;
            border-color: var(--accent);
        }}
        textarea {{ resize: vertical; min-height: 80px; }}
        /* ── Buttons ── */
        .btn {{
            display: inline-block;
            padding: 0.6rem 1.25rem;
            border: none;
            border-radius: 6px;
            font-size: 0.9rem;
            font-weight: 600;
            cursor: pointer;
            text-decoration: none;
            transition: all 0.15s;
        }}
        .btn-primary {{ background: var(--accent); color: #fff; }}
        .btn-primary:hover {{ background: var(--accent-hover); }}
        .btn-success {{ background: rgba(52,211,153,0.8); color: #000; }}
        .btn-danger {{ background: var(--red); color: #fff; }}
        .btn-sm {{ padding: 0.35rem 0.75rem; font-size: 0.8rem; }}
        .btn-warning {{ background: var(--yellow); color: #000; }}
        .btn-warning:hover {{ opacity: 0.85; }}
        .btn-muted {{ background: var(--surface2); border: 1px solid var(--border); color: var(--text-dim); }}
        .btn-muted:hover {{ border-color: var(--text-dim); color: var(--text); }}
        select.btn-muted {{
            appearance: none;
            -webkit-appearance: none;
            background: var(--surface2) url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='10' viewBox='0 0 12 12'%3E%3Cpath fill='%236b7085' d='M6 8L1 3h10z'/%3E%3C/svg%3E") no-repeat right 4px center;
            border: 1px solid var(--border);
            padding-right: 16px;
        }}
        select.btn-muted option {{ background: var(--surface); color: var(--text); }}
        .btn-ghost {{ background: transparent; border: 1px solid var(--border); color: var(--text-dim); }}
        .btn-ghost:hover {{ border-color: var(--text-dim); color: var(--text); }}
        .actions {{ display: flex; gap: 0.25rem; align-items: center; flex-wrap: wrap; }}
        .actions .btn, .actions select {{ font-size: 0.7rem; padding: 0.25rem 0.5rem; white-space: nowrap; }}
        /* ── Search bar ── */
        .search-bar {{ display: flex; gap: 0.75rem; margin-bottom: 1.5rem; align-items: end; }}
        .search-bar .form-group {{ margin-bottom: 0; }}
        /* ── HTMX indicator ── */
        .htmx-indicator {{ opacity: 0; transition: opacity 0.2s; }}
        .htmx-request .htmx-indicator {{ opacity: 1; }}
        .spinner {{ display: inline-block; width: 16px; height: 16px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.6s linear infinite; }}
        @keyframes spin {{ to {{ transform: rotate(360deg); }} }}
        /* ── Alert / toast ── */
        .alert {{
            padding: 0.75rem 1rem;
            border-radius: 6px;
            margin-bottom: 1rem;
            font-size: 0.9rem;
        }}
        .alert-success {{ background: rgba(52,211,153,0.15); color: var(--green); border: 1px solid rgba(52,211,153,0.3); }}
        .alert-error {{ background: rgba(248,113,113,0.15); color: var(--red); border: 1px solid rgba(248,113,113,0.3); }}
        .alert-warning {{ background: rgba(251,146,60,0.15); color: var(--orange); border: 1px solid rgba(251,146,60,0.3); }}

        @media (max-width: 768px) {{
            .shell {{ flex-direction: column; }}
            .sidebar {{ width: 100%; border-right: none; border-bottom: 1px solid var(--border); padding: 1rem; }}
            .sidebar nav {{ display: flex; gap: 0.5rem; flex-wrap: wrap; }}
            .sidebar nav a {{ margin-bottom: 0; }}
            .form-grid {{ grid-template-columns: 1fr; }}
            .main {{ padding: 1rem; }}
        }}
    </style>
</head>
<body>
    <div class="shell">
        <aside class="sidebar">
            <h1>⛊ GateKeeper</h1>
            <div class="subtitle">WBBH Visitor Management</div>
            <nav>
                {sidebar_nav}
            </nav>
        </aside>
        <main class="main">
            {content}
        </main>
    </div>
    <!-- Check-in modal: Step 1 = Camera, Step 2 = Badge Preview -->
    <div id="camera-modal" style="display:none;position:fixed;inset:0;background:rgba(0,0,0,0.8);z-index:1000;justify-content:center;align-items:center;">
        <div id="modal-inner" style="background:var(--surface);border-radius:12px;padding:1.5rem;max-width:520px;width:90%;text-align:center;">

            <!-- Step 1: Camera -->
            <div id="step-camera">
                <h3 id="camera-title" style="margin-bottom:1rem;">Capture Visitor Photo</h3>
                <div id="camera-select-wrap" style="margin-bottom:0.75rem;display:none;">
                    <select id="camera-select" onchange="startCamera()"
                            style="background:var(--surface2);color:var(--text);border:1px solid var(--border);border-radius:6px;padding:0.4rem 0.6rem;font-size:0.85rem;width:100%;max-width:320px;">
                    </select>
                </div>
                <div style="display:flex;gap:1rem;align-items:flex-start;justify-content:center;flex-wrap:wrap;">
                    <div>
                        <video id="camera-video" autoplay playsinline
                               style="width:320px;height:240px;border-radius:8px;background:#000;display:block;"></video>
                        <canvas id="camera-canvas" width="480" height="360" style="display:none;"></canvas>
                    </div>
                    <div>
                        <img id="camera-preview" style="display:none;width:320px;height:240px;border-radius:8px;object-fit:cover;border:2px solid var(--green);" />
                    </div>
                </div>
                <div style="margin-top:1rem;display:flex;gap:0.5rem;justify-content:center;flex-wrap:wrap;">
                    <button id="btn-capture" class="btn btn-primary" onclick="capturePhoto()">Take Photo</button>
                    <button id="btn-retake" class="btn btn-ghost" onclick="retakePhoto()" style="display:none;">Retake</button>
                    <button id="btn-next" class="btn btn-success" onclick="goToPreview()">Next: Preview Badge</button>
                    <button id="btn-skip" class="btn btn-ghost" onclick="goToPreview()">Skip Photo</button>
                    <button class="btn btn-danger btn-sm" onclick="closeCamera()">Cancel</button>
                </div>
            </div>

            <!-- Step 2: Badge Preview -->
            <div id="step-preview" style="display:none;">
                <h3 style="margin-bottom:1rem;">Badge Preview</h3>
                <div id="badge-preview-container" style="display:flex;justify-content:center;margin-bottom:1rem;">
                    <iframe id="badge-preview-frame"
                            style="width:420px;height:320px;border:1px solid var(--border);border-radius:8px;background:#fff;"
                            sandbox="allow-same-origin"></iframe>
                </div>
                <div style="display:flex;gap:0.5rem;justify-content:center;flex-wrap:wrap;">
                    <button class="btn btn-ghost" onclick="goBackToCamera()">Retake Photo</button>
                    <button class="btn btn-success" onclick="approveAndPrint()">Approve &amp; Print Badge</button>
                    <button class="btn btn-danger btn-sm" onclick="closeCamera()">Cancel</button>
                </div>
            </div>

        </div>
    </div>

    <!-- Reschedule modal -->
    <div id="reschedule-modal" style="display:none;position:fixed;inset:0;background:rgba(0,0,0,0.8);z-index:1000;justify-content:center;align-items:center;">
        <div style="background:var(--surface);border-radius:12px;padding:2rem;max-width:380px;width:95%;">
            <h3 style="margin-bottom:1rem;">Reschedule Visit</h3>
            <input type="hidden" id="reschedule-visit-id">
            <label style="display:block;margin-bottom:0.5rem;color:var(--text-dim);font-size:0.85rem;">New Date</label>
            <input type="date" id="reschedule-date" style="width:100%;padding:0.5rem;margin-bottom:1rem;background:var(--surface2);border:1px solid var(--border);border-radius:6px;color:var(--text);">
            <label style="display:block;margin-bottom:0.5rem;color:var(--text-dim);font-size:0.85rem;">New Time (optional)</label>
            <input type="time" id="reschedule-time" style="width:100%;padding:0.5rem;margin-bottom:1.5rem;background:var(--surface2);border:1px solid var(--border);border-radius:6px;color:var(--text);">
            <div style="display:flex;gap:0.75rem;justify-content:flex-end;">
                <button class="btn btn-ghost" onclick="closeReschedule()">Cancel</button>
                <button class="btn btn-success" onclick="submitReschedule()">Reschedule</button>
            </div>
        </div>
    </div>

    <script>
    let camStream = null;
    let camVisitId = null;
    let camRow = null;
    let photoBlob = null;
    let photoUploaded = false;

    function openCamera(visitId, visitorName, btn) {{
        camVisitId = visitId;
        camRow = btn.closest('tr');
        photoBlob = null;
        photoUploaded = false;
        document.getElementById('camera-title').textContent = 'Photo: ' + visitorName;
        document.getElementById('camera-modal').style.display = 'flex';
        // Reset to step 1
        document.getElementById('step-camera').style.display = '';
        document.getElementById('step-preview').style.display = 'none';
        document.getElementById('modal-inner').style.maxWidth = '520px';
        document.getElementById('camera-preview').style.display = 'none';
        document.getElementById('camera-video').style.display = 'block';
        document.getElementById('btn-capture').style.display = '';
        document.getElementById('btn-retake').style.display = 'none';
        document.getElementById('btn-next').textContent = 'Next: Preview Badge';
        // Populate camera selector then start
        populateCameraList().then(() => startCamera());
    }}

    async function populateCameraList() {{
        try {{
            const tempStream = await navigator.mediaDevices.getUserMedia({{ video: true, audio: false }});
            tempStream.getTracks().forEach(t => t.stop());

            const devices = await navigator.mediaDevices.enumerateDevices();
            const videoDevices = devices.filter(d => d.kind === 'videoinput');
            const sel = document.getElementById('camera-select');
            if (!sel) return;
            sel.innerHTML = '';
            const savedId = localStorage.getItem('gk_preferred_camera') || '';
            videoDevices.forEach((d, i) => {{
                const opt = document.createElement('option');
                opt.value = d.deviceId;
                opt.textContent = d.label || ('Camera ' + (i + 1));
                if (d.deviceId === savedId) opt.selected = true;
                sel.appendChild(opt);
            }});
            sel.parentElement.style.display = videoDevices.length > 1 ? '' : 'none';
        }} catch(e) {{
            const sel = document.getElementById('camera-select');
            if (sel) sel.parentElement.style.display = 'none';
        }}
    }}

    function startCamera() {{
        if (camStream) {{
            camStream.getTracks().forEach(t => t.stop());
            camStream = null;
        }}
        const sel = document.getElementById('camera-select');
        const deviceId = sel && sel.value ? sel.value : undefined;
        const constraints = {{
            video: deviceId
                ? {{ deviceId: {{ exact: deviceId }}, width: 480, height: 360 }}
                : {{ facingMode: 'user', width: 480, height: 360 }},
            audio: false
        }};
        navigator.mediaDevices.getUserMedia(constraints)
            .then(stream => {{
                camStream = stream;
                document.getElementById('camera-video').srcObject = stream;
                if (deviceId) localStorage.setItem('gk_preferred_camera', deviceId);
            }})
            .catch(() => {{
                // Camera unavailable — go straight to preview without photo
                goToPreview();
            }});
    }}

    function capturePhoto() {{
        const video = document.getElementById('camera-video');
        const canvas = document.getElementById('camera-canvas');
        canvas.getContext('2d').drawImage(video, 0, 0, 480, 360);
        const dataUrl = canvas.toDataURL('image/jpeg', 0.85);
        const preview = document.getElementById('camera-preview');
        preview.src = dataUrl;
        preview.style.display = 'block';
        video.style.display = 'none';
        document.getElementById('btn-capture').style.display = 'none';
        document.getElementById('btn-retake').style.display = '';
        document.getElementById('btn-next').textContent = 'Next: Preview Badge';
        canvas.toBlob(b => {{ photoBlob = b; }}, 'image/jpeg', 0.85);
        if (camStream) {{
            camStream.getTracks().forEach(t => t.stop());
        }}
    }}

    function retakePhoto() {{
        document.getElementById('camera-preview').style.display = 'none';
        document.getElementById('camera-video').style.display = 'block';
        document.getElementById('btn-capture').style.display = '';
        document.getElementById('btn-retake').style.display = 'none';
        photoBlob = null;
        photoUploaded = false;
        startCamera();
    }}

    async function uploadPhotoIfNeeded() {{
        if (!photoBlob || photoUploaded) return;
        try {{
            const resp = await fetch('/api/visits/' + camVisitId + '/visitor-id');
            const data = await resp.json();
            if (data.visitor_id) {{
                await fetch('/api/visitors/' + data.visitor_id + '/photo', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'image/jpeg' }},
                    body: photoBlob
                }});
                photoUploaded = true;
            }}
        }} catch(e) {{
            console.error('Photo upload failed:', e);
        }}
    }}

    async function goToPreview() {{
        // Stop camera
        if (camStream) {{
            camStream.getTracks().forEach(t => t.stop());
            camStream = null;
        }}
        // Upload photo first so the badge preview shows it
        await uploadPhotoIfNeeded();
        // Switch to step 2
        document.getElementById('step-camera').style.display = 'none';
        document.getElementById('step-preview').style.display = '';
        document.getElementById('modal-inner').style.maxWidth = '520px';
        // Load badge preview (with ?preview=1 to skip auto-print)
        document.getElementById('badge-preview-frame').src = '/badge/' + camVisitId + '?preview=1';
    }}

    function goBackToCamera() {{
        photoBlob = null;
        photoUploaded = false;
        document.getElementById('step-preview').style.display = 'none';
        document.getElementById('step-camera').style.display = '';
        document.getElementById('modal-inner').style.maxWidth = '520px';
        document.getElementById('camera-preview').style.display = 'none';
        document.getElementById('camera-video').style.display = 'block';
        document.getElementById('btn-capture').style.display = '';
        document.getElementById('btn-retake').style.display = 'none';
        document.getElementById('badge-preview-frame').src = '';
        populateCameraList().then(() => startCamera());
    }}

    function approveAndPrint() {{
        const vid = camVisitId;
        const row = camRow;
        // Open badge window immediately (on user click) to avoid popup blocker
        const badgeWin = window.open('about:blank', '_blank');
        // Check in the visitor, then navigate the window to the badge
        fetch('/api/visits/' + vid + '/checkin', {{ method: 'POST' }})
            .then(r => r.text())
            .then(html => {{
                closeCamera();
                if (row) {{ row.outerHTML = html; }}
                if (badgeWin) {{
                    badgeWin.location.href = '/badge/' + vid;
                }}
            }});
    }}

    function markLate(visitId, minutes, btn) {{
        const row = btn.closest('tr');
        fetch('/api/visits/' + visitId + '/late', {{
            method: 'POST',
            headers: {{ 'Content-Type': 'application/x-www-form-urlencoded' }},
            body: 'delay_minutes=' + minutes
        }})
        .then(r => r.text())
        .then(html => {{ if (row) row.outerHTML = html; }});
    }}

    function openReschedule(visitId, btn) {{
        const row = btn.closest('tr');
        const modal = document.getElementById('reschedule-modal');
        document.getElementById('reschedule-visit-id').value = visitId;
        document.getElementById('reschedule-date').value = '';
        document.getElementById('reschedule-time').value = '';
        modal.dataset.row = '';
        modal._row = row;
        modal.style.display = 'flex';
    }}

    function submitReschedule() {{
        const modal = document.getElementById('reschedule-modal');
        const visitId = document.getElementById('reschedule-visit-id').value;
        const date = document.getElementById('reschedule-date').value;
        const time = document.getElementById('reschedule-time').value;
        if (!date) {{ alert('Please select a date.'); return; }}
        const row = modal._row;
        fetch('/api/visits/' + visitId + '/reschedule', {{
            method: 'POST',
            headers: {{ 'Content-Type': 'application/x-www-form-urlencoded' }},
            body: 'new_date=' + encodeURIComponent(date) + '&new_time=' + encodeURIComponent(time)
        }})
        .then(r => r.text())
        .then(html => {{
            if (row) row.outerHTML = html;
            closeReschedule();
        }});
    }}

    function closeReschedule() {{
        document.getElementById('reschedule-modal').style.display = 'none';
    }}

    function closeCamera() {{
        if (camStream) {{
            camStream.getTracks().forEach(t => t.stop());
            camStream = null;
        }}
        photoBlob = null;
        document.getElementById('camera-modal').style.display = 'none';
    }}

    function checkInGroup(visitId, btn) {{
        const row = btn.closest('tr');
        const badgeWin = window.open('about:blank', '_blank');
        fetch('/api/visits/' + visitId + '/checkin', {{ method: 'POST' }})
            .then(r => r.text())
            .then(html => {{
                if (row) {{ row.outerHTML = html; }}
                if (badgeWin) {{ badgeWin.location.href = '/badge/' + visitId; }}
            }});
    }}
    </script>
</body>
</html>"##, title = title, content = content, theme_attr = theme_attr, sidebar_nav = sidebar_nav())
}

/// Wrapper that uses the current thread-local theme and role
pub fn layout(title: &str, content: &str) -> String {
    let theme = get_theme();
    layout_with_theme(title, content, &theme)
}

/// Build sidebar nav based on user role
fn sidebar_nav() -> String {
    let admin_only = if is_admin() {
        r#"<a href="/admin" style="margin-top:1rem;border-top:1px solid var(--border);padding-top:0.75rem;">Admin Panel</a>"#
    } else {
        ""
    };
    format!(r#"
        <a href="/" class="active">Dashboard</a>
        <a href="/pre-register">Pre-Register Visitor</a>
        <a href="/walk-in">Walk-In Check-In</a>
        <a href="/group-visit">Group Visit</a>
        <a href="/hosts">Manage Hosts</a>
        <a href="/log">Visitor Log</a>
        {admin_only}
        <form method="POST" action="/logout" style="margin-top:auto;padding-top:1rem;">
            <button type="submit" style="background:none;border:none;color:var(--text-dim);cursor:pointer;font-size:0.85rem;padding:0.25rem 0;">Logout</button>
        </form>
    "#)
}

/// Dashboard page — today's visits + stats
pub fn dashboard_page(
    visits: &[VisitDetail],
    upcoming: &[VisitDetail],
    graph_connected: bool,
) -> String {
    let total = visits.len();
    let checked_in = visits.iter().filter(|v| v.status == "checked_in").count();
    let pending = visits.iter().filter(|v| v.status == "pending").count();
    let walk_ins = visits.iter().filter(|v| !v.pre_registered).count();

    let calendar_badge = if graph_connected {
        r#"<span style="color:var(--green);font-weight:600;">Connected</span>"#
    } else {
        r#"<span style="color:var(--text-dim);">Disabled</span>"#
    };

    let stats = format!(r##"
        <div class="stats">
            <div class="stat"><div class="label">Today's Visitors</div><div class="value">{total}</div></div>
            <div class="stat"><div class="label">Currently On-Site</div><div class="value" style="color:var(--green)">{checked_in}</div></div>
            <div class="stat"><div class="label">Expected</div><div class="value" style="color:var(--text-dim)">{pending}</div></div>
            <div class="stat"><div class="label">Walk-Ins</div><div class="value" style="color:var(--orange)">{walk_ins}</div></div>
            <div class="stat"><div class="label">Calendar</div><div class="value" style="font-size:0.95rem;">{calendar_badge}</div></div>
        </div>
    "##);

    let checkout_all_btn = if checked_in > 0 {
        r##"<button class="btn" style="background:var(--orange);color:#fff;font-size:0.85rem;padding:0.4rem 1rem;margin-left:1rem;"
                    hx-post="/api/visits/checkout-all"
                    hx-target="#today-table"
                    hx-swap="innerHTML"
                    hx-confirm="Check out all on-site visitors?">
                Check Out All
            </button>"##
    } else {
        ""
    };

    let today_rows = render_visit_rows(visits, true);
    let upcoming_rows = render_visit_rows(upcoming, false);

    let content = format!(r##"
        <h2>Dashboard</h2>
        {stats}
        <div class="card">
            <h3 style="display:inline-block;">Today's Activity</h3>{checkout_all_btn}
            <div id="today-table" hx-get="/api/dashboard/today" hx-trigger="every 30s" hx-swap="innerHTML">
                {today_rows}
            </div>
        </div>
        <div class="card">
            <h3>Upcoming</h3>
            {upcoming_rows}
        </div>
    "##);

    layout("Dashboard", &content)
}

/// Pre-registration form page
pub fn pre_register_page(_hosts: &[Host], purposes: &str, areas: &str, visitor_types: &str) -> String {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let purpose_buttons = build_purpose_buttons(purposes);
    let area_options = build_area_options(areas);
    let type_options = build_visitor_type_options(visitor_types);
    let content = format!(r##"
        <h2>Pre-Register a Visitor</h2>
        <div class="card">
            <form id="prereg-form" hx-post="/api/pre-register" hx-target="#form-result" hx-swap="innerHTML">
                <div id="form-result"></div>

                <!-- Step 1: Who's visiting? (with autocomplete) -->
                <div class="form-group" style="position:relative;">
                    <label>Who's visiting? *</label>
                    <input type="text" id="visitor-search" name="visitor_name" required
                           placeholder="Start typing visitor name..."
                           autocomplete="off" autofocus
                           style="font-size:1.1rem;padding:0.75rem;">
                    <div id="visitor-dropdown" class="typeahead-dropdown" style="display:none;"></div>
                </div>

                <!-- Auto-filled visitor details (collapsed until filled or expanded) -->
                <div id="visitor-details" class="visitor-detail-row">
                    <div class="form-group">
                        <label>Company</label>
                        <input type="text" name="visitor_company" id="visitor-company" placeholder="Company">
                    </div>
                    <div class="form-group">
                        <label>Email</label>
                        <input type="email" name="visitor_email" id="visitor-email"
                               placeholder="visitor@company.com">
                    </div>
                    <div class="form-group">
                        <label>Phone</label>
                        <input type="tel" name="visitor_phone" id="visitor-phone"
                               placeholder="(555) 555-1234">
                    </div>
                </div>

                <!-- Step 2: Who are they seeing? -->
                <div class="form-group" style="position:relative;">
                    <label>Who are they seeing? *</label>
                    <input type="text" id="host-search" placeholder="Start typing host name..."
                           autocomplete="off" style="font-size:1.1rem;padding:0.75rem;">
                    <input type="hidden" name="host_id" id="host-id" required>
                    <div id="host-dropdown" class="typeahead-dropdown" style="display:none;"></div>
                </div>

                <!-- Step 3: Quick purpose (tap buttons or type custom) -->
                <div class="form-group">
                    <label>Purpose *</label>
                    <div class="quick-purposes">
                        {purpose_buttons}
                    </div>
                    <input type="text" name="purpose" id="purpose-input" required
                           placeholder="Or type a custom purpose..." style="margin-top:0.5rem;">
                </div>

                <!-- Visitor Type -->
                <div class="form-group">
                    <label>Visitor Type</label>
                    <select name="visitor_type">
                        {type_options}
                    </select>
                </div>

                <!-- Special Notes -->
                <div class="form-group full">
                    <label>Special Notes</label>
                    <textarea name="special_notes" rows="2"
                              placeholder="Parking needs, equipment, dietary restrictions, NDA required, etc."
                              style="width:100%;resize:vertical;"></textarea>
                </div>

                <!-- Step 4: When? -->
                <div class="form-group">
                    <label>When?</label>
                    <div class="when-row">
                        <div class="when-field">
                            <label class="sub-label">Date</label>
                            <input type="date" name="expected_date" required value="{today}">
                        </div>
                        <div class="when-field">
                            <label class="sub-label">Arrival</label>
                            <input type="time" name="expected_time" id="expected-time" value="09:00">
                        </div>
                        <div class="when-field">
                            <label class="sub-label">Duration</label>
                            <select name="duration" id="duration-select">
                                <option value="30">30 min</option>
                                <option value="60" selected>1 hour</option>
                                <option value="90">1.5 hours</option>
                                <option value="120">2 hours</option>
                                <option value="180">3 hours</option>
                                <option value="240">Half day</option>
                                <option value="480">Full day</option>
                            </select>
                        </div>
                        <div class="when-field">
                            <label class="sub-label">Areas</label>
                            <select name="areas_requested">
                                {area_options}
                            </select>
                        </div>
                    </div>
                    <div id="time-summary" class="time-summary"></div>
                </div>

                <div style="margin-top:1.25rem;">
                    <button type="submit" class="btn btn-primary" style="font-size:1.1rem;padding:0.75rem 2rem;">
                        Register Visitor
                        <span class="htmx-indicator"><span class="spinner"></span></span>
                    </button>
                </div>
            </form>
        </div>

        <style>
            .quick-purposes {{
                display: flex; flex-wrap: wrap; gap: 0.5rem;
            }}
            .purpose-btn {{
                padding: 0.5rem 1rem;
                border-radius: 20px;
                border: 1px solid var(--border);
                background: var(--surface2);
                color: var(--text);
                cursor: pointer;
                font-size: 0.9rem;
                transition: all 0.15s;
            }}
            .purpose-btn:hover {{ border-color: var(--accent); color: var(--accent); }}
            .purpose-btn.selected {{
                background: var(--accent);
                color: #fff;
                border-color: var(--accent);
            }}
            .typeahead-dropdown {{
                position: absolute;
                top: 100%;
                left: 0; right: 0;
                background: var(--surface);
                border: 1px solid var(--border);
                border-radius: 0 0 8px 8px;
                max-height: 220px;
                overflow-y: auto;
                z-index: 100;
                box-shadow: 0 8px 24px rgba(0,0,0,0.4);
            }}
            .typeahead-item {{
                padding: 0.65rem 0.75rem;
                cursor: pointer;
                border-bottom: 1px solid var(--border);
                font-size: 0.9rem;
            }}
            .typeahead-item:hover, .typeahead-item.active {{
                background: var(--surface2);
            }}
            .typeahead-item .sub {{ color: var(--text-dim); font-size: 0.8rem; }}
            .visitor-detail-row {{
                display: grid;
                grid-template-columns: 1fr 1fr 1fr;
                gap: 0.75rem;
                margin-bottom: 1rem;
                padding: 0.75rem;
                background: var(--bg);
                border-radius: 8px;
                border: 1px solid var(--border);
            }}
            .when-row {{
                display: grid;
                grid-template-columns: 1fr 1fr 1fr 1fr;
                gap: 0.75rem;
                margin-top: 0.5rem;
            }}
            .when-field label.sub-label {{
                font-size: 0.75rem;
                color: var(--text-dim);
                margin-bottom: 0.25rem;
            }}
            .time-summary {{
                margin-top: 0.5rem;
                padding: 0.5rem 0.75rem;
                background: rgba(79,140,255,0.1);
                border-radius: 6px;
                font-size: 0.85rem;
                color: var(--accent);
                display: none;
            }}
            .time-summary.visible {{ display: block; }}
            @media (max-width: 768px) {{
                .visitor-detail-row {{ grid-template-columns: 1fr; }}
                .when-row {{ grid-template-columns: 1fr 1fr; }}
            }}
        </style>

        <script>
        // ── Time summary ──
        function updateTimeSummary() {{
            const timeInput = document.getElementById('expected-time');
            const durSelect = document.getElementById('duration-select');
            const summary = document.getElementById('time-summary');
            const t = timeInput.value;
            if (!t) {{ summary.className = 'time-summary'; return; }}
            const [h, m] = t.split(':').map(Number);
            const dur = parseInt(durSelect.value);
            const startDate = new Date(2000, 0, 1, h, m);
            const endDate = new Date(startDate.getTime() + dur * 60000);
            const fmt = d => d.toLocaleTimeString('en-US', {{ hour: 'numeric', minute: '2-digit' }});
            summary.textContent = fmt(startDate) + ' – ' + fmt(endDate) +
                (dur >= 240 ? ' (' + (dur/60) + ' hours)' : '');
            summary.className = 'time-summary visible';
        }}
        document.getElementById('expected-time').addEventListener('change', updateTimeSummary);
        document.getElementById('duration-select').addEventListener('change', updateTimeSummary);

        // Set default arrival to next half-hour
        (function() {{
            const now = new Date();
            let h = now.getHours(), m = now.getMinutes();
            m = m < 30 ? 30 : 0;
            if (m === 0) h++;
            if (h > 17) {{ h = 9; m = 0; }} // after 5 PM, default to 9 AM tomorrow
            const timeInput = document.getElementById('expected-time');
            timeInput.value = String(h).padStart(2, '0') + ':' + String(m).padStart(2, '0');
            updateTimeSummary();
        }})();

        // ── Visitor typeahead ──
        let visitorTimer = null;
        const visitorInput = document.getElementById('visitor-search');
        const visitorDrop = document.getElementById('visitor-dropdown');

        visitorInput.addEventListener('input', function() {{
            clearTimeout(visitorTimer);
            const q = this.value.trim();
            if (q.length < 2) {{ visitorDrop.style.display = 'none'; return; }}
            visitorTimer = setTimeout(() => {{
                fetch('/api/visitors/search?q=' + encodeURIComponent(q))
                    .then(r => r.json())
                    .then(results => {{
                        if (results.length === 0) {{
                            visitorDrop.style.display = 'none';
                            return;
                        }}
                        visitorDrop.innerHTML = results.map(v =>
                            `<div class="typeahead-item" onclick="pickVisitor(this)"
                                  data-name="${{v.name}}"
                                  data-company="${{v.company || ''}}"
                                  data-email="${{v.email || ''}}"
                                  data-phone="${{v.phone || ''}}">
                                <div>${{v.name}}</div>
                                <div class="sub">${{v.company || 'No company'}}</div>
                            </div>`
                        ).join('');
                        visitorDrop.style.display = 'block';
                    }});
            }}, 200);
        }});

        function pickVisitor(el) {{
            visitorInput.value = el.dataset.name;
            document.getElementById('visitor-company').value = el.dataset.company;
            document.getElementById('visitor-email').value = el.dataset.email;
            document.getElementById('visitor-phone').value = el.dataset.phone;
            visitorDrop.style.display = 'none';
        }}

        // ── Host typeahead ──
        let hostTimer = null;
        const hostInput = document.getElementById('host-search');
        const hostDrop = document.getElementById('host-dropdown');
        const hostIdInput = document.getElementById('host-id');

        // Show all hosts on focus
        hostInput.addEventListener('focus', function() {{
            if (!this.value.trim()) {{
                fetchHosts('');
            }}
        }});

        hostInput.addEventListener('input', function() {{
            clearTimeout(hostTimer);
            hostIdInput.value = '';
            const q = this.value.trim();
            hostTimer = setTimeout(() => fetchHosts(q), 150);
        }});

        function fetchHosts(q) {{
            fetch('/api/hosts/search?q=' + encodeURIComponent(q))
                .then(r => r.json())
                .then(results => {{
                    if (results.length === 0) {{
                        hostDrop.style.display = 'none';
                        return;
                    }}
                    hostDrop.innerHTML = results.map(h =>
                        `<div class="typeahead-item" onclick="pickHost(this)"
                              data-id="${{h.id}}" data-name="${{h.name}}" data-dept="${{h.department}}">
                            <div>${{h.name}}</div>
                            <div class="sub">${{h.department}}</div>
                        </div>`
                    ).join('');
                    hostDrop.style.display = 'block';
                }});
        }}

        function pickHost(el) {{
            hostInput.value = el.dataset.name + ' — ' + el.dataset.dept;
            hostIdInput.value = el.dataset.id;
            hostDrop.style.display = 'none';
        }}

        // ── Purpose quick-pick ──
        function pickPurpose(btn) {{
            document.querySelectorAll('.purpose-btn').forEach(b => b.classList.remove('selected'));
            btn.classList.add('selected');
            document.getElementById('purpose-input').value = btn.textContent;
        }}

        // Close dropdowns on outside click
        document.addEventListener('click', function(e) {{
            if (!e.target.closest('#visitor-search') && !e.target.closest('#visitor-dropdown'))
                visitorDrop.style.display = 'none';
            if (!e.target.closest('#host-search') && !e.target.closest('#host-dropdown'))
                hostDrop.style.display = 'none';
        }});

        // Form validation — ensure host is selected
        document.getElementById('prereg-form').addEventListener('htmx:configRequest', function(e) {{
            if (!hostIdInput.value) {{
                e.preventDefault();
                hostInput.focus();
                hostInput.style.borderColor = 'var(--red)';
                setTimeout(() => hostInput.style.borderColor = '', 2000);
            }}
        }});
        </script>
    "##, today = today);

    layout("Pre-Register", &content)
}

/// Walk-in check-in form
pub fn walk_in_page(hosts: &[Host], areas: &str, visitor_types: &str) -> String {
    let host_options = hosts_to_options(hosts);
    let area_options = build_area_options(areas);
    let type_options = build_visitor_type_options(visitor_types);
    let content = format!(r##"
        <h2>Walk-In Check-In</h2>
        <div class="alert alert-warning">⚠ Unannounced visitor — host will be notified for approval</div>
        <div class="card">
            <form hx-post="/api/walk-in" hx-target="#form-result" hx-swap="innerHTML">
                <div id="form-result"></div>
                <div class="form-grid">
                    <div class="form-group">
                        <label>Visitor Name *</label>
                        <input type="text" name="visitor_name" required placeholder="Full name" autofocus>
                    </div>
                    <div class="form-group">
                        <label>Company</label>
                        <input type="text" name="visitor_company" placeholder="Company or organization">
                    </div>
                    <div class="form-group">
                        <label>Phone</label>
                        <input type="tel" name="visitor_phone" placeholder="(555) 555-1234">
                    </div>
                    <div class="form-group">
                        <label>Here to See *</label>
                        <select name="host_id" required>
                            <option value="">Select a host...</option>
                            {host_options}
                        </select>
                    </div>
                    <div class="form-group full">
                        <label>Purpose *</label>
                        <input type="text" name="purpose" required placeholder="e.g., Unscheduled vendor visit, Interview">
                    </div>
                    <div class="form-group">
                        <label>Visitor Type</label>
                        <select name="visitor_type">
                            {type_options}
                        </select>
                    </div>
                    <div class="form-group">
                        <label>Areas Requested</label>
                        <select name="areas_requested">
                            {area_options}
                        </select>
                    </div>
                </div>
                <div class="form-group full">
                    <label>Special Notes</label>
                    <textarea name="special_notes" rows="2"
                              placeholder="Parking needs, equipment, special instructions..."
                              style="width:100%;resize:vertical;"></textarea>
                </div>
                <div style="margin-top:1rem;">
                    <button type="submit" class="btn btn-primary">
                        Check In &amp; Notify Host
                        <span class="htmx-indicator"><span class="spinner"></span></span>
                    </button>
                </div>
            </form>
        </div>
    "##);

    layout("Walk-In", &content)
}

/// Host management page
pub fn hosts_page(hosts: &[Host]) -> String {
    let host_rows: String = hosts.iter().map(|h| {
        let phone = h.phone.as_deref().unwrap_or("");
        let phone_display = if phone.is_empty() { "—" } else { phone };
        // Escape for HTML content and JS string contexts
        let name_html = html_escape(&h.name);
        let dept_html = html_escape(&h.department);
        let email_html = html_escape(&h.email);
        let phone_html = html_escape(phone_display);
        // For JS string literals inside onclick, escape both HTML and JS
        let js_escape = |s: &str| html_escape(&s.replace('\\', "\\\\").replace('\'', "\\'"));
        let name_js = js_escape(&h.name);
        let dept_js = js_escape(&h.department);
        let email_js = js_escape(&h.email);
        let phone_js = js_escape(phone);
        format!(r##"<tr id="host-row-{id}">
            <td>{name_html}</td>
            <td>{dept_html}</td>
            <td>{email_html}</td>
            <td>{phone_html}</td>
            <td class="actions">
                <button class="btn btn-ghost btn-sm" onclick="editHost('{id}','{name_js}','{dept_js}','{email_js}','{phone_js}')">Edit</button>
                <button class="btn btn-danger btn-sm"
                        hx-delete="/api/hosts/{id}" hx-target="#host-row-{id}" hx-swap="outerHTML"
                        hx-confirm="Remove {name_html}? They won&#x27;t receive visitor notifications anymore.">Remove</button>
            </td>
        </tr>"##,
            id = h.id,
        )
    }).collect();

    let content = format!(r##"
        <h2>Manage Hosts</h2>
        <div class="card">
            <h3 id="host-form-title">Add New Host</h3>
            <form id="host-form" hx-post="/api/hosts" hx-target="#host-result" hx-swap="innerHTML"
                  hx-on::after-request="if(event.detail.successful) setTimeout(()=>location.reload(), 800)">
                <div id="host-result"></div>
                <input type="hidden" name="host_id" id="edit-host-id" value="">
                <div class="form-grid">
                    <div class="form-group">
                        <label>Name *</label>
                        <input type="text" name="name" id="host-name" required placeholder="Staff member's name">
                    </div>
                    <div class="form-group">
                        <label>Department *</label>
                        <select name="department" id="host-dept" required>
                            <option value="">Select...</option>
                            <option value="Engineering">Engineering</option>
                            <option value="News">News</option>
                            <option value="Sales">Sales</option>
                            <option value="Programming">Programming</option>
                            <option value="Creative Services">Creative Services</option>
                            <option value="IT">IT</option>
                            <option value="Management">Management</option>
                            <option value="Other">Other</option>
                        </select>
                    </div>
                    <div class="form-group">
                        <label>Email *</label>
                        <input type="email" name="email" id="host-email" required placeholder="name@wbbh.com">
                    </div>
                    <div class="form-group">
                        <label>Phone (for SMS alerts)</label>
                        <input type="tel" name="phone" id="host-phone" placeholder="+1 (555) 555-1234">
                    </div>
                </div>
                <div style="margin-top:0.5rem;display:flex;gap:0.5rem;">
                    <button type="submit" class="btn btn-primary" id="host-submit-btn">Add Host</button>
                    <button type="button" class="btn btn-ghost" id="host-cancel-btn" onclick="cancelEdit()" style="display:none;">Cancel</button>
                </div>
            </form>
        </div>
        <div class="card">
            <h3>Current Hosts</h3>
            <table>
                <thead><tr><th>Name</th><th>Department</th><th>Email</th><th>Phone</th><th>Actions</th></tr></thead>
                <tbody>{host_rows}</tbody>
            </table>
        </div>

        <script>
        function editHost(id, name, dept, email, phone) {{
            document.getElementById('edit-host-id').value = id;
            document.getElementById('host-name').value = name;
            document.getElementById('host-email').value = email;
            document.getElementById('host-phone').value = phone;
            // Set department select
            const deptSel = document.getElementById('host-dept');
            for (let opt of deptSel.options) {{
                opt.selected = (opt.value === dept);
            }}
            // Switch form to edit mode
            document.getElementById('host-form-title').textContent = 'Edit Host';
            document.getElementById('host-submit-btn').textContent = 'Save Changes';
            document.getElementById('host-cancel-btn').style.display = '';
            const form = document.getElementById('host-form');
            form.setAttribute('hx-post', '/api/hosts/' + id);
            htmx.process(form);
            // Scroll to form
            form.scrollIntoView({{ behavior: 'smooth' }});
        }}

        function cancelEdit() {{
            document.getElementById('edit-host-id').value = '';
            document.getElementById('host-name').value = '';
            document.getElementById('host-email').value = '';
            document.getElementById('host-phone').value = '';
            document.getElementById('host-dept').selectedIndex = 0;
            document.getElementById('host-form-title').textContent = 'Add New Host';
            document.getElementById('host-submit-btn').textContent = 'Add Host';
            document.getElementById('host-cancel-btn').style.display = 'none';
            const form = document.getElementById('host-form');
            form.setAttribute('hx-post', '/api/hosts');
            htmx.process(form);
        }}
        </script>
    "##);

    layout("Hosts", &content)
}

/// Visitor log / search page
pub fn log_page(visits: &[VisitDetail]) -> String {
    let rows = render_visit_rows(visits, false);
    let content = format!(r##"
        <h2>Visitor Log</h2>
        <div class="search-bar">
            <div class="form-group" style="flex:1">
                <label>Search</label>
                <input type="text" name="q" placeholder="Visitor name, company, host, or purpose..."
                       hx-get="/api/log/search" hx-trigger="keyup changed delay:300ms" hx-target="#log-results"
                       hx-include="[name='from'],[name='to']">
            </div>
            <div class="form-group">
                <label>From</label>
                <input type="date" name="from">
            </div>
            <div class="form-group">
                <label>To</label>
                <input type="date" name="to">
            </div>
        </div>
        <div class="card">
            <div id="log-results">
                {rows}
            </div>
        </div>
    "##);

    layout("Visitor Log", &content)
}

/// Badge rendering options (collected from admin settings)
pub struct BadgeOpts<'a> {
    pub company_name: &'a str,
    pub expiry_text: &'a str,
    pub primary_color: &'a str,
    pub logo_filename: Option<&'a str>,
    pub footer_text: &'a str,
    pub badge_type_label: &'a str,
    pub badge_label_color: &'a str, // "primary" = use primary_color, otherwise treat as hex
    pub show_purpose: bool,
    pub show_areas: bool,
    pub show_badge_number: bool,
    pub show_escort: bool,
    pub font_name_pt: u8,    // visitor name font size in pt
    pub font_company_pt: u8, // company line font size in pt
    pub font_detail_pt: u8,  // detail rows font size in pt
    pub line_spacing: u8,    // line spacing in px (0=tight, 4=normal, 8=loose)
}

/// Print-ready visitor badge (opens in new tab, auto-prints unless preview mode)
pub fn badge_page(
    v: &VisitDetail,
    photo_filename: Option<&str>,
    opts: &BadgeOpts,
) -> String {
    badge_page_inner(v, photo_filename, opts, false)
}

pub fn badge_page_preview(
    v: &VisitDetail,
    photo_filename: Option<&str>,
    opts: &BadgeOpts,
) -> String {
    badge_page_inner(v, photo_filename, opts, true)
}

fn badge_page_inner(
    v: &VisitDetail,
    photo_filename: Option<&str>,
    opts: &BadgeOpts,
    preview_only: bool,
) -> String {
    let company_name = opts.company_name;
    let expiry_text = opts.expiry_text;
    let primary_color = opts.primary_color;
    let logo_filename = opts.logo_filename;
    let footer_text = opts.footer_text;
    let badge_type_label = opts.badge_type_label;
    let label_color = if opts.badge_label_color == "primary" || opts.badge_label_color.is_empty() {
        primary_color
    } else {
        opts.badge_label_color
    };
    // Use per-visit visitor_type, fall back to global badge_type_label
    let badge_label = if v.visitor_type.is_empty() || v.visitor_type == "Visitor" {
        badge_type_label
    } else {
        &v.visitor_type
    };
    let visitor_company = html_escape(v.visitor.company.as_deref().unwrap_or(""));
    let areas = v.areas_requested.as_deref().unwrap_or("General");
    let badge_num = v.badge_number.as_deref().unwrap_or("—");
    let date_raw = v.expected_date.as_deref()
        .or(v.check_in.as_deref())
        .unwrap_or(&v.created_at);
    // Format date nicely: "2026-03-10" → "March 10, 2026"
    let date = chrono::NaiveDate::parse_from_str(
        &date_raw.chars().take(10).collect::<String>(), "%Y-%m-%d"
    )
        .map(|d| d.format("%B %-d, %Y").to_string())
        .unwrap_or_else(|_| date_raw.to_string());
    let checkin_time = v.check_in.as_deref().unwrap_or("—");
    let visit_id_short = if v.id.len() >= 8 { &v.id[..8] } else { &v.id };
    let escort = if opts.show_escort && areas != "General" && areas != "Lobby" && areas != "Main Lobby" {
        "ESCORT REQUIRED"
    } else {
        ""
    };

    let photo_html = match photo_filename {
        Some(f) => format!(
            r#"<div class="photo-col">
                <img class="photo" src="/photos/{}" alt="Visitor photo">
            </div>"#,
            f
        ),
        None => r#"<div class="photo-col">
            <div class="photo-placeholder">NO<br>PHOTO</div>
        </div>"#.to_string(),
    };

    // Build detail rows — upper rows go next to photo, lower rows span full width
    let mut upper_rows = String::new();
    upper_rows.push_str(&format!(
        r#"<tr><td class="label">Host:</td><td class="value">{} ({})</td></tr>"#,
        html_escape(&v.host.name), html_escape(&v.host.department)
    ));
    if opts.show_purpose {
        upper_rows.push_str(&format!(
            r#"<tr><td class="label">Purpose:</td><td class="value">{}</td></tr>"#,
            html_escape(&v.purpose)
        ));
    }
    if opts.show_areas {
        upper_rows.push_str(&format!(
            r#"<tr><td class="label">Areas:</td><td class="value">{}</td></tr>"#,
            html_escape(areas)
        ));
    }

    let mut lower_rows = String::new();
    lower_rows.push_str(&format!(
        r#"<tr><td class="label">Checked In:</td><td class="value">{}</td></tr>"#,
        checkin_time
    ));
    if opts.show_badge_number {
        lower_rows.push_str(&format!(
            r#"<tr><td class="label">Badge #:</td><td class="value">{}</td></tr>"#,
            badge_num
        ));
    }

    let escort_html = if !escort.is_empty() {
        format!(r#"<div class="escort">{}</div>"#, escort)
    } else {
        String::new()
    };

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Visitor Badge — {name}</title>
<style>
@page {{ size: 4in 2.4in; margin: 0; }}
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
body {{
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
    width: 4in; height: 2.4in;
    display: flex; flex-direction: column;
    padding: 0;
    overflow: hidden;
}}
/* Top banner */
.banner {{
    background: {primary_color}; color: #fff;
    padding: 4px 12px;
    display: flex; justify-content: space-between; align-items: center;
}}
.banner .org {{ font-size: 11pt; font-weight: 700; letter-spacing: 1px; }}
.banner .type {{ font-size: 14pt; font-weight: 800; text-transform: uppercase;
    background: #fff; color: {label_color}; padding: 2px 14px; border-radius: 4px;
    border: 2px solid #fff; letter-spacing: 1px; }}
/* Body — two-part layout */
.badge-content {{ flex: 1; display: flex; flex-direction: column; overflow: hidden; }}
.badge-upper {{
    display: flex; padding: 0.06in 0.15in 0; gap: 0.1in;
}}
.photo-col {{
    flex: 0 0 1in;
    display: flex; flex-direction: column;
    align-items: center; justify-content: flex-start;
}}
.photo {{
    width: 1in; height: 1.2in;
    object-fit: cover;
    border-radius: 4px;
    border: 1px solid #ccc;
}}
.photo-placeholder {{
    width: 1in; height: 1.2in;
    border: 2px dashed #ccc; border-radius: 4px;
    display: flex; align-items: center; justify-content: center;
    font-size: 8pt; color: #999; text-align: center;
}}
.info-col {{
    flex: 1;
    display: flex; flex-direction: column;
    justify-content: flex-start;
    overflow: hidden;
}}
.name {{ font-size: {font_name}pt; font-weight: 700; line-height: 1.0;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-bottom: {spacing}px; }}
.company {{ font-size: {font_company}pt; color: #666; margin-bottom: {spacing}px; }}
.details {{ font-size: {font_detail}pt; width: 100%; border-top: 1px solid #ddd; padding-top: {spacing}px;
    border-collapse: collapse; }}
.details tr td {{ padding: {half_spacing}px 0; vertical-align: top; line-height: 1.2; }}
.details .label {{ font-weight: 600; color: #333; text-align: right; white-space: nowrap;
    padding-right: 6px; width: 1px; }}
.details .value {{ color: #555; }}
/* Full-width bottom details (Checked In, Badge #) */
.badge-lower {{
    padding: 0 0.15in 0.04in;
    font-size: {font_detail}pt;
}}
.badge-lower .details {{ border-top: 1px solid #ddd; }}
.escort {{
    margin: 2px 0.15in; padding: 2px 6px;
    background: #fee2e2; color: #dc2626;
    font-size: 8pt; font-weight: 700;
    border-radius: 3px; display: inline-block;
    text-transform: uppercase;
}}
/* Footer */
.badge-footer {{
    background: #f3f4f6;
    border-top: 1px solid #ddd;
    padding: 2px 10px;
    display: flex; justify-content: space-between; align-items: center;
    font-size: 7pt; color: #666;
}}
.badge-footer .expiry {{
    font-weight: 700; color: #dc2626; text-transform: uppercase;
}}
@media screen {{
    html {{ display:flex; justify-content:center; align-items:center;
            min-height:100vh; background:#e5e7eb; }}
    body {{ border: 2px solid #ccc; }}
}}
/* Print: force pure black only — no grays that trigger color ink mixing */
@media print {{
    html {{ -webkit-print-color-adjust: exact; print-color-adjust: exact; }}
    body, .name, .company, .details, .details .label, .details .value,
    .badge-lower, .badge-footer, .badge-footer span,
    .badge-footer .expiry, .escort, .photo-placeholder {{
        color: #000 !important;
    }}
    .company {{ opacity: 0.7; }}
    .details .value {{ opacity: 0.8; }}
    .badge-footer {{ background: #fff !important; border-top: 1px solid #000 !important; }}
    .badge-footer span, .badge-footer .expiry {{ color: #000 !important; }}
    .escort {{ background: #fff !important; border: 2px solid #000 !important; }}
    .photo-placeholder {{ border-color: #000 !important; }}
    .details {{ border-top-color: #000 !important; }}
    .badge-lower .details {{ border-top-color: #000 !important; }}
}}
</style>
</head>
<body>
<div class="banner">
    <span class="org">{logo_html}{company_name}</span>
    <span class="type">{badge_label}</span>
</div>
<div class="badge-content">
    <div class="badge-upper">
        {photo}
        <div class="info-col">
            <div class="name">{name}</div>
            <div class="company">{visitor_company}</div>
            <table class="details">
                {upper_rows}
            </table>
        </div>
    </div>
    {escort}
    <div class="badge-lower">
        <table class="details">
            {lower_rows}
        </table>
    </div>
</div>
<div class="badge-footer">
    <span>Visit: {visit_id} | {date}{badge_footer}</span>
    <span class="expiry">{expiry}</span>
</div>
<script>{auto_print_script}</script>
</body>
</html>"#,
        auto_print_script = if preview_only { "" } else { QUANTIZE_PRINT_JS },
        primary_color = primary_color,
        label_color = label_color,
        font_name = opts.font_name_pt,
        font_company = opts.font_company_pt,
        font_detail = opts.font_detail_pt,
        spacing = opts.line_spacing,
        half_spacing = opts.line_spacing / 2,
        company_name = html_escape(company_name),
        logo_html = match logo_filename {
            Some(f) => format!(
                r#"<img src="/photos/{}" alt="" style="height:18px;margin-right:6px;vertical-align:middle;">"#,
                f
            ),
            None => String::new(),
        },
        photo = photo_html,
        name = html_escape(&v.visitor.name),
        visitor_company = visitor_company,
        upper_rows = upper_rows,
        lower_rows = lower_rows,
        escort = escort_html,
        badge_label = badge_label,
        date = date,
        visit_id = visit_id_short,
        badge_footer = if footer_text.is_empty() {
            String::new()
        } else {
            format!(" | {}", footer_text)
        },
        expiry = expiry_text.replace("TODAY", &date),
    )
}

/// Admin panel page
pub fn admin_page(
    settings: &[(String, String)],
    hosts: &[Host],
    stats: (usize, usize, usize),
    graph_status: &str,
) -> String {
    let (host_count, visitor_count, visit_count) = stats;

    let setting_val = |key: &str| -> String {
        settings.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };

    let host_rows: String = hosts.iter().map(|h| {
        format!(r##"<tr>
            <td>{}</td><td>{}</td><td>{}</td><td>{}</td>
            <td><span class="badge {}">{}</span></td>
        </tr>"##,
            h.name, h.department, h.email,
            h.phone.as_deref().unwrap_or("—"),
            if h.active { "checked_in" } else { "denied" },
            if h.active { "Active" } else { "Inactive" },
        )
    }).collect();

    let graph_badge = match graph_status {
        "connected" => r#"<span class="badge checked_in">Connected</span>"#,
        "disabled" => r#"<span class="badge denied">Disabled — credentials not set</span>"#,
        _ => r#"<span class="badge pending">Unknown</span>"#,
    };

    let content = format!(r##"
        <h2>Admin Panel</h2>

        <!-- System Status -->
        <div class="stats">
            <div class="stat">
                <div class="label">Active Hosts</div>
                <div class="value">{host_count}</div>
            </div>
            <div class="stat">
                <div class="label">Total Visitors</div>
                <div class="value">{visitor_count}</div>
            </div>
            <div class="stat">
                <div class="label">Total Visits</div>
                <div class="value">{visit_count}</div>
            </div>
            <div class="stat">
                <div class="label">Calendar</div>
                <div class="value" style="font-size:0.9rem;">{graph_badge}</div>
            </div>
        </div>

        <!-- Theme -->
        <div class="card">
            <h3>Appearance</h3>
            <form hx-post="/admin/settings/theme" hx-target="#theme-result" hx-swap="innerHTML">
                <div id="theme-result"></div>
                <div style="display:flex;gap:1rem;align-items:center;">
                    <label style="margin-bottom:0;">Color Scheme</label>
                    <select name="ui_theme" style="width:auto;">
                        <option value="system" {theme_system}>System</option>
                        <option value="dark" {theme_dark}>Dark</option>
                        <option value="light" {theme_light}>Light</option>
                    </select>
                    <button type="submit" class="btn btn-primary btn-sm">Apply</button>
                </div>
            </form>
        </div>

        <!-- Dropdown Lists -->
        <div class="card">
            <h3>Form Dropdowns</h3>
            <form hx-post="/admin/settings/dropdowns" hx-target="#dropdown-result" hx-swap="innerHTML">
                <div id="dropdown-result"></div>
                <div class="form-grid">
                    <div class="form-group full">
                        <label>Purpose Options <span style="color:var(--text-dim);font-weight:normal;">(comma-separated)</span></label>
                        <input type="text" name="purpose_list" value="{purpose_list}"
                               placeholder="Meeting, Sales Call, Interview, Tour, Delivery">
                    </div>
                    <div class="form-group full">
                        <label>Area Options <span style="color:var(--text-dim);font-weight:normal;">(comma-separated, &ldquo;Lobby only&rdquo; is always first)</span></label>
                        <input type="text" name="area_list" value="{area_list}"
                               placeholder="Studios, Master Control, Rack Room, Newsroom, Offices">
                    </div>
                    <div class="form-group full">
                        <label>Visitor Type Options <span style="color:var(--text-dim);font-weight:normal;">(comma-separated, shown on badge)</span></label>
                        <input type="text" name="visitor_type_list" value="{visitor_type_list}"
                               placeholder="Visitor, Guest, Contractor, Vendor, Interview">
                    </div>
                </div>
                <button type="submit" class="btn btn-primary btn-sm">Save Dropdowns</button>
            </form>
        </div>

        <!-- General Settings -->
        <div class="card">
            <h3>General Settings</h3>
            <form hx-post="/admin/settings" hx-target="#general-result" hx-swap="innerHTML">
                <div id="general-result"></div>
                <div class="form-grid">
                    <div class="form-group">
                        <label>Company / Facility Name</label>
                        <input type="text" name="company_name" value="{company_name}">
                    </div>
                    <div class="form-group">
                        <label>Subtitle</label>
                        <input type="text" name="company_subtitle" value="{company_subtitle}">
                    </div>
                    <div class="form-group">
                        <label>Receptionist Email</label>
                        <input type="email" name="receptionist_email" value="{receptionist_email}"
                               placeholder="frontdesk@company.com">
                    </div>
                    <div class="form-group">
                        <label>Badge Expiry Text</label>
                        <input type="text" name="badge_expiry_text" value="{badge_expiry_text}">
                    </div>
                    <div class="form-group">
                        <label>Timezone</label>
                        <select name="timezone">
                            <option value="Eastern Standard Time" {tz_est}>Eastern</option>
                            <option value="Central Standard Time" {tz_cst}>Central</option>
                            <option value="Mountain Standard Time" {tz_mst}>Mountain</option>
                            <option value="Pacific Standard Time" {tz_pst}>Pacific</option>
                        </select>
                    </div>
                    <div class="form-group">
                        <label>Photo Retention</label>
                        <select name="photo_retention_hours">
                            <option value="0" {pr_0}>Keep Forever</option>
                            <option value="12" {pr_12}>12 Hours</option>
                            <option value="24" {pr_24}>24 Hours</option>
                            <option value="48" {pr_48}>48 Hours</option>
                            <option value="72" {pr_72}>72 Hours (3 Days)</option>
                            <option value="168" {pr_168}>168 Hours (7 Days)</option>
                        </select>
                        <small style="color:var(--text-dim);">Auto-delete visitor photos after this period. Checked hourly.</small>
                    </div>
                </div>
                <button type="submit" class="btn btn-primary" style="margin-top:0.5rem;">Save General Settings</button>
            </form>
        </div>

        <!-- Badge Branding -->
        <div class="card">
            <h3>Badge Branding</h3>
            <p style="color:var(--text-dim);font-size:0.85rem;margin-bottom:1rem;">
                Customize badge appearance for this location. Changes apply to all new badges immediately.
            </p>
            <form hx-post="/admin/settings/badge" hx-target="#badge-result" hx-swap="innerHTML">
                <div id="badge-result"></div>
                <div class="form-grid">
                    <div class="form-group">
                        <label>Primary Color</label>
                        <div style="display:flex;gap:0.5rem;align-items:center;">
                            <input type="color" name="badge_primary_color" value="{badge_primary_color}"
                                   style="width:48px;height:36px;border:1px solid var(--border);border-radius:4px;cursor:pointer;background:none;">
                            <input type="text" value="{badge_primary_color}" id="badge-color-text"
                                   style="width:100px;font-family:monospace;"
                                   oninput="this.previousElementSibling.previousElementSibling.value=this.value"
                                   onchange="this.previousElementSibling.previousElementSibling.value=this.value">
                        </div>
                    </div>
                    <div class="form-group">
                        <label>Badge Expiry Text</label>
                        <input type="text" name="badge_expiry_text" value="{badge_expiry_text}"
                               placeholder="VALID TODAY ONLY">
                    </div>
                    <div class="form-group">
                        <label>Badge Type Label</label>
                        <input type="text" name="badge_type_label" value="{badge_type_label}"
                               placeholder="VISITOR">
                        <small style="color:var(--text-dim);">Shown in top-right of badge (e.g. VISITOR, CONTRACTOR)</small>
                    </div>
                    <div class="form-group">
                        <label>Badge Number Prefix</label>
                        <input type="text" name="badge_number_prefix" value="{badge_number_prefix}"
                               placeholder="V-">
                        <small style="color:var(--text-dim);">Daily auto-number: prefix + sequence (e.g. V-001)</small>
                    </div>
                    <div class="form-group">
                        <label>Label Color</label>
                        <select name="badge_label_color">
                            <option value="primary" {lc_primary}>Match Primary Color</option>
                            <option value="#000000" {lc_black}>Black</option>
                            <option value="#dc2626" {lc_red}>Red</option>
                            <option value="#d97706" {lc_orange}>Orange</option>
                            <option value="#059669" {lc_green}>Green</option>
                        </select>
                        <small style="color:var(--text-dim);">Color of the type label (VISITOR, CONTRACTOR, etc.)</small>
                    </div>
                    <div class="form-group full">
                        <label style="margin-bottom:0.5rem;">Font &amp; Spacing</label>
                        <div style="display:flex;gap:1rem;flex-wrap:wrap;align-items:center;">
                            <label style="font-weight:normal;display:flex;gap:0.3rem;align-items:center;">
                                Name
                                <input type="number" name="badge_font_name_pt" value="{badge_font_name_pt}" min="10" max="36" style="width:60px;">
                            </label>
                            <label style="font-weight:normal;display:flex;gap:0.3rem;align-items:center;">
                                Company
                                <input type="number" name="badge_font_company_pt" value="{badge_font_company_pt}" min="6" max="24" style="width:60px;">
                            </label>
                            <label style="font-weight:normal;display:flex;gap:0.3rem;align-items:center;">
                                Details
                                <input type="number" name="badge_font_detail_pt" value="{badge_font_detail_pt}" min="6" max="24" style="width:60px;">
                            </label>
                            <label style="font-weight:normal;display:flex;gap:0.3rem;align-items:center;">
                                Spacing
                                <select name="badge_line_spacing" style="width:auto;">
                                    <option value="0" {ls0}>None</option>
                                    <option value="2" {ls2}>Tight</option>
                                    <option value="4" {ls4}>Normal</option>
                                    <option value="6" {ls6}>Relaxed</option>
                                    <option value="8" {ls8}>Loose</option>
                                </select>
                            </label>
                        </div>
                        <small style="color:var(--text-dim);">Font sizes in pt, spacing between lines. Use Preview Badge to test.</small>
                    </div>
                    <div class="form-group full">
                        <label>Custom Footer Text</label>
                        <input type="text" name="badge_footer_text" value="{badge_footer_text}"
                               placeholder="e.g. Fort Myers, FL — Building A">
                    </div>
                    <div class="form-group full">
                        <label>Badge Fields</label>
                        <div style="display:inline-flex;gap:1.2rem;align-items:center;font-weight:normal;font-size:0.9rem;">
                            <span style="cursor:pointer;"><input type="checkbox" name="badge_show_purpose" value="1" {chk_purpose}> Purpose</span>
                            <span style="cursor:pointer;"><input type="checkbox" name="badge_show_areas" value="1" {chk_areas}> Areas</span>
                            <span style="cursor:pointer;"><input type="checkbox" name="badge_show_badge_number" value="1" {chk_badge_num}> Badge #</span>
                            <span style="cursor:pointer;"><input type="checkbox" name="badge_show_escort" value="1" {chk_escort}> Escort Required</span>
                        </div>
                        <small style="color:var(--text-dim);display:block;margin-top:4px;">Toggle which fields appear on printed badges</small>
                    </div>
                </div>
                <div style="margin-top:0.5rem;display:flex;gap:0.5rem;align-items:center;">
                    <button type="submit" class="btn btn-primary">Save Badge Branding</button>
                    <a href="/badge/preview" target="_blank" class="btn btn-ghost">Preview Badge</a>
                </div>
            </form>
            <div style="margin-top:1rem;border-top:1px solid var(--border);padding-top:1rem;">
                <label style="font-size:0.9rem;font-weight:600;margin-bottom:0.5rem;display:block;">Logo</label>
                <p style="color:var(--text-dim);font-size:0.8rem;margin-bottom:0.5rem;">
                    PNG or JPEG, max 2 MB. Appears on badge banner next to company name.
                </p>
                <div id="logo-upload-result"></div>
                <div style="display:flex;gap:0.75rem;align-items:center;">
                    {current_logo}
                    <input type="file" id="logo-file" accept="image/png,image/jpeg"
                           style="font-size:0.85rem;">
                    <button type="button" class="btn btn-ghost btn-sm" onclick="uploadLogo()">Upload Logo</button>
                </div>
            </div>
        </div>
        <script>
        function uploadLogo() {{
            const file = document.getElementById('logo-file').files[0];
            if (!file) return;
            if (file.size > 2000000) {{
                document.getElementById('logo-upload-result').innerHTML =
                    '<div class=\"alert alert-error\">File too large (max 2 MB)</div>';
                return;
            }}
            fetch('/admin/settings/badge/logo', {{
                method: 'POST',
                headers: {{ 'Content-Type': file.type }},
                body: file
            }})
            .then(r => r.text())
            .then(html => {{
                document.getElementById('logo-upload-result').innerHTML = html;
                setTimeout(() => location.reload(), 1000);
            }});
        }}
        // Sync color picker with text input
        document.querySelector('input[name="badge_primary_color"]').addEventListener('input', function() {{
            document.getElementById('badge-color-text').value = this.value;
        }});
        </script>

        <!-- Calendar / Graph API -->
        <div class="card">
            <h3>Microsoft 365 Calendar Integration</h3>
            <div style="display:flex;align-items:center;gap:0.75rem;margin-bottom:0.75rem;">
                <span>Status:</span> {graph_badge}
            </div>
            <p style="color:var(--text-dim);font-size:0.85rem;">
                Calendar and email credentials are configured via environment variables
                in the <code>.env</code> file for security. Set <code>GRAPH_TENANT_ID</code>,
                <code>GRAPH_CLIENT_ID</code>, <code>GRAPH_CLIENT_SECRET</code>,
                <code>GRAPH_GROUP_ID</code>, and <code>GRAPH_GROUP_EMAIL</code>,
                then restart GateKeeper.
            </p>
        </div>

        <!-- Host Management -->
        <div class="card">
            <h3>Hosts ({host_count})</h3>
            <table>
                <thead><tr><th>Name</th><th>Department</th><th>Email</th><th>Phone</th><th>Status</th></tr></thead>
                <tbody>{host_rows}</tbody>
            </table>
            <div style="margin-top:1rem;">
                <a href="/hosts" class="btn btn-ghost">Manage Hosts</a>
            </div>
        </div>

        <!-- Email Settings -->
        <div class="card">
            <h3>Email Notifications</h3>
            <p style="color:var(--text-dim);font-size:0.85rem;margin-bottom:1rem;">
                Sends email via Microsoft Graph API (same credentials as Calendar above).
                Requires <strong>Mail.Send</strong> application permission in your Azure app registration.
            </p>
            <form hx-post="/admin/settings/smtp" hx-target="#smtp-result" hx-swap="innerHTML">
                <div id="smtp-result"></div>
                <div class="form-grid">
                    <div class="form-group">
                        <label>From Address (must be a mailbox in your O365 tenant)</label>
                        <input type="email" name="smtp_from_address" value="{smtp_from_address}"
                               placeholder="gatekeeper@company.com">
                    </div>
                    <div class="form-group">
                        <label>From Name</label>
                        <input type="text" name="smtp_from_name" value="{smtp_from_name}"
                               placeholder="GateKeeper">
                    </div>
                </div>
                <div style="margin-top:0.5rem;display:flex;gap:0.5rem;">
                    <button type="submit" class="btn btn-primary">Save Email Settings</button>
                    <button type="button" class="btn btn-ghost"
                            hx-post="/admin/settings/smtp/test" hx-target="#smtp-result"
                            hx-include="closest form">Send Test Email</button>
                </div>
            </form>
            <div style="margin-top:1rem;padding:0.75rem;background:var(--bg);border-radius:6px;font-size:0.8rem;color:var(--text-dim);">
                <strong>Emails sent at:</strong> Pre-registration (to host, visitor, receptionist) |
                Walk-in (to host, receptionist) | Check-in (to host)
            </div>
        </div>

    "##,
        host_count = host_count,
        visitor_count = visitor_count,
        visit_count = visit_count,
        graph_badge = graph_badge,
        theme_system = if setting_val("ui_theme").is_empty() || setting_val("ui_theme") == "system" { "selected" } else { "" },
        theme_dark = if setting_val("ui_theme") == "dark" { "selected" } else { "" },
        theme_light = if setting_val("ui_theme") == "light" { "selected" } else { "" },
        purpose_list = {
            let v = setting_val("purpose_list");
            if v.is_empty() { "Meeting,Sales Call,Interview,Vendor / Install,Tour,Delivery".to_string() } else { v }
        },
        area_list = {
            let v = setting_val("area_list");
            if v.is_empty() { "Studios,Master Control,Rack Room,Transmitter,Newsroom,Offices,Multiple Areas".to_string() } else { v }
        },
        visitor_type_list = {
            let v = setting_val("visitor_type_list");
            if v.is_empty() { "Visitor,Guest,Contractor,Vendor,Interview".to_string() } else { v }
        },
        company_name = setting_val("company_name"),
        company_subtitle = setting_val("company_subtitle"),
        receptionist_email = setting_val("receptionist_email"),
        badge_expiry_text = setting_val("badge_expiry_text"),
        tz_est = if setting_val("timezone").contains("Eastern") { "selected" } else { "" },
        tz_cst = if setting_val("timezone").contains("Central") { "selected" } else { "" },
        tz_mst = if setting_val("timezone").contains("Mountain") { "selected" } else { "" },
        tz_pst = if setting_val("timezone").contains("Pacific") { "selected" } else { "" },
        pr_0 = if setting_val("photo_retention_hours") == "0" { "selected" } else { "" },
        pr_12 = if setting_val("photo_retention_hours") == "12" { "selected" } else { "" },
        pr_24 = {
            let v = setting_val("photo_retention_hours");
            if v == "24" || v.is_empty() { "selected" } else { "" }
        },
        pr_48 = if setting_val("photo_retention_hours") == "48" { "selected" } else { "" },
        pr_72 = if setting_val("photo_retention_hours") == "72" { "selected" } else { "" },
        pr_168 = if setting_val("photo_retention_hours") == "168" { "selected" } else { "" },
        smtp_from_address = setting_val("smtp_from_address"),
        smtp_from_name = setting_val("smtp_from_name"),
        badge_primary_color = {
            let c = setting_val("badge_primary_color");
            if c.is_empty() { "#1a56db".to_string() } else { c }
        },
        badge_footer_text = setting_val("badge_footer_text"),
        badge_type_label = {
            let v = setting_val("badge_type_label");
            if v.is_empty() { "VISITOR".to_string() } else { v }
        },
        badge_number_prefix = {
            let v = setting_val("badge_number_prefix");
            if v.is_empty() { "V-".to_string() } else { v }
        },
        lc_primary = {
            let v = setting_val("badge_label_color");
            if v.is_empty() || v == "primary" { "selected" } else { "" }
        },
        lc_black = if setting_val("badge_label_color") == "#000000" { "selected" } else { "" },
        lc_red = if setting_val("badge_label_color") == "#dc2626" { "selected" } else { "" },
        lc_orange = if setting_val("badge_label_color") == "#d97706" { "selected" } else { "" },
        lc_green = if setting_val("badge_label_color") == "#059669" { "selected" } else { "" },
        chk_purpose = {
            let v = setting_val("badge_show_purpose");
            if v.is_empty() || v == "1" { "checked" } else { "" }
        },
        chk_areas = {
            let v = setting_val("badge_show_areas");
            if v.is_empty() || v == "1" { "checked" } else { "" }
        },
        chk_badge_num = {
            let v = setting_val("badge_show_badge_number");
            if v.is_empty() || v == "1" { "checked" } else { "" }
        },
        chk_escort = {
            let v = setting_val("badge_show_escort");
            if v.is_empty() || v == "1" { "checked" } else { "" }
        },
        badge_font_name_pt = { let v = setting_val("badge_font_name_pt"); if v.is_empty() { "18".to_string() } else { v } },
        badge_font_company_pt = { let v = setting_val("badge_font_company_pt"); if v.is_empty() { "11".to_string() } else { v } },
        badge_font_detail_pt = { let v = setting_val("badge_font_detail_pt"); if v.is_empty() { "10".to_string() } else { v } },
        ls0 = if setting_val("badge_line_spacing") == "0" { "selected" } else { "" },
        ls2 = if setting_val("badge_line_spacing") == "2" { "selected" } else { "" },
        ls4 = { let v = setting_val("badge_line_spacing"); if v == "4" || v.is_empty() { "selected" } else { "" } },
        ls6 = if setting_val("badge_line_spacing") == "6" { "selected" } else { "" },
        ls8 = if setting_val("badge_line_spacing") == "8" { "selected" } else { "" },
        current_logo = {
            let logo = setting_val("badge_logo");
            if logo.is_empty() {
                r#"<span style="color:var(--text-dim);font-size:0.85rem;">No logo uploaded</span>"#.to_string()
            } else {
                format!(
                    r#"<img src="/photos/{}" alt="Current logo" style="height:32px;border-radius:4px;border:1px solid var(--border);">"#,
                    logo
                )
            }
        },
        host_rows = host_rows,
    );

    layout("Admin", &content)
}

// ── Group Visit ───────────────────────────────────────────────

/// Group visit registration form
pub fn group_visit_page(_hosts: &[Host], purposes: &str, areas: &str, visitor_types: &str) -> String {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let purpose_buttons = build_purpose_buttons(purposes);
    let area_options = build_area_options(areas);
    let type_options = build_visitor_type_options(visitor_types);
    let content = format!(r##"
        <h2>Register Group Visit</h2>
        <div class="alert" style="background:rgba(79,140,255,0.1);color:var(--accent);border:1px solid rgba(79,140,255,0.3);">
            For tours, school groups, or large parties. Prints numbered badges for all members.
        </div>
        <div class="card">
            <form id="group-form" hx-post="/api/group-visit" hx-target="#form-result" hx-swap="innerHTML">
                <div id="form-result"></div>

                <div class="form-grid">
                    <div class="form-group">
                        <label>Group Name *</label>
                        <input type="text" name="group_name" required autofocus
                               placeholder="e.g., Lincoln Elementary 3rd Grade"
                               style="font-size:1.1rem;padding:0.75rem;">
                    </div>
                    <div class="form-group">
                        <label>Group Size *</label>
                        <input type="number" name="group_size" required min="2" max="200" value="10"
                               style="font-size:1.1rem;padding:0.75rem;">
                    </div>
                </div>

                <div class="form-group" style="position:relative;">
                    <label>Host / Escort *</label>
                    <input type="text" id="host-search" placeholder="Start typing host name..."
                           autocomplete="off" style="font-size:1.1rem;padding:0.75rem;">
                    <input type="hidden" name="host_id" id="host-id" required>
                    <div id="host-dropdown" class="typeahead-dropdown" style="display:none;"></div>
                </div>

                <div class="form-group">
                    <label>Purpose *</label>
                    <div class="quick-purposes">
                        {purpose_buttons}
                    </div>
                    <input type="text" name="purpose" id="purpose-input" required
                           placeholder="Or type a custom purpose..." style="margin-top:0.5rem;">
                </div>

                <div class="form-grid">
                    <div class="form-group">
                        <label>Visitor Type</label>
                        <select name="visitor_type">
                            {type_options}
                        </select>
                    </div>
                    <div class="form-group">
                        <label>Areas</label>
                        <select name="areas_requested">
                            {area_options}
                        </select>
                    </div>
                </div>

                <div class="form-group">
                    <label>When?</label>
                    <div class="when-row">
                        <div class="when-field">
                            <label class="sub-label">Date</label>
                            <input type="date" name="expected_date" required value="{today}">
                        </div>
                        <div class="when-field">
                            <label class="sub-label">Arrival</label>
                            <input type="time" name="expected_time" value="09:00">
                        </div>
                        <div class="when-field">
                            <label class="sub-label">Duration</label>
                            <select name="duration">
                                <option value="60">1 hour</option>
                                <option value="90">1.5 hours</option>
                                <option value="120" selected>2 hours</option>
                                <option value="180">3 hours</option>
                                <option value="240">Half day</option>
                                <option value="480">Full day</option>
                            </select>
                        </div>
                    </div>
                </div>

                <div class="form-group full">
                    <label>Special Notes</label>
                    <textarea name="special_notes" rows="2"
                              placeholder="Chaperone info, parking needs, special accommodations..."
                              style="width:100%;resize:vertical;"></textarea>
                </div>

                <div style="margin-top:1.25rem;">
                    <button type="submit" class="btn btn-primary" style="font-size:1.1rem;padding:0.75rem 2rem;">
                        Register Group
                        <span class="htmx-indicator"><span class="spinner"></span></span>
                    </button>
                </div>
            </form>
        </div>

        <style>
            .quick-purposes {{
                display: flex; flex-wrap: wrap; gap: 0.5rem;
            }}
            .purpose-btn {{
                padding: 0.5rem 1rem;
                border-radius: 20px;
                border: 1px solid var(--border);
                background: var(--surface2);
                color: var(--text);
                cursor: pointer;
                font-size: 0.9rem;
                transition: all 0.15s;
            }}
            .purpose-btn:hover {{ border-color: var(--accent); color: var(--accent); }}
            .purpose-btn.selected {{
                background: var(--accent);
                color: #fff;
                border-color: var(--accent);
            }}
            .typeahead-dropdown {{
                position: absolute;
                top: 100%;
                left: 0; right: 0;
                background: var(--surface);
                border: 1px solid var(--border);
                border-radius: 0 0 8px 8px;
                max-height: 220px;
                overflow-y: auto;
                z-index: 100;
                box-shadow: 0 8px 24px rgba(0,0,0,0.4);
            }}
            .typeahead-item {{
                padding: 0.65rem 0.75rem;
                cursor: pointer;
                border-bottom: 1px solid var(--border);
                font-size: 0.9rem;
            }}
            .typeahead-item:hover {{ background: var(--surface2); }}
            .typeahead-item .sub {{ color: var(--text-dim); font-size: 0.8rem; }}
            .when-row {{
                display: grid;
                grid-template-columns: 1fr 1fr 1fr;
                gap: 0.75rem;
                margin-top: 0.5rem;
            }}
            .when-field label.sub-label {{
                font-size: 0.75rem;
                color: var(--text-dim);
                margin-bottom: 0.25rem;
            }}
            @media (max-width: 768px) {{
                .when-row {{ grid-template-columns: 1fr; }}
            }}
        </style>

        <script>
        // ── Host typeahead ──
        let hostTimer = null;
        const hostInput = document.getElementById('host-search');
        const hostDrop = document.getElementById('host-dropdown');
        const hostIdInput = document.getElementById('host-id');

        hostInput.addEventListener('focus', function() {{
            if (!this.value.trim()) fetchHosts('');
        }});

        hostInput.addEventListener('input', function() {{
            clearTimeout(hostTimer);
            hostIdInput.value = '';
            hostTimer = setTimeout(() => fetchHosts(this.value.trim()), 150);
        }});

        function fetchHosts(q) {{
            fetch('/api/hosts/search?q=' + encodeURIComponent(q))
                .then(r => r.json())
                .then(results => {{
                    if (results.length === 0) {{ hostDrop.style.display = 'none'; return; }}
                    hostDrop.innerHTML = results.map(h =>
                        `<div class="typeahead-item" onclick="pickHost(this)"
                              data-id="${{h.id}}" data-name="${{h.name}}" data-dept="${{h.department}}">
                            <div>${{h.name}}</div>
                            <div class="sub">${{h.department}}</div>
                        </div>`
                    ).join('');
                    hostDrop.style.display = 'block';
                }});
        }}

        function pickHost(el) {{
            hostInput.value = el.dataset.name + ' — ' + el.dataset.dept;
            hostIdInput.value = el.dataset.id;
            hostDrop.style.display = 'none';
        }}

        function pickPurpose(btn) {{
            document.querySelectorAll('.purpose-btn').forEach(b => b.classList.remove('selected'));
            btn.classList.add('selected');
            document.getElementById('purpose-input').value = btn.textContent;
        }}

        document.addEventListener('click', function(e) {{
            if (!e.target.closest('#host-search') && !e.target.closest('#host-dropdown'))
                hostDrop.style.display = 'none';
        }});

        document.getElementById('group-form').addEventListener('htmx:configRequest', function(e) {{
            if (!hostIdInput.value) {{
                e.preventDefault();
                hostInput.focus();
                hostInput.style.borderColor = 'var(--red)';
                setTimeout(() => hostInput.style.borderColor = '', 2000);
            }}
        }});
        </script>
    "##, today = today);

    layout("Group Visit", &content)
}

/// Multi-badge page for group visits — renders N badges, one per page
pub fn group_badge_page(
    v: &VisitDetail,
    opts: &BadgeOpts,
    group_size: i32,
) -> String {
    let company_name = opts.company_name;
    let expiry_text = opts.expiry_text;
    let primary_color = opts.primary_color;
    let label_color = if opts.badge_label_color == "primary" || opts.badge_label_color.is_empty() {
        primary_color
    } else {
        opts.badge_label_color
    };
    let badge_label = "GROUP";
    let group_name = html_escape(v.group_name.as_deref().unwrap_or(&v.visitor.name));
    let areas = v.areas_requested.as_deref().unwrap_or("General");
    let badge_num = v.badge_number.as_deref().unwrap_or("—");
    let date_raw = v.expected_date.as_deref()
        .or(v.check_in.as_deref())
        .unwrap_or(&v.created_at);
    let date = chrono::NaiveDate::parse_from_str(
        &date_raw.chars().take(10).collect::<String>(), "%Y-%m-%d"
    )
        .map(|d| d.format("%B %-d, %Y").to_string())
        .unwrap_or_else(|_| date_raw.to_string());
    let checkin_time = v.check_in.as_deref().unwrap_or("—");
    let visit_id_short = if v.id.len() >= 8 { &v.id[..8] } else { &v.id };
    let escort = if opts.show_escort && areas != "General" && areas != "Lobby" && areas != "Main Lobby" {
        "ESCORT REQUIRED"
    } else {
        ""
    };

    let logo_html = match opts.logo_filename {
        Some(f) => format!(
            r#"<img src="/photos/{}" alt="" style="height:18px;margin-right:6px;vertical-align:middle;">"#,
            f
        ),
        None => String::new(),
    };

    let footer_extra = if opts.footer_text.is_empty() {
        String::new()
    } else {
        format!(" | {}", opts.footer_text)
    };

    // Build detail rows
    let mut upper_rows = String::new();
    upper_rows.push_str(&format!(
        r#"<tr><td class="label">Host:</td><td class="value">{} ({})</td></tr>"#,
        html_escape(&v.host.name), html_escape(&v.host.department)
    ));
    if opts.show_purpose {
        upper_rows.push_str(&format!(
            r#"<tr><td class="label">Purpose:</td><td class="value">{}</td></tr>"#,
            html_escape(&v.purpose)
        ));
    }
    if opts.show_areas {
        upper_rows.push_str(&format!(
            r#"<tr><td class="label">Areas:</td><td class="value">{}</td></tr>"#,
            html_escape(areas)
        ));
    }

    let mut lower_rows = String::new();
    lower_rows.push_str(&format!(
        r#"<tr><td class="label">Checked In:</td><td class="value">{}</td></tr>"#,
        checkin_time
    ));
    if opts.show_badge_number {
        lower_rows.push_str(&format!(
            r#"<tr><td class="label">Badge #:</td><td class="value">{}</td></tr>"#,
            badge_num
        ));
    }

    let escort_html = if !escort.is_empty() {
        format!(r#"<div class="escort">{}</div>"#, escort)
    } else {
        String::new()
    };

    // Generate all badge divs
    let mut badges = String::new();
    for i in 1..=group_size {
        let page_break = if i < group_size { "page-break-after: always;" } else { "" };
        badges.push_str(&format!(
            r#"<div class="badge-sheet" style="{page_break}">
<div class="banner">
    <span class="org">{logo}{company}</span>
    <span class="type">{badge_label}</span>
</div>
<div class="badge-content">
    <div class="badge-upper">
        <div class="info-col" style="flex:1;">
            <div class="name">{group_name}</div>
            <div class="company">Member {i} of {total}</div>
            <table class="details">
                {upper_rows}
            </table>
        </div>
    </div>
    {escort}
    <div class="badge-lower">
        <table class="details">
            {lower_rows}
        </table>
    </div>
</div>
<div class="badge-footer">
    <span>Visit: {visit_id} | {date}{footer}</span>
    <span class="expiry">{expiry}</span>
</div>
</div>"#,
            page_break = page_break,
            logo = logo_html,
            company = html_escape(company_name),
            badge_label = badge_label,
            group_name = group_name,
            i = i,
            total = group_size,
            upper_rows = upper_rows,
            escort = escort_html,
            lower_rows = lower_rows,
            visit_id = visit_id_short,
            date = date,
            footer = footer_extra,
            expiry = expiry_text,
        ));
    }

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Group Badges — {group_name} ({total} members)</title>
<style>
@page {{ size: 4in 2.4in; margin: 0; }}
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
body {{
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
    padding: 0;
}}
.badge-sheet {{
    width: 4in; height: 2.4in;
    display: flex; flex-direction: column;
    overflow: hidden;
}}
.banner {{
    background: {primary_color}; color: #fff;
    padding: 4px 12px;
    display: flex; justify-content: space-between; align-items: center;
}}
.banner .org {{ font-size: 11pt; font-weight: 700; letter-spacing: 1px; }}
.banner .type {{ font-size: 14pt; font-weight: 800; text-transform: uppercase;
    background: #fff; color: {label_color}; padding: 2px 14px; border-radius: 4px;
    border: 2px solid #fff; letter-spacing: 1px; }}
.badge-content {{ flex: 1; display: flex; flex-direction: column; overflow: hidden; }}
.badge-upper {{
    display: flex; padding: 0.06in 0.15in 0; gap: 0.1in;
}}
.info-col {{
    flex: 1;
    display: flex; flex-direction: column;
    justify-content: flex-start;
    overflow: hidden;
}}
.name {{ font-size: {font_name}pt; font-weight: 700; line-height: 1.0;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-bottom: {spacing}px; }}
.company {{ font-size: {font_company}pt; color: #666; margin-bottom: {spacing}px; font-weight: 600; }}
.details {{ font-size: {font_detail}pt; width: 100%; border-top: 1px solid #ddd; padding-top: {spacing}px;
    border-collapse: collapse; }}
.details tr td {{ padding: {half_spacing}px 0; vertical-align: top; line-height: 1.2; }}
.details .label {{ font-weight: 600; color: #333; text-align: right; white-space: nowrap;
    padding-right: 6px; width: 1px; }}
.details .value {{ color: #555; }}
.badge-lower {{
    padding: 0 0.15in 0.04in;
    font-size: {font_detail}pt;
}}
.badge-lower .details {{ border-top: 1px solid #ddd; }}
.escort {{
    margin: 2px 0.15in; padding: 2px 6px;
    background: #fee2e2; color: #dc2626;
    font-size: 8pt; font-weight: 700;
    border-radius: 3px; display: inline-block;
    text-transform: uppercase;
}}
.badge-footer {{
    background: #f3f4f6;
    border-top: 1px solid #ddd;
    padding: 2px 10px;
    display: flex; justify-content: space-between; align-items: center;
    font-size: 7pt; color: #666;
}}
.badge-footer .expiry {{
    font-weight: 700; color: #dc2626; text-transform: uppercase;
}}
@media screen {{
    body {{ background:#e5e7eb; display:flex; flex-direction:column; align-items:center; gap:1rem; padding:1rem; }}
    .badge-sheet {{ border: 2px solid #ccc; }}
}}
@media print {{
    html {{ -webkit-print-color-adjust: exact; print-color-adjust: exact; }}
    body, .name, .company, .details, .details .label, .details .value,
    .badge-lower, .badge-footer, .badge-footer span,
    .badge-footer .expiry, .escort {{
        color: #000 !important;
    }}
    .company {{ opacity: 0.7; }}
    .details .value {{ opacity: 0.8; }}
    .badge-footer {{ background: #fff !important; border-top: 1px solid #000 !important; }}
    .badge-footer span, .badge-footer .expiry {{ color: #000 !important; }}
    .escort {{ background: #fff !important; border: 2px solid #000 !important; }}
    .details {{ border-top-color: #000 !important; }}
    .badge-lower .details {{ border-top-color: #000 !important; }}
}}
</style>
</head>
<body>
{badges}
<script>{quantize_script}</script>
</body>
</html>"#,
        quantize_script = QUANTIZE_GROUP_PRINT_JS,
        group_name = group_name,
        total = group_size,
        primary_color = primary_color,
        label_color = label_color,
        font_name = opts.font_name_pt,
        font_company = opts.font_company_pt,
        font_detail = opts.font_detail_pt,
        spacing = opts.line_spacing,
        half_spacing = opts.line_spacing / 2,
        badges = badges,
    )
}

// ── Helpers ────────────────────────────────────────────────────

fn render_visit_rows(visits: &[VisitDetail], show_actions: bool) -> String {
    if visits.is_empty() {
        return "<p style='color:var(--text-dim);padding:1rem;'>No visitors to show.</p>".to_string();
    }

    let action_header = if show_actions { "<th>Actions</th>" } else { "" };

    let rows: String = visits.iter().map(|v| {
        let (status_class, status_label) = match v.status.as_str() {
            "pending" => ("pending", "EXPECTED"),
            "running_late" => ("running_late", "DELAYED"),
            "checked_in" => ("checked_in", "ON SITE"),
            "checked_out" => ("checked_out", "CHECKED OUT"),
            "rescheduled" => ("rescheduled", "RESCHEDULED"),
            _ => ("pending", v.status.as_str()),
        };

        let type_badge = if v.is_group {
            let size = v.group_size.unwrap_or(0);
            format!(r##" <span class="badge" style="background:rgba(79,140,255,0.15);color:var(--accent);">GROUP ({})</span>"##, size)
        } else if !v.pre_registered {
            r##" <span class="badge walk_in">WALK-IN</span>"##.to_string()
        } else {
            String::new()
        };

        let display_name = html_escape(if v.is_group {
            v.group_name.as_deref().unwrap_or(&v.visitor.name)
        } else {
            &v.visitor.name
        });
        let company = html_escape(if v.is_group {
            "—"
        } else {
            v.visitor.company.as_deref().unwrap_or("—")
        });
        let expected_time = format_expected(v.expected_date.as_deref(), v.expected_time.as_deref());
        let checkin_time = v.check_in.as_deref().unwrap_or("—");
        let checkout_time = v.check_out.as_deref().unwrap_or("—");

        let actions = if show_actions {
            visit_action_buttons(v)
        } else { String::new() };

        format!(r##"<tr>
            <td>{name}{type_badge}</td>
            <td>{company}</td>
            <td>{host}</td>
            <td>{purpose}</td>
            <td>{expected}</td>
            <td><span class="badge {status_class}">{status}</span></td>
            <td>{checkin}</td>
            <td>{checkout}</td>
            {actions}
        </tr>"##,
            name = display_name,
            type_badge = type_badge,
            company = company,
            host = html_escape(&v.host.name),
            purpose = html_escape(&v.purpose),
            expected = expected_time,
            status_class = status_class,
            status = status_label,
            checkin = checkin_time,
            checkout = checkout_time,
            actions = actions,
        )
    }).collect();

    format!(r##"<table>
        <thead><tr>
            <th>Visitor</th><th>Company</th><th>Host</th><th>Purpose</th><th>Expected</th><th>Status</th><th>In</th><th>Out</th>
            {action_header}
        </tr></thead>
        <tbody>{rows}</tbody>
    </table>"##)
}

/// Format expected date+time. Shows just time for today, "Mar 26 @ 2:00 PM" for other dates.
fn format_expected(date: Option<&str>, time: Option<&str>) -> String {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let time_str = format_time_12h(time);
    match date {
        Some(d) if !d.is_empty() && d != today => {
            let date_pretty = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .map(|nd| nd.format("%b %-d").to_string())
                .unwrap_or_else(|_| d.to_string());
            if time_str == "—" {
                date_pretty
            } else {
                format!("{} @ {}", date_pretty, time_str)
            }
        }
        _ => time_str,
    }
}

/// Format "HH:MM" (24h) to friendly "9:00 AM" style
fn format_time_12h(time: Option<&str>) -> String {
    match time {
        Some(t) if !t.is_empty() => {
            let parts: Vec<&str> = t.split(':').collect();
            if let (Some(h), Some(m)) = (
                parts.first().and_then(|p| p.parse::<u32>().ok()),
                parts.get(1).and_then(|p| p.parse::<u32>().ok()),
            ) {
                let (h12, ampm) = if h == 0 { (12, "AM") }
                    else if h < 12 { (h, "AM") }
                    else if h == 12 { (12, "PM") }
                    else { (h - 12, "PM") };
                format!("{}:{:02} {}", h12, m, ampm)
            } else {
                t.to_string()
            }
        }
        _ => "—".to_string(),
    }
}

fn hosts_to_options(hosts: &[Host]) -> String {
    hosts.iter().map(|h| {
        format!(
            r##"<option value="{}">{} — {}</option>"##,
            html_escape(&h.id), html_escape(&h.name), html_escape(&h.department)
        )
    }).collect()
}

/// HTMX partial: success alert (escapes HTML in message)
/// Admin login page (separate port) — password + optional TOTP field
pub fn admin_login_page(error: Option<&str>, needs_totp: bool) -> String {
    let error_html = match error {
        Some(msg) => format!(
            r#"<div style="color:#dc2626;background:#fee2e2;padding:0.75rem;
            border-radius:6px;margin-bottom:1rem;font-size:0.9rem;">{}</div>"#,
            html_escape(msg)
        ),
        None => String::new(),
    };

    let totp_field = if needs_totp {
        r#"<label for="totp_code">Authenticator Code</label>
        <input type="text" name="totp_code" id="totp_code"
               placeholder="6-digit code" inputmode="numeric"
               pattern="[0-9]{6}" maxlength="6" autocomplete="one-time-code"
               required
               style="width:100%;padding:0.75rem;border:1px solid #d1d5db;
               border-radius:6px;font-size:1.2rem;letter-spacing:0.3em;
               text-align:center;margin-bottom:1rem;">"#
    } else {
        r#"<input type="hidden" name="totp_code" value="">"#
    };

    let totp_note = if !needs_totp {
        r#"<p style="color:#666;font-size:0.8rem;margin-top:0.75rem;
        text-align:center;">TOTP will be configured after first login.</p>"#
    } else {
        ""
    };

    format!(r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>GateKeeper — Admin Login</title>
<style>
* {{ margin:0; padding:0; box-sizing:border-box; }}
body {{
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI",
                 Helvetica, Arial, sans-serif;
    background: #1a1a2e; display:flex; justify-content:center;
    align-items:center; min-height:100vh;
}}
.login-card {{
    background: #fff; border-radius: 12px; padding: 2.5rem;
    box-shadow: 0 4px 24px rgba(0,0,0,0.3); width: 100%;
    max-width: 380px;
}}
.login-card h1 {{ font-size:1.5rem; margin-bottom:0.25rem; }}
.login-card .sub {{
    color:#666; font-size:0.9rem; margin-bottom:1.5rem;
}}
.login-card .admin-badge {{
    display:inline-block; background:#dc2626; color:#fff;
    font-size:0.7rem; font-weight:700; padding:2px 8px;
    border-radius:4px; vertical-align:middle; margin-left:6px;
}}
.login-card label {{
    display:block; font-weight:600; margin-bottom:0.4rem;
    font-size:0.9rem;
}}
.login-card input[type="password"] {{
    width:100%; padding:0.75rem; border:1px solid #d1d5db;
    border-radius:6px; font-size:1rem; margin-bottom:1rem;
}}
.login-card button {{
    width:100%; padding:0.75rem; background:#dc2626; color:#fff;
    border:none; border-radius:6px; font-size:1rem; font-weight:600;
    cursor:pointer;
}}
.login-card button:hover {{ background:#b91c1c; }}
</style>
</head>
<body>
<div class="login-card">
    <h1>GateKeeper <span class="admin-badge">ADMIN</span></h1>
    <p class="sub">Administrative Access</p>
    {error}
    <form method="POST" action="/login">
        <label for="password">Admin Password</label>
        <input type="password" name="password" id="password"
               placeholder="Enter admin password" autofocus required>
        {totp_field}
        <button type="submit">Sign In</button>
    </form>
    {totp_note}
</div>
</body>
</html>"#,
        error = error_html,
        totp_field = totp_field,
        totp_note = totp_note,
    )
}

/// TOTP setup page — shown on first admin login to scan QR into Authy
pub fn totp_setup_page(
    qr_data_uri: &str,
    secret: &str,
    error: Option<&str>,
) -> String {
    let error_html = match error {
        Some(msg) => format!(
            r#"<div style="color:#dc2626;background:#fee2e2;padding:0.75rem;
            border-radius:6px;margin-bottom:1rem;font-size:0.9rem;">{}</div>"#,
            html_escape(msg)
        ),
        None => String::new(),
    };

    format!(r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>GateKeeper — TOTP Setup</title>
<style>
* {{ margin:0; padding:0; box-sizing:border-box; }}
body {{
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI",
                 Helvetica, Arial, sans-serif;
    background: #1a1a2e; display:flex; justify-content:center;
    align-items:center; min-height:100vh;
}}
.setup-card {{
    background: #fff; border-radius: 12px; padding: 2.5rem;
    box-shadow: 0 4px 24px rgba(0,0,0,0.3); width: 100%;
    max-width: 440px; text-align: center;
}}
.setup-card h1 {{ font-size:1.4rem; margin-bottom:0.5rem; }}
.setup-card .sub {{
    color:#666; font-size:0.9rem; margin-bottom:1.5rem;
}}
.qr-img {{
    border: 1px solid #e5e7eb; border-radius: 8px;
    padding: 8px; margin-bottom: 1rem;
}}
.secret-code {{
    font-family: monospace; font-size: 0.85rem; background: #f3f4f6;
    padding: 0.5rem 1rem; border-radius: 6px; margin-bottom: 1.5rem;
    word-break: break-all; color: #374151;
}}
.setup-card label {{
    display:block; font-weight:600; margin-bottom:0.4rem;
    font-size:0.9rem; text-align:left;
}}
.setup-card input[type="text"] {{
    width:100%; padding:0.75rem; border:1px solid #d1d5db;
    border-radius:6px; font-size:1.2rem; letter-spacing:0.3em;
    text-align:center; margin-bottom:1rem;
}}
.setup-card button {{
    width:100%; padding:0.75rem; background:#059669; color:#fff;
    border:none; border-radius:6px; font-size:1rem; font-weight:600;
    cursor:pointer;
}}
.setup-card button:hover {{ background:#047857; }}
</style>
</head>
<body>
<div class="setup-card">
    <h1>Set Up Authenticator</h1>
    <p class="sub">Scan this QR code with Authy, Google Authenticator,
    or any TOTP app.</p>
    {error}
    <img class="qr-img" src="{qr}" alt="TOTP QR Code"
         width="200" height="200"><br>
    <p style="font-size:0.8rem;color:#666;margin-bottom:0.5rem;">
    Or enter this code manually:</p>
    <div class="secret-code">{secret}</div>
    <form method="POST" action="/totp/confirm">
        <label for="code">Enter 6-digit code to confirm</label>
        <input type="text" name="code" id="code" inputmode="numeric"
               pattern="[0-9]{{6}}" maxlength="6"
               autocomplete="one-time-code" autofocus required>
        <button type="submit">Verify &amp; Activate</button>
    </form>
</div>
</body>
</html>"#,
        error = error_html,
        qr = qr_data_uri,
        secret = html_escape(secret),
    )
}

pub fn alert_success(msg: &str) -> String {
    format!(r##"<div class="alert alert-success">{}</div>"##, html_escape(msg))
}

/// HTMX partial: error alert (escapes HTML in message)
pub fn alert_error(msg: &str) -> String {
    format!(r##"<div class="alert alert-error">{}</div>"##, html_escape(msg))
}

/// Render a single updated table row (for HTMX swap after action)
pub fn visit_row_partial(v: &VisitDetail) -> String {
    let (status_class, status_label) = match v.status.as_str() {
        "pending" => ("pending", "EXPECTED"),
        "running_late" => ("running_late", "DELAYED"),
        "checked_in" => ("checked_in", "ON SITE"),
        "checked_out" => ("checked_out", "CHECKED OUT"),
        "rescheduled" => ("rescheduled", "RESCHEDULED"),
        _ => ("pending", v.status.as_str()),
    };
    let type_badge = if v.is_group {
        let size = v.group_size.unwrap_or(0);
        format!(r##" <span class="badge" style="background:rgba(79,140,255,0.15);color:var(--accent);">GROUP ({})</span>"##, size)
    } else if !v.pre_registered {
        r##" <span class="badge walk_in">WALK-IN</span>"##.to_string()
    } else {
        String::new()
    };
    let display_name = html_escape(if v.is_group {
        v.group_name.as_deref().unwrap_or(&v.visitor.name)
    } else {
        &v.visitor.name
    });
    let company = html_escape(if v.is_group { "—" } else { v.visitor.company.as_deref().unwrap_or("—") });
    let expected_time = format_expected(v.expected_date.as_deref(), v.expected_time.as_deref());
    let checkin_time = v.check_in.as_deref().unwrap_or("—");
    let checkout_time = v.check_out.as_deref().unwrap_or("—");

    let actions = visit_action_buttons(v);

    format!(r##"<tr>
        <td>{name}{type_badge}</td><td>{company}</td><td>{host}</td><td>{purpose}</td>
        <td>{expected}</td>
        <td><span class="badge {status_class}">{status_label}</span></td>
        <td>{checkin}</td><td>{checkout}</td>{actions}
    </tr>"##,
        name = display_name, type_badge = type_badge, company = company,
        host = html_escape(&v.host.name), purpose = html_escape(&v.purpose),
        expected = expected_time,
        status_class = status_class, status_label = status_label,
        checkin = checkin_time, checkout = checkout_time, actions = actions
    )
}

/// Generate action buttons based on visit status
fn visit_action_buttons(v: &VisitDetail) -> String {
    match v.status.as_str() {
        "pending" | "running_late" => {
            let checkin_button = if v.is_group {
                let size = v.group_size.unwrap_or(0);
                format!(
                    r##"<button class="btn btn-success btn-sm" onclick="if(!confirm('Check in group of {size}? This will print {size} badges.'))return;checkInGroup('{id}',this)">Check In Group ({size})</button>"##,
                    id = v.id, size = size
                )
            } else {
                format!(
                    r##"<button class="btn btn-success btn-sm" onclick="openCamera('{id}','{name}',this)">On Site</button>"##,
                    id = v.id,
                    name = html_escape(&v.visitor.name.replace('\\', "\\\\").replace('\'', "\\'")),
                )
            };
            format!(
                r##"<td class="actions">
                    {checkin_button}
                    <select class="btn btn-muted btn-sm" style="cursor:pointer;" onchange="if(this.value){{markLate('{id}',this.value,this);this.value='';}}">
                        <option value="">Delayed</option>
                        <option value="5">~5 min</option>
                        <option value="10">~10 min</option>
                        <option value="15">~15 min</option>
                        <option value="20">~20 min</option>
                        <option value="25">~25 min</option>
                        <option value="30">~30 min</option>
                        <option value="35">~35 min</option>
                        <option value="40">~40 min</option>
                        <option value="45">~45 min</option>
                    </select>
                    <button class="btn btn-primary btn-sm" onclick="openReschedule('{id}',this)">Reschedule</button>
                </td>"##,
                checkin_button = checkin_button,
                id = v.id,
            )
        },
        "checked_in" => {
            if v.is_group {
                let size = v.group_size.unwrap_or(0);
                format!(
                    r##"<td class="actions"><button class="btn btn-ghost btn-sm" hx-post="/api/visits/{id}/checkout" hx-swap="outerHTML" hx-target="closest tr" hx-confirm="Check out all {size} members of this group?">Check Out Group</button></td>"##,
                    id = v.id, size = size
                )
            } else {
                format!(
                    r##"<td class="actions"><button class="btn btn-ghost btn-sm" hx-post="/api/visits/{id}/checkout" hx-swap="outerHTML" hx-target="closest tr">Check Out</button></td>"##,
                    id = v.id
                )
            }
        },
        _ => "<td>—</td>".to_string(),
    }
}

/// Build purpose quick-pick buttons from comma-separated string
fn build_purpose_buttons(purposes: &str) -> String {
    purposes.split(',')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| format!(
            r#"<button type="button" class="purpose-btn" onclick="pickPurpose(this)">{}</button>"#,
            html_escape(p)
        ))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build visitor type <option> tags from comma-separated string
fn build_visitor_type_options(types: &str) -> String {
    types.split(',')
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(|t| {
            let escaped = html_escape(t);
            format!(r#"<option value="{escaped}">{escaped}</option>"#)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build area <option> tags from comma-separated string
fn build_area_options(areas: &str) -> String {
    let mut opts = String::from(r#"<option value="">Lobby only</option>"#);
    for area in areas.split(',').map(|a| a.trim()).filter(|a| !a.is_empty()) {
        let escaped = html_escape(area);
        opts.push_str(&format!(
            r#"<option value="{escaped}">{escaped}</option>"#
        ));
    }
    opts
}
