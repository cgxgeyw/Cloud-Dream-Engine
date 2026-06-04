const fs = require('fs');
const content = fs.readFileSync('E:\\code\\rustweb\\examples\\world-packages\\_piao-mobile.jsonc', 'utf8');
fs.writeFileSync('E:\\code\\rustweb\\examples\\world-packages\\piao-v2-ui\\mobile-ui.jsonc', content, 'utf8');
console.log('Written piao mobile:', content.length, 'bytes');
