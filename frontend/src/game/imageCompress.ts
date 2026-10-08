/** 聊天发图前压缩：避免安卓 WebView 把原图 base64 撑爆导致闪退。 */

const DEFAULT_MAX_EDGE = 1280;
const DEFAULT_QUALITY = 0.82;
/** 约 1.2MB 以下且边长已很小的图可原样发送 */
const SKIP_BYTES = 400 * 1024;

/** 单张图片压缩后允许的最大体积（二进制字节）。base64 后约再膨胀 1/3。 */
export const MAX_CHAT_IMAGE_BYTES = 6 * 1024 * 1024;
/** 允许参与压缩的原图体积上限，超过这个尺寸直接拒绝，不再读进内存。 */
export const MAX_CHAT_IMAGE_SOURCE_BYTES = 40 * 1024 * 1024;

export type CompressImageOptions = {
  maxEdge?: number;
  quality?: number;
};

export type ImageInspection =
  | { ok: true }
  | { ok: false; reason: string };

/**
 * 按魔数判断文件的真实类型。
 *
 * 文件选择器的 accept / File.type 都不可信：Android WebView 的 content URI、
 * 手动切换到「所有文件」的桌面选择器，都会把任意文件标记为 image/*。
 * 曾经有一个 94MB 的 APK 被当作 image/png 塞进 LLM 请求，导致请求永久挂起。
 */
export async function inspectImageFile(file: File): Promise<ImageInspection> {
  if (file.size <= 0) {
    return { ok: false, reason: "文件为空" };
  }
  if (file.size > MAX_CHAT_IMAGE_SOURCE_BYTES) {
    return {
      ok: false,
      reason: `文件 ${(file.size / 1024 / 1024).toFixed(1)}MB，超过 ${MAX_CHAT_IMAGE_SOURCE_BYTES / 1024 / 1024}MB 上限`,
    };
  }

  let head: Uint8Array;
  try {
    head = new Uint8Array(await file.slice(0, 16).arrayBuffer());
  } catch {
    return { ok: false, reason: "无法读取文件内容" };
  }

  const is = (offset: number, ...bytes: number[]) =>
    bytes.every((byte, index) => head[offset + index] === byte);
  const tag = (offset: number) =>
    String.fromCharCode(...Array.from(head.slice(offset, offset + 4)));

  // 先识别常见的「非图片」，给出比「格式不支持」更明确的提示。
  if (is(0, 0x50, 0x4b, 0x03, 0x04)) {
    return { ok: false, reason: "这是一个 ZIP/APK 压缩包，不是图片" };
  }
  if (is(0, 0x25, 0x50, 0x44, 0x46)) {
    return { ok: false, reason: "这是一个 PDF 文档，不是图片" };
  }
  if (tag(4) === "ftyp") {
    return { ok: false, reason: "这是 HEIC/AVIF 或视频文件，暂不支持发送" };
  }

  const looksLikeImage =
    is(0, 0x89, 0x50, 0x4e, 0x47) || // PNG
    is(0, 0xff, 0xd8, 0xff) || // JPEG
    is(0, 0x47, 0x49, 0x46, 0x38) || // GIF
    (tag(0) === "RIFF" && tag(8) === "WEBP") || // WEBP
    is(0, 0x42, 0x4d); // BMP

  if (!looksLikeImage) {
    return { ok: false, reason: `「${file.name}」不是有效的图片文件` };
  }
  return { ok: true };
}

export async function compressImageForChat(
  file: File,
  options: CompressImageOptions = {},
): Promise<File> {
  if (!file.type.startsWith("image/")) {
    return file;
  }
  const maxEdge = options.maxEdge ?? DEFAULT_MAX_EDGE;
  const quality = options.quality ?? DEFAULT_QUALITY;

  try {
    const bitmap = await createImageBitmap(file);
    const longEdge = Math.max(bitmap.width, bitmap.height);
    const scale = longEdge > maxEdge ? maxEdge / longEdge : 1;
    const width = Math.max(1, Math.round(bitmap.width * scale));
    const height = Math.max(1, Math.round(bitmap.height * scale));

    if (scale >= 1 && file.size <= SKIP_BYTES && /image\/jpeg|image\/webp/i.test(file.type)) {
      bitmap.close?.();
      return file;
    }

    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d");
    if (!ctx) {
      bitmap.close?.();
      return file;
    }
    ctx.drawImage(bitmap, 0, 0, width, height);
    bitmap.close?.();

    const blob = await new Promise<Blob | null>((resolve) => {
      canvas.toBlob((result) => resolve(result), "image/jpeg", quality);
    });
    if (!blob || blob.size <= 0) {
      return file;
    }
    const name = file.name.replace(/\.[^.]+$/, "") + ".jpg";
    return new File([blob], name, { type: "image/jpeg", lastModified: Date.now() });
  } catch {
    return file;
  }
}

export async function compressImagesForChat(
  files: File[],
  options?: CompressImageOptions,
): Promise<File[]> {
  const out: File[] = [];
  for (const file of files) {
    out.push(await compressImageForChat(file, options));
  }
  return out;
}

export function fileToDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result ?? ""));
    reader.onerror = () => reject(reader.error ?? new Error("读取文件失败"));
    reader.readAsDataURL(file);
  });
}
