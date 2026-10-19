// CDP diagnostic: evaluate shell page metrics on the running app.
const ID = process.argv[2] || "DEE3CCEB4F65C19BC6DF5FE3C366A7C6";
const expression = process.argv[3] || `(() => {
  const root = document.querySelector(".layout-root");
  const cs = root ? getComputedStyle(root) : null;
  const header = document.querySelector(".layout-header");
  return {
    href: location.href,
    insets: window.__nativeSafeAreaInsets ?? null,
    keyboard: window.__nativeKeyboardHeight ?? null,
    inner: { w: window.innerWidth, h: window.innerHeight },
    visual: { h: window.visualViewport?.height, top: window.visualViewport?.offsetTop },
    layoutPaddingTop: cs ? cs.paddingTop : "NO .layout-root",
    layoutPaddingBottom: cs ? cs.paddingBottom : "NO .layout-root",
    appSafeVar: getComputedStyle(document.querySelector(".mobile-nav-container") || document.documentElement).getPropertyValue("--app-safe-area-top"),
    headerTop: header ? Math.round(header.getBoundingClientRect().top) : "NO .layout-header",
  };
})()`;

const ws = new WebSocket(`ws://127.0.0.1:9222/devtools/page/${ID}`);
let done = false;
ws.onopen = () => {
  ws.send(JSON.stringify({ id: 1, method: "Runtime.evaluate", params: { expression, returnByValue: true, awaitPromise: true } }));
};
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  if (msg.id === 1) {
    done = true;
    if (msg.result?.exceptionDetails) {
      console.log("EXCEPTION:", JSON.stringify(msg.result.exceptionDetails).slice(0, 500));
    } else {
      console.log(JSON.stringify(msg.result?.result?.value, null, 2));
    }
    ws.close();
  }
};
ws.onclose = () => { if (!done) { console.log("socket closed before result"); process.exit(1); } process.exit(0); };
ws.onerror = () => { console.log("WS ERROR"); process.exit(1); };
setTimeout(() => { console.log("TIMEOUT"); process.exit(1); }, 8000);
