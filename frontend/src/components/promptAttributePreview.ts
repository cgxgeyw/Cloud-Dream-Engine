import type { AttributeSchemaResponse } from "../data/types";

export type PromptPreviewModule = Record<string, unknown>;

export function buildDirectorAttributePreview(
  modules: PromptPreviewModule[],
  schemas: AttributeSchemaResponse[],
) {
  const modulesById = new Map(modules.map((module) => [String(module.id ?? ""), module]));
  const labels = new Map<string, string>();
  for (const schema of schemas) {
    const prefix = schema.scope === "world" ? "world_attribute:" : "character_attribute:";
    const id = `${prefix}${schema.key}`;
    labels.set(id, schema.label || schema.key);
    if (!modulesById.has(id)) {
      modulesById.set(id, {
        id,
        source: "属性定义默认值 · 当前预览没有运行时值",
        content: {
          key: schema.key,
          label: schema.label,
          value_type: schema.value_type,
          description: schema.description,
          enum_options: schema.enum_options,
          default_value: schema.default_value,
        },
        sent: false,
        preview_only: true,
      });
    }
  }
  const attributes = Array.from(modulesById.values()).filter((module) => {
    const id = String(module.id ?? "");
    return id.startsWith("world_attribute:") || id.startsWith("character_attribute:");
  });
  if (!modulesById.has("attributes")) {
    modulesById.set("attributes", {
      id: "attributes",
      source: "属性汇总，仅供预览。实际发送内容和排序见各属性条目；缺少运行时值时展示定义默认值。",
      content: attributes.length ? attributes.map((module) => ({
        attribute: labels.get(String(module.id)) ?? module.id,
        value_source: module.preview_only === true ? "默认值" : "Prompt 预览值",
        content: module.content,
      })) : "当前世界没有可预览的自定义属性。可在世界属性或角色共享属性中添加定义。",
      sent: false,
      preview_only: true,
      preview_label: "属性汇总",
    });
  }
  return { modulesById, labels };
}
