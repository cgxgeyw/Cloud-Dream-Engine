import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(path.join(root, 'frontend/package.json'));
const ts = require('typescript');
const configPath = path.join(root, 'frontend/tsconfig.json');
const config = ts.readConfigFile(configPath, ts.sys.readFile);
const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, path.dirname(configPath), {}, configPath);

const allTargets = [
  {
    name: 'accounting-assistant',
    base: path.join(root, 'examples/world-packages/accounting-assistant'),
    source: 'ui.ts',
    html: ['world/ui.desktop.html', 'world/ui.mobile.html'],
    css: ['ui.desktop.css', 'ui.mobile.css'],
  },
  {
    name: 'healthy-life',
    base: path.join(root, 'examples/world-packages/healthy-life'),
    source: 'ui.ts',
    html: ['world/ui.desktop.html', 'world/ui.mobile.html'],
    css: ['ui.desktop.css', 'ui.mobile.css'],
  },
  {
    name: 'blue-fat-fish',
    base: path.join(root, 'examples/world-packages/blue-fat-fish'),
    source: 'ui.ts',
    html: ['world/ui.desktop.html', 'world/ui.mobile.html'],
    css: ['world/ui.desktop.css', 'world/ui.mobile.css'],
  },
  {
    name: 'schedule-assistant',
    base: path.join(root, 'src-tauri/src/db/seeds/assets'),
    source: 'schedule-assistant-ui.ts',
    html: ['schedule-assistant-desktop-ui.html', 'schedule-assistant-mobile-ui.html'],
    css: ['schedule-assistant-desktop-ui.css', 'schedule-assistant-mobile-ui.css'],
  },
];

function compile(sourcePath) {
  const program = ts.createProgram([...parsed.fileNames, sourcePath], parsed.options);
  const diagnostics = ts.getPreEmitDiagnostics(program);
  if (diagnostics.length) {
    console.error(ts.formatDiagnosticsWithColorAndContext(diagnostics, {
      getCurrentDirectory: () => root,
      getCanonicalFileName: (file) => file,
      getNewLine: () => '\n',
    }));
    process.exit(1);
  }
  return ts.transpileModule(fs.readFileSync(sourcePath, 'utf8'), {
    compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
  }).outputText.replace(/export\s*\{\s*\};?\s*$/, '');
}

function inject(source, css, code, platform) {
  let html = source;
  if (!/<style data-world-css>[\s\S]*?<\/style>/.test(html)) throw new Error('Missing <style data-world-css>');
  if (!/<script data-world-script>[\s\S]*?<\/script>/.test(html)) throw new Error('Missing <script data-world-script>');
  html = html.replace(/<style data-world-css>[\s\S]*?<\/style>/, `<style data-world-css>\n${css}\n</style>`);
  const script = `\n(() => {\n${code}\n})();\n`.replace(/<\/script/gi, '<\\/script');
  html = html.replace(/<script data-world-script>[\s\S]*?<\/script>/, `<script data-world-script>${script}</script>`);
  html = html.replace(/<body(?: data-platform="[^"]+")?>/, `<body data-platform="${platform}">`);
  return html;
}

const only = process.argv.slice(2);
const targets = only.length ? allTargets.filter((item) => only.includes(item.name)) : allTargets;

for (const target of targets) {
  const compiled = compile(path.join(target.base, target.source));
  for (const [index, htmlFile] of target.html.entries()) {
    const platform = index === 0 ? 'desktop' : 'mobile';
    const htmlPath = path.join(target.base, htmlFile);
    const cssPath = path.join(target.base, target.css[index]);
    if (!fs.existsSync(htmlPath)) throw new Error(`Missing HTML shell: ${htmlPath}`);
    if (!fs.existsSync(cssPath)) throw new Error(`Missing stylesheet: ${cssPath}`);
    const output = inject(
      fs.readFileSync(htmlPath, 'utf8'),
      fs.readFileSync(cssPath, 'utf8'),
      compiled,
      platform,
    );
    fs.writeFileSync(htmlPath, output, 'utf8');
    console.log(`built ${target.name} ${platform} -> ${path.relative(root, htmlPath)}`);
  }
}
