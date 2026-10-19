/** HTML pages run in an opaque-origin iframe. Only this MessagePort reaches the host. */
export function createHtmlWorldDocument(html: string, stylesheet = ''): string {
  const page = new DOMParser().parseFromString(html, 'text/html');
  page.querySelectorAll('base, meta[http-equiv], iframe, object, embed').forEach(node => node.remove());
  // Package-local scripts/styles are bundled by the importer. Runtime network access is disabled.
  page.querySelectorAll('script[src], link[rel="stylesheet"]').forEach(node => node.remove());
  const head = page.head;
  const base = page.createElement('base');
  base.href = new URL('/', window.location.href).toString();
  const csp = page.createElement('meta');
  csp.httpEquiv = 'Content-Security-Policy';
  csp.content = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src asset: http://asset.localhost https://asset.localhost data: blob:; media-src asset: http://asset.localhost https://asset.localhost data: blob:; font-src data:; connect-src 'none'; worker-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";
  const sdk = page.createElement('script');
  sdk.textContent = `(${installWorldSdk.toString()})();`;
  const baseline = page.createElement('style');
  baseline.textContent = 'html{height:100%;box-sizing:border-box;padding:var(--world-inset-top,0px) var(--world-inset-right,0px) var(--world-inset-bottom,0px) var(--world-inset-left,0px)}body{height:100%;margin:0}*,*::before,*::after{box-sizing:inherit}';
  const css = page.createElement('style'); css.textContent = stylesheet;
  head.prepend(base, csp, baseline, css, sdk);
  return '<!doctype html>\n' + page.documentElement.outerHTML;
}

function installWorldSdk() {
  let port: MessagePort | undefined;
  let channelId = '';
  let state: Record<string, unknown> | undefined;
  let revision = -1;
  let sequence = 0;
  const listeners = new Set<(state: Record<string, unknown>) => void>();
  const pending = new Map<string, { resolve: (value: unknown) => void; reject: (error: Error) => void; timer: number }>();
  const showError = (error: unknown) => {
    const message = error instanceof Error ? error.message : String(error);
    // Keep bridge and script failures visible even if a world omits its own error UI.
    let notice = document.getElementById('world-sdk-error');
    if (!notice) {
      notice = document.createElement('div');
      notice.id = 'world-sdk-error'; notice.setAttribute('role', 'alert');
      notice.style.cssText = 'position:fixed;z-index:2147483647;top:var(--world-inset-top,0px);left:12px;right:12px;padding:12px 16px;background:#f5e5de;color:#802e27;border:1px solid #b96758;font:13px/1.5 system-ui;white-space:pre-wrap;overflow-wrap:anywhere';
      document.body?.append(notice);
    }
    notice.textContent = message;
    window.dispatchEvent(new CustomEvent('world-error', { detail: { message } }));
    if (port && channelId) port.postMessage({ type: 'world-frame/diagnostic', protocolVersion: 3, channelId, level: 'error', phase: 'runtime', message });
  };
  window.addEventListener('error', event => { if (event.message) showError(event.message); });
  window.addEventListener('unhandledrejection', event => showError(event.reason));
  const request = (action: Record<string, unknown>) => new Promise<unknown>((resolve, reject) => {
    if (!port) { reject(new Error('World SDK is not connected yet. Use world.subscribe().')); return; }
    const requestId = String(++sequence);
    const timer = window.setTimeout(() => { pending.delete(requestId); const error = new Error('World action timed out'); if (port) port.postMessage({ type: 'world-frame/diagnostic', protocolVersion: 3, channelId, level: 'error', phase: 'action', message: error.message, requestId }); reject(error); }, 120000);
    pending.set(requestId, { resolve, reject, timer });
    port.postMessage({ type: 'world-frame/action', protocolVersion: 3, channelId, requestId, action });
    port.postMessage({ type: 'world-frame/diagnostic', protocolVersion: 3, channelId, level: 'info', phase: 'action', message: String(action.type), requestId });
  });
  const action = (type: string, args: Record<string, unknown> = {}) => request({ ...args, type }).catch(error => { showError(error); throw error; });
  Object.defineProperty(window, 'world', { value: Object.freeze({
    getState: () => state,
    subscribe: (listener: (value: Record<string, unknown>) => void) => { listeners.add(listener); if (state) listener(state); return () => listeners.delete(listener); },
    actions: Object.freeze({
      sendMessage: (content: string) => action('submit-message', { options: { content, mode: 'submit' } }),
      submitMessage: (options: { content?: string; mode?: 'submit' | 'edit' | 'resend'; turnIndex?: number }) => action('submit-message', { options }),
      clearError: () => { document.getElementById('world-sdk-error')?.remove(); return action('clear-action-error'); },
      setDraft: (value: string) => action('set-draft-value', { value }),
      setAutoScroll: (enabled: boolean) => action('set-auto-scroll', { enabled }),
      switchSideTab: (tabKey: string) => action('switch-side-tab', { tabKey }),
      navigate: (target: string) => action('navigate', { target }),
      copyText: (text: string) => action('copy-text', { text }),
      branch: () => action('branch-from-current'),
      retry: (retryToken: string) => action('retry-turn', { retryToken }),
      editTurn: (content: string, turnIndex: number) => action('start-editing-turn', { content, turnIndex }),
      cancelEdit: () => action('cancel-editing-turn'),
      answerInteraction: (messageId: string, interactionId: string, answer: unknown) => action('answer-interaction', { messageId, interactionId, answer }),
      pickImage: () => action('pick-image'),
      startRecording: () => action('start-recording'),
      stopRecording: (send = false) => action('stop-recording', { send }),
      removeImage: (index: number) => action('remove-image', { index }),
      removeAudio: (index: number) => action('remove-audio', { index }),
    }),
    logic: Object.freeze({ run: (handler: string, input: unknown = {}) => action('world-logic-invoke', { handler, input }) }),
    kv: Object.freeze({
      list: (namespace: string, scope?: unknown) => action('world-kv-list', { namespace, scope }),
      get: (namespace: string, key: string, scope?: unknown) => action('world-kv-get', { namespace, key, scope }),
      set: (namespace: string, key: string, value: unknown, scope?: unknown) => action('world-kv-set', { namespace, key, value, scope }),
      delete: (namespace: string, key: string, scope?: unknown) => action('world-kv-delete', { namespace, key, scope }),
    }),
    records: Object.freeze({
      list: (collection: string) => action('world-record-list', { collection }),
      create: (collection: string, data: unknown) => action('world-record-create', { collection, data }),
      update: (collection: string, recordId: string, data: unknown) => action('world-record-update', { collection, recordId, data }),
      delete: (collection: string, recordId: string) => action('world-record-delete', { collection, recordId }),
    }),
    platform: Object.freeze({ invoke: (feature: string, params: unknown = {}) => action('world-platform-invoke', { feature, params }) }),
  }), writable: false, configurable: false });
  const boot = () => parent.postMessage({ type: 'world-frame/boot', protocolVersion: 3 }, '*');
  const bootTimer = window.setInterval(boot, 250);
  window.addEventListener('message', event => {
    if (event.source !== parent || event.data?.type !== 'world-frame/connect' || event.data.protocolVersion !== 3 || !event.ports[0]) return;
    window.clearInterval(bootTimer); port?.close();
    port = event.ports[0]; channelId = event.data.channelId; revision = -1;
    port.onmessage = event => {
      const message = event.data;
      if (message.channelId !== channelId || message.protocolVersion !== 3) return;
      if (message.type === 'world-frame/action-result') {
        const entry = pending.get(message.requestId); if (!entry) return;
        pending.delete(message.requestId); window.clearTimeout(entry.timer);
        if (message.ok) { port?.postMessage({ type: 'world-frame/diagnostic', protocolVersion: 3, channelId, level: 'info', phase: 'action', message: 'action completed', requestId: message.requestId }); entry.resolve(message.result); } else { port?.postMessage({ type: 'world-frame/diagnostic', protocolVersion: 3, channelId, level: 'error', phase: 'action', message: message.error || 'World UI action failed', requestId: message.requestId }); entry.reject(new Error(message.error)); }
        return;
      }
      if (!['world-frame/render-runtime', 'world-frame/render-preview'].includes(message.type)) return;
      if (message.revision <= revision) {
        port?.postMessage({ type: 'world-frame/diagnostic', protocolVersion: 3, channelId, level: 'warn', phase: 'render', message: 'ignored stale snapshot', revision: message.revision });
        return;
      }
      revision = message.revision;
      port?.postMessage({ type: 'world-frame/diagnostic', protocolVersion: 3, channelId, level: 'info', phase: 'render', message: 'snapshot rendered', revision });
      const payload = message.payload;
      state = payload.snapshot || { preview: true, world: { name: payload.worldName }, session: { location: payload.location, time_label: payload.timeLabel }, player: { name: payload.playerName }, messages: payload.messages, visible_characters: payload.visibleCharacters, ui_state: { submitting: false }, draft_input: { value: '' } };
      const viewport = (state?.viewport || {}) as { safe_area?: Record<string, number>; keyboard_height?: number };
      for (const side of ['top', 'right', 'bottom', 'left']) document.documentElement.style.setProperty('--world-inset-' + side, ((side === 'bottom' && viewport.keyboard_height ? 0 : viewport.safe_area?.[side]) || 0) + 'px');
      const runtimeError = (state?.errors as { action_error?: string } | undefined)?.action_error;
      if (runtimeError) showError(runtimeError);
      for (const listener of listeners) { try { listener(state!); } catch (error) { console.error(error); showError(error); } }
    };
    port.start(); port.postMessage({ type: 'world-frame/ready', protocolVersion: 3, channelId });
  });
  // System navigation belongs to the bridge, not HTML links or forms.
  document.addEventListener('click', event => { if ((event.target as Element)?.closest?.('a[href]')) event.preventDefault(); }, true);
  document.addEventListener('submit', event => event.preventDefault(), true);
  boot();
}
