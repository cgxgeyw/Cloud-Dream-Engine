export function createWorldFrameDocument(
  runtimeUrl = "./runtime.iife.js",
  stylesheetUrl = "./style.css",
  crossOrigin = false,
  trustedSourcePolicy = "'self' http://tauri.localhost https://tauri.localhost tauri:",
): string {
  const crossOriginAttribute = crossOrigin ? ' crossorigin="anonymous"' : "";
  const csp = [
    "default-src 'none'",
    `script-src ${trustedSourcePolicy}`,
    `style-src ${trustedSourcePolicy} 'unsafe-inline'`,
    "img-src asset: http://asset.localhost data: blob:",
    "font-src data: blob:",
    "media-src asset: http://asset.localhost data: blob:",
    "connect-src 'none'",
    "object-src 'none'",
    "frame-src 'none'",
    "base-uri 'none'",
    "form-action 'none'",
  ].join("; ");
  return `<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="UTF-8" />
    <meta http-equiv="Content-Security-Policy" content="${csp}" />
    <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover, interactive-widget=resizes-content" />
    <link rel="stylesheet" href="${escapeHtmlAttribute(stylesheetUrl)}"${crossOriginAttribute} />
    <title>World UI Frame</title>
  </head>
  <body>
    <div id="world-frame-root"></div>
    <script src="${escapeHtmlAttribute(runtimeUrl)}"${crossOriginAttribute}></script>
  </body>
</html>`;
}

function escapeHtmlAttribute(source: string): string {
  return source.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
}
