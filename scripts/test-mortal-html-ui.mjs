import { chromium } from 'playwright';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import assert from 'node:assert/strict';
import fs from 'node:fs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(path.join(root, 'frontend/package.json'));
const { createServer, build, preview } = await import(pathToFileURL(require.resolve('vite')).href);
const config = { root: path.join(root, 'frontend'), configFile: path.join(root, 'frontend/vite.config.ts') };
const production = process.argv.includes('--production');
let closeServer;
if (production) {
  const outDir = path.join(root, 'output/mortal-runtime-test');
  await build({ ...config, build: { outDir, emptyOutDir: false, rollupOptions: { input: path.join(root, 'frontend/mortal-html-harness.html') } } });
  const server = await preview({ root: config.root, configFile: false, build: { outDir }, preview: { host: '127.0.0.1', port: 8897, strictPort: true, open: false } });
  closeServer = () => new Promise(resolve => server.httpServer.close(resolve));
} else {
  const server = await createServer({ ...config, server: { port: 8897, strictPort: true, open: false } });
  await server.listen(); closeServer = () => server.close();
}
const browser = await chromium.launch();
try {
  fs.mkdirSync(path.join(root, 'output/mortal-ui-shots'), { recursive: true });
  for (const platform of ['desktop', 'mobile']) {
    const page = await browser.newPage({ viewport: platform === 'desktop' ? { width: 1440, height: 900 } : { width: 390, height: 844 } });
    const errors = [];
    page.on('console', msg => { if (msg.type() === 'error') errors.push(msg.text()); });
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(`http://127.0.0.1:8897/mortal-html-harness.html?platform=${platform}`);
    const frame = page.frameLocator('iframe');
    await frame.locator('#world-name').filter({ hasText: '小石村' }).waitFor();
    assert.equal(await page.locator('iframe').getAttribute('sandbox'), 'allow-scripts');
    if (platform === 'mobile') await frame.locator('#handle').click();
    await frame.locator('.topology svg').waitFor({ state: 'visible' });
    assert.equal(await frame.locator('[data-node-id]').count(), 6);
    assert.equal(await frame.locator('[data-edge-id]').count(), 6);
    const box = await frame.locator('.topology svg').getAttribute('viewBox');
    await frame.getByRole('button', { name: '放大', exact: true }).click();
    assert.notEqual(await frame.locator('.topology svg').getAttribute('viewBox'), box);
    await frame.getByRole('button', { name: '适应视图', exact: true }).click();
    assert.equal(await frame.locator('.topology svg').getAttribute('viewBox'), box);
    await page.screenshot({ path: path.join(root, `output/mortal-ui-shots/${platform}-map.png`) });
    await frame.getByRole('button', { name: '状态', exact: true }).click();
    await frame.locator('[role=meter]').first().waitFor();
    assert.equal(await frame.locator('[role=meter]').count(), 3);
    await frame.getByRole('button', { name: '背包', exact: true }).click();
    await frame.locator('.attribute-text').filter({ hasText: '干粮' }).waitFor();
    if (platform === 'mobile') await frame.locator('#close').click();
    assert.equal(await frame.locator('.game-cot').count(), 2);
    const cot = frame.locator('.game-cot').last();
    assert.equal((await cot.locator('.game-cot-content').textContent()).length, 200);
    await cot.getByRole('button', { name: '展开全部' }).click();
    assert.ok((await cot.locator('.game-cot-content').textContent()).length > 200);
    await page.evaluate(() => window.pushSnapshot({ visible_characters: ['韩立'] }));
    assert.ok((await cot.locator('.game-cot-content').textContent()).length > 200, 'snapshot must retain expanded reasoning');
    const interaction = frame.locator('.message--interaction');
    await interaction.getByRole('button', { name: '沿山脊前进' }).click();
    await page.waitForFunction(() => window.received.some(a => a.type === 'answer-interaction'));
    const answered = await page.evaluate(() => window.received.find(a => a.type === 'answer-interaction'));
    assert.equal(answered.answer, 'ridge');
    await interaction.getByText('已选择：沿山脊前进').waitFor();
    const confirm = frame.locator('.message--interaction').filter({ hasText: '是否开始入门试炼？' });
    await confirm.getByRole('button', { name: '开始试炼' }).click();
    await page.waitForFunction(() => window.received.some(a => a.type === 'answer-interaction' && a.messageId === 'confirm-interaction'));
    const confirmed = await page.evaluate(() => window.received.find(a => a.type === 'answer-interaction' && a.messageId === 'confirm-interaction'));
    assert.equal(confirmed.answer, true);
    await confirm.getByText('已选择：开始试炼').waitFor();
    const slider = frame.locator('.message--interaction').filter({ hasText: '选择行进速度' });
    await slider.locator('input[type=range]').evaluate(element => { element.value = '7'; element.dispatchEvent(new Event('input', { bubbles: true })); });
    await slider.getByRole('button', { name: '确认' }).click();
    await page.waitForFunction(() => window.received.some(a => a.type === 'answer-interaction' && a.messageId === 'slider-interaction'));
    const slid = await page.evaluate(() => window.received.find(a => a.type === 'answer-interaction' && a.messageId === 'slider-interaction'));
    assert.equal(slid.answer, 7);
    await slider.getByText('已选择：7').waitFor();
    // Clicking a button blurs input; an unrelated snapshot must not erase the local draft.
    await frame.locator('#input').fill('走上石阶。');
    await frame.locator('#world-name').click();
    await page.evaluate(() => window.pushSnapshot({ draft_input: { value: '', images: [], audios: [] } }));
    assert.equal(await frame.locator('#input').inputValue(), '走上石阶。');
    await frame.locator('#send').click();
    await page.waitForFunction(() => window.received.some(a => a.type === 'submit-message'));
    await frame.locator('.message-content').filter({ hasText: '走上石阶。' }).waitFor();
    const sent = await page.evaluate(() => window.received.find(a => a.type === 'submit-message'));
    assert.equal(sent.options.content, '走上石阶。');
    await page.waitForFunction(() => document.querySelector('iframe') !== null);
    await frame.locator('#send').waitFor({ state: 'visible' });
    await frame.locator('#input').fill('再次观察山门。');
    await page.evaluate(() => { window.failSubmit = true; });
    await frame.locator('#send').click();
    await frame.locator('#status[role=alert]').filter({ hasText: '模型服务不可用' }).waitFor();
    await frame.locator('#world-sdk-error').filter({ hasText: '模型服务不可用' }).waitFor();
    assert.equal(await frame.locator('#input').inputValue(), '再次观察山门。');
    assert.equal(await frame.locator('#send').isEnabled(), true);
    await page.evaluate(() => { window.failSubmit = false; });
    await frame.locator('#send').click();
    await frame.locator('.message-content').filter({ hasText: '再次观察山门。' }).waitFor();
    assert.equal(await frame.locator('#world-sdk-error').count(), 0);
    await page.screenshot({ path: path.join(root, `output/mortal-ui-shots/${platform}-chat.png`) });
    assert.deepEqual(errors, [], 'No console or sandbox errors');
    console.log(`${platform}: topology, zoom, tabs, attributes, inventory, CoT, interaction, draft, send, failure, retry passed`);
    await page.close();
  }
} finally {
  await browser.close();
  await closeServer();
}
