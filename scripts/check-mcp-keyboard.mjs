// Linux native WebKitGTK acceptance probe. DOM access is read-only; xdotool
// supplies every key. See docs/native-acceptance.md for setup and sequences.
import { execFileSync } from 'node:child_process';
import assert from 'node:assert/strict';

const [expected, ...keys] = process.argv.slice(2);
if (!['stdio', 'http', 'sse'].includes(expected) || !keys.length) {
  console.error('Usage: node scripts/check-mcp-keyboard.mjs stdio|http|sse KEY ...');
  process.exit(2);
}
const ws = new WebSocket(process.env.WEBKIT_DEBUG_SOCKET ||
  'ws://127.0.0.1:9223/socket/1/1/WebPage');
let target, sequence = 10;
const pending = new Map();
const deadline = setTimeout(() => {
  console.error('FAIL: WebKit inspection timed out (12 seconds)');
  process.exit(1);
}, 12000);
const ready = new Promise((resolve, reject) => {
  ws.onerror = () => reject(new Error('Could not connect to WebKit inspection socket'));
  ws.onopen = () => ws.send(JSON.stringify({
    id: 1, method: 'Target.setPauseOnStart', params: { pauseOnStart: false },
  }));
  ws.onmessage = ({ data }) => {
    const response = JSON.parse(data);
    if (response.method === 'Target.targetCreated') {
      target = response.params.targetInfo.targetId;
      resolve();
    }
    if (response.method === 'Target.dispatchMessageFromTarget') {
      const message = JSON.parse(response.params.message);
      pending.get(message.id)?.(message.result?.result?.value);
      pending.delete(message.id);
    }
  };
});
function inspect() {
  return new Promise(resolve => {
    const id = ++sequence;
    pending.set(id, resolve);
    ws.send(JSON.stringify({
      id: ++sequence, method: 'Target.sendMessageToTarget', params: {
        targetId: target,
        message: JSON.stringify({ id, method: 'Runtime.evaluate', params: {
          expression: `JSON.stringify({
            focus: document.activeElement.getAttribute('aria-label'),
            transport: document.querySelector('dialog[open] select[aria-label="MCP server 1 transport"]')?.value,
            url: !!document.querySelector('dialog[open] input[aria-label="MCP server 1 URL"]'),
            command: !!document.querySelector('dialog[open] input[aria-label="MCP server 1 command"]')
          })`, returnByValue: true,
        } }),
      },
    }));
  });
}
try {
  await ready;
  for (const key of keys) {
    execFileSync('xdotool', ['key', '--clearmodifiers', key], {
      env: process.env, timeout: 2000,
    });
    await new Promise(resolve => setTimeout(resolve, 400));
    console.log(key, await inspect());
  }
  const state = JSON.parse(await inspect());
  assert.equal(state.transport, expected, 'the selected transport must be committed');
  assert.equal(state.focus, `MCP server 1 ${expected === 'stdio' ? 'command' : 'URL'}`,
    'Tab must reach the transport-specific field');
  assert.equal(state.url, expected !== 'stdio');
  assert.equal(state.command, expected === 'stdio');
  console.log(`PASS: native ${expected} selection reaches its field`);
} finally {
  clearTimeout(deadline);
  ws.close();
}
