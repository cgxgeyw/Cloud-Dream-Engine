const GAME_UI_BINDING_PREFIX = "$";

export function isGameUiBindingValue(value: unknown): value is string {
  return typeof value === "string" && value.startsWith(GAME_UI_BINDING_PREFIX) && value.length > 1;
}

export function normalizeGameUiBindingPath(binding: string): string[] {
  if (!isGameUiBindingValue(binding)) {
    return [];
  }

  const path = binding.slice(1).trim();
  if (!path || path.includes("[") || path.includes("]")) {
    return [];
  }

  return path
    .split(".")
    .map((segment) => segment.trim())
    .filter(Boolean);
}

export function resolveGameUiBinding(root: unknown, binding: string): unknown {
  const segments = normalizeGameUiBindingPath(binding);
  if (segments.length === 0) {
    return undefined;
  }

  let current: unknown = root;
  for (const segment of segments) {
    if (Array.isArray(current)) {
      if (segment === "length") {
        current = current.length;
        continue;
      }
      return undefined;
    }

    if (!current || typeof current !== "object") {
      return undefined;
    }

    current = (current as Record<string, unknown>)[segment];
  }

  return current;
}
