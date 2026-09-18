/** 聊天发图前压缩：避免安卓 WebView 把原图 base64 撑爆导致闪退。 */

const DEFAULT_MAX_EDGE = 1280;
const DEFAULT_QUALITY = 0.82;
/** 约 1.2MB 以下且边长已很小的图可原样发送 */
const SKIP_BYTES = 400 * 1024;

export type CompressImageOptions = {
  maxEdge?: number;
  quality?: number;
};

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
