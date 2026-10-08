import type { WorldLogicConfig } from "../data/gameUi";
import type { KvScope } from "../data/types";
import type { WorldFrameAction } from "./protocol";

const MAX_LOGIC_MESSAGE_BYTES = 256 * 1024;

type SendAction = (action: WorldFrameAction) => Promise<unknown>;

type WorkerStorageRequest = {
  type: "storage-request";
  requestId: string;
  operation: string;
  payload: Record<string, unknown>;
};

type WorkerResult = {
  type: "result";
  ok: boolean;
  result?: unknown;
  error?: string;
};

export async function invokeWorldLogic(
  config: WorldLogicConfig,
  handler: string,
  input: unknown,
  sendAction: SendAction,
): Promise<unknown> {
  if (config.runtime !== "sandbox-js-v1" || !config.source.trim()) {
    throw new Error("该世界包没有提供沙箱 JavaScript 逻辑。");
  }
  if (!/^[a-zA-Z0-9._-]{1,128}$/.test(handler)) {
    throw new Error("世界逻辑处理函数名无效。");
  }
  assertMessageSize(input, "世界逻辑输入");

  try {
    return await invokeWorldLogicViaWorker(config, handler, input, sendAction);
  } catch (errorLike) {
    const message = errorLike instanceof Error ? errorLike.message : String(errorLike);
    // 安卓 WebView + sandbox iframe 创建 Worker 失败时，退回宿主同线程执行同一套 SDK。
    if (/Worker|worker|创建|运行失败/i.test(message)) {
      return invokeWorldLogicInline(config, handler, input, sendAction);
    }
    throw errorLike;
  }
}

async function invokeWorldLogicViaWorker(
  config: WorldLogicConfig,
  handler: string,
  input: unknown,
  sendAction: SendAction,
): Promise<unknown> {
  const workerSource = createWorkerSource(config.source);
  const objectUrl = URL.createObjectURL(new Blob([workerSource], { type: "text/javascript" }));
  let worker: Worker;
  try {
    worker = new Worker(objectUrl, { name: `world-logic:${handler}` });
  } catch (errorLike) {
    URL.revokeObjectURL(objectUrl);
    throw new Error(
      `世界逻辑 Worker 运行失败。${errorLike instanceof Error ? errorLike.message : String(errorLike)}`,
    );
  }
  URL.revokeObjectURL(objectUrl);

  return new Promise<unknown>((resolve, reject) => {
    let settled = false;
    const finish = (callback: () => void) => {
      if (settled) {
        return;
      }
      settled = true;
      window.clearTimeout(timer);
      worker.terminate();
      callback();
    };
    const timer = window.setTimeout(() => {
      finish(() => reject(new Error(`世界逻辑超过 ${config.timeout_ms} 毫秒，已被终止。`)));
    }, config.timeout_ms);

    worker.onerror = (event) => {
      finish(() => reject(new Error(event.message || "世界逻辑 Worker 运行失败。")));
    };
    worker.onmessage = (event: MessageEvent<unknown>) => {
      const message = asRecord(event.data);
      if (message?.type === "storage-request") {
        const storageRequest = message as unknown as WorkerStorageRequest;
        void handleStorageRequest(storageRequest, sendAction).then(
          (result) => {
            if (!settled) {
              worker.postMessage({
                type: "storage-result",
                requestId: storageRequest.requestId,
                ok: true,
                result,
              });
            }
          },
          (errorLike) => {
            if (!settled) {
              worker.postMessage({
                type: "storage-result",
                requestId: storageRequest.requestId,
                ok: false,
                error: errorLike instanceof Error ? errorLike.message : String(errorLike),
              });
            }
          },
        );
        return;
      }
      if (message?.type !== "result") {
        return;
      }
      const result = message as unknown as WorkerResult;
      if (result.ok) {
        finish(() => resolve(result.result));
      } else {
        finish(() => reject(new Error(result.error || "世界逻辑执行失败。")));
      }
    };
    worker.postMessage({ type: "invoke", handler, input });
  });
}

/** Worker 不可用时的宿主同线程实现：同一 register/handler 与 storage SDK 语义。 */
async function invokeWorldLogicInline(
  config: WorldLogicConfig,
  handler: string,
  input: unknown,
  sendAction: SendAction,
): Promise<unknown> {
  const handlers = new Map<string, (input: unknown, api: unknown) => unknown>();
  const records = {
    list: (collection: string) =>
      handleStorageRequest({ type: "storage-request", requestId: "inline", operation: "records.list", payload: { collection } }, sendAction),
    create: (collection: string, data: unknown) =>
      handleStorageRequest({ type: "storage-request", requestId: "inline", operation: "records.create", payload: { collection, data } }, sendAction),
    update: (collection: string, recordId: string, data: unknown) =>
      handleStorageRequest({ type: "storage-request", requestId: "inline", operation: "records.update", payload: { collection, recordId, data } }, sendAction),
    remove: (collection: string, recordId: string) =>
      handleStorageRequest({ type: "storage-request", requestId: "inline", operation: "records.delete", payload: { collection, recordId } }, sendAction),
    query: async (collection: string, options: { where?: Record<string, unknown>; orderBy?: [string, string?]; offset?: number; limit?: number } = {}) => {
      const raw = (await handleStorageRequest(
        { type: "storage-request", requestId: "inline", operation: "records.list", payload: { collection } },
        sendAction,
      )) as unknown[];
      const list = Array.isArray(raw) ? raw : [];
      let recordsOut = list.filter((row) => matchesWhereInline(row, options.where));
      if (Array.isArray(options.orderBy) && options.orderBy.length > 0) {
        const field = options.orderBy[0];
        const direction = options.orderBy[1] === "desc" ? -1 : 1;
        recordsOut = [...recordsOut].sort((left, right) => {
          const a = left && typeof left === "object" ? (left as { data?: Record<string, unknown> }).data?.[field] : undefined;
          const b = right && typeof right === "object" ? (right as { data?: Record<string, unknown> }).data?.[field] : undefined;
          return a === b ? 0 : (a as number | string) > (b as number | string) ? direction : -direction;
        });
      }
      const offset = Math.max(0, Number(options.offset) || 0);
      const limit = Math.min(1000, Math.max(0, Number(options.limit) || 1000));
      return recordsOut.slice(offset, offset + limit);
    },
  };
  const kv = {
    list: (namespace: string, options?: { scope?: string; characterId?: string }) =>
      handleStorageRequest({ type: "storage-request", requestId: "inline", operation: "kv.list", payload: { namespace, scope: normalizeScope(options) } }, sendAction),
    get: async (namespace: string, key: string, fallback: unknown = null, options?: { scope?: string; characterId?: string }) => {
      const entry = (await handleStorageRequest(
        { type: "storage-request", requestId: "inline", operation: "kv.get", payload: { namespace, key, scope: normalizeScope(options) } },
        sendAction,
      )) as { value?: unknown } | null;
      return entry && typeof entry === "object" && "value" in entry ? entry.value : fallback;
    },
    set: (namespace: string, key: string, value: unknown, options?: { scope?: string; characterId?: string }) =>
      handleStorageRequest({ type: "storage-request", requestId: "inline", operation: "kv.set", payload: { namespace, key, value, scope: normalizeScope(options) } }, sendAction),
    remove: (namespace: string, key: string, options?: { scope?: string; characterId?: string }) =>
      handleStorageRequest({ type: "storage-request", requestId: "inline", operation: "kv.delete", payload: { namespace, key, scope: normalizeScope(options) } }, sendAction),
  };
  const platform = {
    invoke: (feature: string, params?: unknown) =>
      handleStorageRequest({ type: "storage-request", requestId: "inline", operation: "platform.invoke", payload: { feature, params: params ?? {} } }, sendAction),
  };
  const worldApi = Object.freeze({ records, kv, platform });
  const world = Object.freeze({
    register(name: string, handlerFn: (input: unknown, api: unknown) => unknown) {
      if (typeof name !== "string" || typeof handlerFn !== "function") {
        throw new Error("world.register(name, handler) 需要传入函数。");
      }
      handlers.set(name, handlerFn);
    },
  });

  const runner = new Function(
    "world",
    `"use strict";\n${config.source}\nreturn typeof world !== "undefined" ? world : null;`,
  ) as (world: unknown) => unknown;
  runner(world);

  const target = handlers.get(handler);
  if (!target) {
    throw new Error(`World logic handler not found: ${handler}`);
  }
  const result = await Promise.resolve(target(input, worldApi));
  const encoded = JSON.stringify(result === undefined ? null : result);
  if (encoded && encoded.length > MAX_LOGIC_MESSAGE_BYTES) {
    throw new Error(`世界逻辑结果超过 ${MAX_LOGIC_MESSAGE_BYTES} 字节上限。`);
  }
  return result;
}

function normalizeScope(options?: { scope?: string; characterId?: string }): Record<string, unknown> | undefined {
  if (!options || typeof options.scope !== "string") return undefined;
  if (options.scope !== "world" && options.scope !== "session" && options.scope !== "character") return undefined;
  const scope: Record<string, unknown> = { scope: options.scope };
  if (options.scope === "character" && typeof options.characterId === "string") {
    scope.character_id = options.characterId;
  }
  return scope;
}

function matchesWhereInline(record: unknown, where: Record<string, unknown> | undefined): boolean {
  if (!where || typeof where !== "object") return true;
  const data = record && typeof record === "object" && "data" in record
    ? (record as { data?: Record<string, unknown> }).data
    : undefined;
  return Object.entries(where).every(([field, expected]) => {
    const actual = data?.[field];
    if (!expected || typeof expected !== "object" || Array.isArray(expected)) {
      return actual === expected;
    }
    const exp = expected as Record<string, unknown>;
    if ("eq" in exp && actual !== exp.eq) return false;
    if ("ne" in exp && actual === exp.ne) return false;
    if ("gt" in exp && !((actual as number) > (exp.gt as number))) return false;
    if ("gte" in exp && !((actual as number) >= (exp.gte as number))) return false;
    if ("lt" in exp && !((actual as number) < (exp.lt as number))) return false;
    if ("lte" in exp && !((actual as number) <= (exp.lte as number))) return false;
    if (Array.isArray(exp.in) && !exp.in.includes(actual)) return false;
    return true;
  });
}

async function handleStorageRequest(
  request: WorkerStorageRequest,
  sendAction: SendAction,
): Promise<unknown> {
  const payload = request.payload;
  switch (request.operation) {
    case "records.list":
      return sendAction({
        type: "world-record-list",
        collection: readString(payload.collection, "collection"),
      });
    case "records.create":
      return sendAction({
        type: "world-record-create",
        collection: readString(payload.collection, "collection"),
        data: readData(payload.data),
      });
    case "records.update":
      return sendAction({
        type: "world-record-update",
        collection: readString(payload.collection, "collection"),
        recordId: readString(payload.recordId, "recordId"),
        data: readData(payload.data),
      });
    case "records.delete":
      return sendAction({
        type: "world-record-delete",
        collection: readString(payload.collection, "collection"),
        recordId: readString(payload.recordId, "recordId"),
      });
    case "kv.list":
      return sendAction({
        type: "world-kv-list",
        namespace: readString(payload.namespace, "namespace"),
        scope: readKvScope(payload.scope),
      });
    case "kv.get":
      return sendAction({
        type: "world-kv-get",
        namespace: readString(payload.namespace, "namespace"),
        key: readString(payload.key, "key"),
        scope: readKvScope(payload.scope),
      });
    case "kv.set":
      return sendAction({
        type: "world-kv-set",
        namespace: readString(payload.namespace, "namespace"),
        key: readString(payload.key, "key"),
        value: payload.value,
        scope: readKvScope(payload.scope),
      });
    case "kv.delete":
      return sendAction({
        type: "world-kv-delete",
        namespace: readString(payload.namespace, "namespace"),
        key: readString(payload.key, "key"),
        scope: readKvScope(payload.scope),
      });
    case "platform.invoke":
      return sendAction({
        type: "world-platform-invoke",
        feature: readString(payload.feature, "feature"),
        params: payload.params,
      });
    default:
      throw new Error(`不支持的世界逻辑操作：${request.operation}`);
  }
}

function readKvScope(value: unknown): KvScope | undefined {
  const scope = asRecord(value);
  const scopeName = scope?.scope;
  if (scopeName !== "world" && scopeName !== "session" && scopeName !== "character") {
    return undefined;
  }
  const result: KvScope = { scope: scopeName };
  if (scope && scopeName === "character" && typeof scope.character_id === "string") {
    result.character_id = scope.character_id;
  }
  return result;
}

function createWorkerSource(packageSource: string): string {
  const prefix = `
"use strict";
const __worldHandlers = new Map();
const __worldPending = new Map();
let __worldRequestSequence = 0;

function __worldRequest(operation, payload) {
  const requestId = "storage-" + (++__worldRequestSequence);
  return new Promise((resolve, reject) => {
    __worldPending.set(requestId, { resolve, reject });
    self.postMessage({ type: "storage-request", requestId, operation, payload });
  });
}

function __matchesWhere(record, where) {
  if (!where || typeof where !== "object") return true;
  return Object.entries(where).every(([field, expected]) => {
    const actual = record && record.data ? record.data[field] : undefined;
    if (!expected || typeof expected !== "object" || Array.isArray(expected)) {
      return actual === expected;
    }
    if (Object.prototype.hasOwnProperty.call(expected, "eq") && actual !== expected.eq) return false;
    if (Object.prototype.hasOwnProperty.call(expected, "ne") && actual === expected.ne) return false;
    if (Object.prototype.hasOwnProperty.call(expected, "gt") && !(actual > expected.gt)) return false;
    if (Object.prototype.hasOwnProperty.call(expected, "gte") && !(actual >= expected.gte)) return false;
    if (Object.prototype.hasOwnProperty.call(expected, "lt") && !(actual < expected.lt)) return false;
    if (Object.prototype.hasOwnProperty.call(expected, "lte") && !(actual <= expected.lte)) return false;
    if (Array.isArray(expected.in) && !expected.in.includes(actual)) return false;
    return true;
  });
}

const __worldRecords = Object.freeze({
  list: (collection) => __worldRequest("records.list", { collection }),
  create: (collection, data) => __worldRequest("records.create", { collection, data }),
  update: (collection, recordId, data) => __worldRequest("records.update", { collection, recordId, data }),
  remove: (collection, recordId) => __worldRequest("records.delete", { collection, recordId }),
  query: async (collection, options = {}) => {
    let records = await __worldRequest("records.list", { collection });
    records = records.filter((record) => __matchesWhere(record, options.where));
    if (Array.isArray(options.orderBy) && options.orderBy.length > 0) {
      const field = options.orderBy[0];
      const direction = options.orderBy[1] === "desc" ? -1 : 1;
      records.sort((left, right) => {
        const a = left && left.data ? left.data[field] : undefined;
        const b = right && right.data ? right.data[field] : undefined;
        return a === b ? 0 : a > b ? direction : -direction;
      });
    }
    const offset = Math.max(0, Number(options.offset) || 0);
    const limit = Math.min(1000, Math.max(0, Number(options.limit) || 1000));
    return records.slice(offset, offset + limit);
  },
});

// 作用域参数（可选）：{ scope: "world" | "session" | "character", characterId?: string }
// 缺省为 world（跨存档共享）；session 为世界存档级；character 需带 characterId。
const __kvScopeArgs = (options) => {
  if (!options || typeof options !== "object") return {};
  const scope = {};
  if (typeof options.scope === "string") scope.scope = options.scope;
  if (typeof options.characterId === "string") scope.character_id = options.characterId;
  return Object.keys(scope).length > 0 ? { scope } : {};
};

const __worldKv = Object.freeze({
  list: (namespace, options) => __worldRequest("kv.list", { namespace, ...__kvScopeArgs(options) }),
  get: async (namespace, key, fallback = null, options) => {
    const entry = await __worldRequest("kv.get", { namespace, key, ...__kvScopeArgs(options) });
    return entry ? entry.value : fallback;
  },
  set: (namespace, key, value, options) =>
    __worldRequest("kv.set", { namespace, key, value, ...__kvScopeArgs(options) }),
  remove: (namespace, key, options) =>
    __worldRequest("kv.delete", { namespace, key, ...__kvScopeArgs(options) }),
});

// 平台能力（第 12 项）：世界包调用目录内的平台 action（第一批为文件能力）。
// 结果直接 resolve；失败 reject 带 'code: 中文说明' 的错误（unsupported / not_declared /
// not_granted / invalid_params / io / cancelled），可用字符串前缀程序化判断。
const __worldPlatform = Object.freeze({
  invoke: (feature, params) => __worldRequest("platform.invoke", { feature, params: params ?? {} }),
});

const __worldApi = Object.freeze({ records: __worldRecords, kv: __worldKv, platform: __worldPlatform });
const world = Object.freeze({
  register(name, handler) {
    if (typeof name !== "string" || typeof handler !== "function") {
      throw new Error("world.register(name, handler) 需要传入函数。");
    }
    __worldHandlers.set(name, handler);
  },
});
Object.defineProperty(globalThis, "world", { value: world, writable: false, configurable: false });

(function registerWorldLogic() {
`;
  const suffix = `
})();

self.onmessage = async (event) => {
  const message = event.data || {};
  if (message.type === "storage-result") {
    const pending = __worldPending.get(message.requestId);
    if (!pending) return;
    __worldPending.delete(message.requestId);
    if (message.ok) pending.resolve(message.result);
    else pending.reject(new Error(message.error || "世界存储请求失败。"));
    return;
  }
  if (message.type !== "invoke") return;
  const handler = __worldHandlers.get(message.handler);
  if (!handler) {
    self.postMessage({ type: "result", ok: false, error: "World logic handler not found: " + message.handler });
    return;
  }
  try {
    const result = await handler(message.input, __worldApi);
    const encoded = JSON.stringify(result === undefined ? null : result);
    if (encoded && encoded.length > ${MAX_LOGIC_MESSAGE_BYTES}) {
      throw new Error("世界逻辑结果超过 ${MAX_LOGIC_MESSAGE_BYTES} 字节上限。");
    }
    self.postMessage({ type: "result", ok: true, result });
  } catch (error) {
    self.postMessage({
      type: "result",
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    });
  }
};
`;
  return `${prefix}\n${packageSource}\n${suffix}`;
}

function readString(value: unknown, label: string): string {
  if (typeof value !== "string" || !value.trim()) {
    throw new Error(`世界逻辑 ${label} 不能为空字符串。`);
  }
  return value.trim();
}

function readData(value: unknown): Record<string, unknown> {
  const record = asRecord(value);
  if (!record) {
    throw new Error("世界逻辑记录数据必须是对象。");
  }
  return record;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function assertMessageSize(value: unknown, label: string) {
  const encoded = JSON.stringify(value);
  if (encoded && encoded.length > MAX_LOGIC_MESSAGE_BYTES) {
    throw new Error(`${label} 超过 ${MAX_LOGIC_MESSAGE_BYTES} 字节上限。`);
  }
}
