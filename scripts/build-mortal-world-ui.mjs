import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(path.join(root, 'frontend/package.json'));
const ts = require('typescript');
const base = path.join(root, 'examples/world-packages/mortal-cultivation-journey');
const configPath = path.join(root, 'frontend/tsconfig.json');
const config = ts.readConfigFile(configPath, ts.sys.readFile);
const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, path.dirname(configPath), {}, configPath);
const program = ts.createProgram([...parsed.fileNames, path.join(base, 'ui.ts')], parsed.options);
const diagnostics = ts.getPreEmitDiagnostics(program);
if (diagnostics.length) {
  console.error(ts.formatDiagnosticsWithColorAndContext(diagnostics, { getCurrentDirectory: () => root, getCanonicalFileName: f => f, getNewLine: () => '\n' }));
  process.exit(1);
}
const compiled = ts.transpileModule(fs.readFileSync(path.join(base, 'ui.ts'), 'utf8'), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText.replace(/export\s*\{\s*\};?\s*$/, '');
const code = `\n(() => {\n${compiled}\n})();\n`.replace(/<\/script/gi, '<\\/script');
const css = fs.readFileSync(path.join(base, 'ui.css'), 'utf8');
for (const platform of ['desktop', 'mobile']) {
  const filename = path.join(base, 'world', `ui.${platform}.html`);
  const source = fs.readFileSync(filename, 'utf8');
  const scriptStart = source.indexOf('<script>'), scriptEnd = source.lastIndexOf('</script>');
  if (scriptStart < 0 || scriptEnd < scriptStart) throw new Error(`Missing script boundaries: ${filename}`);
  let html = source.slice(0, scriptStart) + '<script>' + code + source.slice(scriptEnd);
  html = html.replace(/\n?<style data-html-runtime>[\s\S]*?<\/style>\n?/, '\n');
  html = html.replace('</head>', `<style data-html-runtime>\n${css}\n</style></head>`);
  html = html.replace(/<body(?: data-platform="[^"]+")?>/, `<body data-platform="${platform}">`);
  // Keep markup editable in each HTML file; only the bounded generated style/script blocks change.
  fs.writeFileSync(filename, html, 'utf8');
  console.log(`Built ${path.relative(root, filename)}`);
}
