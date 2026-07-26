import type { WorldLogicConfig } from "../data/gameUi";
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
    throw new Error("This world package does not provide sandbox JavaScript logic.");
  }
  if (!/^[a-zA-Z0-9._-]{1,128}$/.test(handler)) {
    throw new Error("World logic handler name is invalid.");
  }
  assertMessageSize(input, "World logic input");

  const workerSource = createWorkerSource(config.source);
  const objectUrl = URL.createObjectURL(new Blob([workerSource], { type: "text/javascript" }));
  const worker = new Worker(objectUrl, { name: `world-logic:${handler}` });
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
      finish(() => reject(new Error(`World logic exceeded ${config.timeout_ms} ms and was terminated.`)));
    }, config.timeout_ms);

    worker.onerror = (event) => {
      finish(() => reject(new Error(event.message || "World logic worker failed.")));
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
        finish(() => reject(new Error(result.error || "World logic failed.")));
      }
    };
    worker.postMessage({ type: "invoke", handler, input });
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
      });
    case "kv.get":
      return sendAction({
        type: "world-kv-get",
        namespace: readString(payload.namespace, "namespace"),
        key: readString(payload.key, "key"),
      });
    case "kv.set":
      return sendAction({
        type: "world-kv-set",
        namespace: readString(payload.namespace, "namespace"),
        key: readString(payload.key, "key"),
        value: payload.value,
      });
    case "kv.delete":
      return sendAction({
        type: "world-kv-delete",
        namespace: readString(payload.namespace, "namespace"),
        key: readString(payload.key, "key"),
      });
    case "platform.invoke":
      return sendAction({
        type: "world-platform-invoke",
        feature: readString(payload.feature, "feature"),
        params: payload.params,
      });
    default:
      throw new Error(`Unsupported world logic operation: ${request.operation}`);
  }
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
      throw new Error("world.register(name, handler) requires a function handler.");
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
    else pending.reject(new Error(message.error || "World storage request failed."));
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
      throw new Error("World logic result exceeds the ${MAX_LOGIC_MESSAGE_BYTES} byte limit.");
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
    throw new Error(`World logic ${label} must be a non-empty string.`);
  }
  return value.trim();
}

function readData(value: unknown): Record<string, unknown> {
  const record = asRecord(value);
  if (!record) {
    throw new Error("World logic record data must be an object.");
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
    throw new Error(`${label} exceeds the ${MAX_LOGIC_MESSAGE_BYTES} byte limit.`);
  }
}
