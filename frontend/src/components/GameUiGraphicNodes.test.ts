import { describe, expect, it } from "vitest";

import { parseGameUiDocument } from "../data/gameUi/parser";
import type {
  GameUiCanvasNode,
  GameUiStackNode,
  GameUiSvgNode,
} from "../data/gameUi/types";

function parseDesktop(source: string) {
  return parseGameUiDocument(source, "desktop");
}

const SVG_DOC = `
{
  "schema_version": 2,
  "layout": {
    "root": {
      "type": "stack",
      "children": [
        {
          "type": "svg",
          "view_box": "0 0 100 100",
          "class_name": "wheel",
          "children": [
            { "type": "svg_circle", "cx": "50", "cy": "50", "r": "40", "fill": "#eee" },
            { "type": "svg_path", "d": "M50 10 L50 50", "stroke": "#333", "stroke_width": "2", "rotate": "45" },
            { "type": "svg_text", "x": "50", "y": "20", "text": "{{ data.label }}", "text_anchor": "middle" },
            { "type": "svg_unknown", "cx": "1" }
          ]
        }
      ]
    }
  }
}`;

describe("svg node parsing", () => {
  it("keeps view_box and every supported primitive", () => {
    const parsed = parseDesktop(SVG_DOC);
    expect(parsed.error).toBeNull();
    const svg = (parsed.document.layout.root as GameUiStackNode).children?.[0] as GameUiSvgNode;
    expect(svg.type).toBe("svg");
    expect(svg.view_box).toBe("0 0 100 100");
    expect(svg.preserve_aspect_ratio).toBeUndefined();
    expect(svg.children?.map((child) => child.type)).toEqual([
      "svg_circle",
      "svg_path",
      "svg_text",
    ]);
  });

  it("drops unknown primitives instead of failing the whole document", () => {
    const parsed = parseDesktop(SVG_DOC);
    const svg = (parsed.document.layout.root as GameUiStackNode).children?.[0] as GameUiSvgNode;
    // 未知图元被静默跳过：画面少一笔，好过整页打不开。
    expect(JSON.stringify(svg)).not.toContain("svg_unknown");
    expect(parsed.error).toBeNull();
  });

  it("defaults view_box so a bare svg node still renders", () => {
    const parsed = parseDesktop(
      '{ "schema_version": 2, "layout": { "root": { "type": "svg" } } }',
    );
    const svg = parsed.document.layout.root as GameUiSvgNode;
    expect(svg.type).toBe("svg");
    expect(svg.view_box).toBe("0 0 100 100");
  });

  it("narrows stroke_linejoin to the values SVG accepts", () => {
    const parsed = parseDesktop(`
      {
        "schema_version": 2,
        "layout": {
          "root": {
            "type": "svg",
            "children": [
              { "type": "svg_rect", "x": "0", "y": "0", "width": "1", "height": "1",
                "stroke_linejoin": "miter", "stroke_linecap": "bogus" }
            ]
          }
        }
      }`);
    const svg = parsed.document.layout.root as GameUiSvgNode;
    const rect = svg.children?.[0];
    expect(rect?.stroke_linejoin).toBe("miter");
    // 非法取值被丢弃而不是透传给 DOM。
    expect(rect?.stroke_linecap).toBeUndefined();
  });
});

const CANVAS_DOC = `
{
  "schema_version": 2,
  "layout": {
    "root": {
      "type": "stack",
      "children": [
        {
          "type": "canvas",
          "width": "240",
          "height": "120",
          "ops": [
            { "type": "clear", "color": "#fff" },
            { "type": "arc", "x": "120", "y": "60", "radius": "50", "start_angle": "0", "end_angle": "90", "fill": "#f00" },
            { "type": "text", "x": "10", "y": "20", "text": "{{ data.label }}" },
            { "type": "nuke", "x": "0" }
          ]
        }
      ]
    }
  }
}`;

describe("canvas node parsing", () => {
  it("keeps declared ops and drops unknown ones", () => {
    const parsed = parseDesktop(CANVAS_DOC);
    expect(parsed.error).toBeNull();
    const canvas = (parsed.document.layout.root as GameUiStackNode).children?.[0] as GameUiCanvasNode;
    expect(canvas.type).toBe("canvas");
    expect(canvas.width).toBe("240");
    expect(canvas.ops?.map((op) => op.type)).toEqual(["clear", "arc", "text"]);
  });

  it("defaults text alignment and canvas size", () => {
    const parsed = parseDesktop(
      '{ "schema_version": 2, "layout": { "root": { "type": "canvas", "ops": [ { "type": "text", "x": "0", "y": "0", "text": "hi" } ] } } }',
    );
    const canvas = parsed.document.layout.root as GameUiCanvasNode;
    expect(canvas.width).toBe("300");
    expect(canvas.height).toBe("150");
    expect(canvas.ops?.[0]).toMatchObject({ type: "text", text_align: "left" });
  });

  it("never carries executable payloads through", () => {
    // 注意别在模板字符串里写 `https://` 之类含 `//` 的值——jsonc-parser 会当行注释。
    const parsed = parseDesktop(`
      {
        "schema_version": 2,
        "layout": {
          "root": {
            "type": "canvas",
            "ops": [
              { "type": "circle", "x": "1", "y": "1", "radius": "1", "fill": "#000",
                "script": "globalThis.__pwned = true" }
            ]
          }
        }
      }`);
    const canvas = parsed.document.layout.root as GameUiCanvasNode;
    // 只保留白名单字段，script 这类多余键在归一化阶段就被丢掉。
    expect(JSON.stringify(canvas)).not.toContain("script");
    expect(canvas.ops?.[0]).toEqual({
      type: "circle",
      x: "1",
      y: "1",
      radius: "1",
      fill: "#000",
    });
  });
});