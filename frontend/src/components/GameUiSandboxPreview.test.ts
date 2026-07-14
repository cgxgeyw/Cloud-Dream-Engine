import { describe, expect, it } from "vitest";

import {
  createWorldFrameDocument,
  resolveWorldFrameAssetUrl,
} from "./GameUiSandboxPreview";

describe("world frame document", () => {
  it("loads the trusted renderer as external self-hosted resources", () => {
    const document = createWorldFrameDocument(
      "http://127.0.0.1:8851/world-frame/runtime.iife.js",
      "http://127.0.0.1:8851/world-frame/style.css",
    );

    expect(document).toContain("script-src 'self' http://tauri.localhost");
    expect(document).toContain("connect-src 'none'");
    expect(document).toContain("default-src 'none'");
    expect(document).toContain('src="http://127.0.0.1:8851/world-frame/runtime.iife.js"');
    expect(document).toContain('href="http://127.0.0.1:8851/world-frame/style.css"');
    expect(document).not.toContain("nonce=");
    expect(document).not.toContain("crossorigin=");
  });

  it("resolves frame assets from the application root", () => {
    expect(resolveWorldFrameAssetUrl("runtime.iife.js", "http://127.0.0.1:8851/worlds/new"))
      .toBe("http://127.0.0.1:8851/world-frame/runtime.iife.js");
    expect(resolveWorldFrameAssetUrl("style.css", "file:///E:/app/index.html"))
      .toBe("file:///E:/app/world-frame/style.css");
  });
});
