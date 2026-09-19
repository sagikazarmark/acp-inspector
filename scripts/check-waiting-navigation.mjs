// Opt-in Linux WebKitGTK regression probe, using real X11 input and read-only
// DOM inspection. Start Testy and send `callbacks` before running this script.
// The Timeline must show its waiting-request "go to it" control, and the
// request must not already be marked by navigation from Trace.
import { execFileSync } from 'node:child_process';
import assert from 'node:assert/strict';

const ws = new WebSocket(process.env.WEBKIT_DEBUG_SOCKET ||
  'ws://127.0.0.1:9223/socket/1/1/WebPage');
let target, sequence = 10;
const pending = new Map();
const deadline = setTimeout(() => {
  console.error('FAIL: waiting-request navigation timed out');
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
function inspect(expression) {
  return new Promise(resolve => {
    const id = ++sequence;
    pending.set(id, resolve);
    ws.send(JSON.stringify({
      id: ++sequence, method: 'Target.sendMessageToTarget', params: {
        targetId: target,
        message: JSON.stringify({ id, method: 'Runtime.evaluate', params: {
          expression, returnByValue: true,
        } }),
      },
    }));
  });
}
const key = name => execFileSync('xdotool', ['key', '--clearmodifiers', name], {
  env: process.env, timeout: 2000,
});
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
try {
  await ready;
  assert.equal(await inspect("!!document.querySelector('.waiting button')"), true,
    'prepare a waiting request with Testy callbacks first');
  assert.equal(await inspect("!!document.querySelector('[data-frames].sought')"), false,
    'the regression requires a request not previously sought from Trace');
  // Reach the actual control with Tab, rather than focusing it through script.
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (await inspect("document.activeElement.matches('.waiting button')")) break;
    key('Tab');
    await delay(40);
  }
  assert.equal(await inspect("document.activeElement.matches('.waiting button')"), true);
  key('Return');
  // Longer than the production helper's bounded paint wait: a no-op cannot pass.
  await delay(1500);
  const result = JSON.parse(await inspect(`JSON.stringify({
    focusedRequest: document.activeElement.matches('[data-frames]') &&
      !!document.activeElement.querySelector('[data-slot="permission"], [data-slot="elicitation"]'),
    marked: document.activeElement.classList.contains('sought'),
    visible: !!document.activeElement.getClientRects().length,
    focus: document.activeElement.outerHTML.slice(0, 250)
  })`));
  console.log(result);
  assert.equal(result.focusedRequest, true, 'go to it must focus the waiting request row');
  assert.equal(result.visible, true);
  assert.equal(result.marked, false, 'local request navigation needs no Trace selection');
  console.log('PASS: go to it focuses an unmarked waiting request');
} finally {
  clearTimeout(deadline);
  ws.close();
}
