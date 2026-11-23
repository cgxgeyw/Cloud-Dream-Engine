export function resolveParentPath(pathname: string, search: string) {
  const params = new URLSearchParams(search);
  const worldId = params.get("worldId");

  // 调试面板的上一级是它所属的那局游戏，不是应用首页。
  if (/^\/debug\/[^/]+$/.test(pathname)) {
    const sessionId = pathname.split("/")[2] ?? "";
    return sessionId ? `/game/${encodeURIComponent(sessionId)}` : "/";
  }

  if (pathname === "/" || pathname.startsWith("/game/")) {
    return "/";
  }

  if (pathname === "/stickers") {
    return "/knowledge";
  }

  if (pathname === "/new-game" || pathname === "/saves" || pathname === "/worlds" || pathname === "/settings" || pathname === "/mcp-tools" || pathname === "/knowledge") {
    return "/";
  }

  if (pathname.startsWith("/new-game/setup/")) {
    return "/new-game";
  }

  if (pathname === "/worlds/new" || /^\/worlds\/[^/]+\/edit$/.test(pathname)) {
    return "/worlds";
  }

  if (/^\/worlds\/[^/]+\/characters$/.test(pathname)) {
    const id = pathname.split("/")[2] ?? "";
    return id ? `/worlds/${encodeURIComponent(id)}/edit` : "/worlds";
  }

  if (pathname === "/characters/new" || /^\/characters\/[^/]+\/edit$/.test(pathname)) {
    return worldId ? `/worlds/${encodeURIComponent(worldId)}/characters` : "/worlds";
  }

  const segments = pathname.split("/").filter(Boolean);
  if (segments.length <= 1) {
    return "/";
  }
  return `/${segments.slice(0, -1).join("/")}`;
}
