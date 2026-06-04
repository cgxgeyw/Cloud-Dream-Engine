export function getDocumentCspNonce(): string | undefined {
  if (typeof document === "undefined") {
    return undefined;
  }

  const nonceElement = document.querySelector<HTMLElement>(
    "style[nonce], script[nonce], link[nonce]",
  );
  const nonce = nonceElement?.nonce || nonceElement?.getAttribute("nonce") || "";

  return nonce || undefined;
}
