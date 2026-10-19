import { describe, expect, it } from 'vitest';
import { parseGameUiDocument, stringifyGameUiDocument } from './parser';

describe('HTML world UI', () => {
  it('accepts a normal HTML page as the world UI document', () => {
    const source = '<!doctype html><html><body><main id="app">hello</main><script>document.title="world"</script></body></html>';
    const parsed = parseGameUiDocument(source, 'desktop');
    expect(parsed.error).toBeNull();
    expect(parsed.usedFallback).toBe(false);
    expect(parsed.document.html).toBe(source);
    expect(stringifyGameUiDocument(parsed.document)).toBe(source);
  });

  it('rejects the old JSONC document format', () => {
    const parsed = parseGameUiDocument('{"schema_version":2,"layout":{"root":{"type":"stack","children":[]}}}', 'desktop');
    expect(parsed.usedFallback).toBe(true);
    expect(parsed.error).toContain('HTML');
    expect(parsed.diagnostics.some((item) => item.code === 'ui.legacy_dsl')).toBe(true);
  });

  it('reports unsafe or incomplete HTML before preview', () => {
    const parsed = parseGameUiDocument('<div><iframe src="https://example.test"></iframe><form></form></div>', 'mobile');
    expect(parsed.usedFallback).toBe(true);
    expect(parsed.diagnostics.map((item) => item.code)).toEqual(expect.arrayContaining([
      'ui.document_structure',
      'ui.nested_frame',
      'ui.mobile_viewport',
      'ui.form_submit',
    ]));
  });
});
