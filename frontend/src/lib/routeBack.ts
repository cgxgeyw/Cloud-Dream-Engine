export function resolveParentPath(pathname: string, search: string) {
  const params = new URLSearchParams(search);
  const worldId = params.get("worldId");

  if (pathname === "/" || pathname.startsWith("/game/") || pathname.startsWith("/debug/")) {
    return "/";
  }

  if (pathname === "/new-game" || pathname === "/saves" || pathname === "/worlds" || pathname === "/settings" || pathname === "/mcp-tools") {
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
