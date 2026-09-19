import { readFile } from 'node:fs/promises';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';

const source = await readFile(new URL('./clipboard.js', import.meta.url), 'utf8');
const tick = () => new Promise(resolve => setImmediate(resolve));

function mount(nativeRead) {
  const root = new EventTarget();
  root.isConnected = true;
  root.dataset = { imagePaste: 'true' };
  const output = [], reads = [], incoming = [['composer', 5]], waiting = [];
  const deliver = value => waiting.length ? waiting.shift()(value) : incoming.push(value);
  let removed;
  const done = runInNewContext(`(async()=>{${source}})()`, {
    queueMicrotask,
    navigator: {clipboard: {read: nativeRead}},
    document: { getElementById: () => ({ closest: () => root }), body: {} },
    dioxus: {
      recv: () => incoming.length ? Promise.resolve(incoming.shift()) : new Promise(resolve => waiting.push(resolve)),
      send: message => output.push(JSON.parse(JSON.stringify(message))),
    },
    MutationObserver: class { constructor(callback) { removed = callback; } observe() {} disconnect() {} },
    FileReader: class {
      readAsDataURL(file) {
        reads.push(file.name);
        this.result = `data:image/png;base64,${file.bytes}`;
        file.fail ? this.onerror() : this.onload();
      }
    },
  });
  function paste(files = [], types) {
    const event = new Event('paste', { cancelable: true });
    event.clipboardData = { files, types };
    root.dispatchEvent(event);
    assert.equal(event.defaultPrevented, false, 'text paste stays browser-owned');
  }
  return { root, output, reads, deliver, paste, async close() {
    root.isConnected = false; removed(); await done;
  } };
}

test('reserves before reading, blocks immediate Send, and reads only admitted images', async () => {
  const h = mount(); await tick();
  h.paste();
  h.paste([{ name: 'audio', type: 'audio/wav', size: 1 }]);
  await tick();
  h.deliver([]); h.deliver([]); await tick(); h.output.length = 0;
  const file = { name: 'image.png', type: 'image/png', size: 4, bytes: 'cG5n' };
  h.paste([file, file]);
  await tick();
  assert.deepEqual(h.output, [{ kind: 'reserve', names: ['image.png', 'image.png'] }]);
  assert.deepEqual(h.reads, []);
  const enter = new Event('keydown', { cancelable: true }); enter.key = 'Enter';
  h.root.dispatchEvent(enter);
  assert.equal(enter.defaultPrevented, true);
  h.deliver([7, null]); await tick();
  assert.deepEqual(h.reads, ['image.png']);
  assert.deepEqual(h.output[1], { kind: 'finish', id: 7, data: 'cG5n', error: null });
  await h.close();
});

test('opaque WebKit discovery precedes admission and bounds the returned blob', async () => {
  let resolve;
  const h = mount(() => new Promise(done => { resolve = done; })); await tick();
  h.paste([], []);
  await tick();
  assert.deepEqual(h.output, []);
  assert.deepEqual(h.reads, []);
  resolve([{types:['image/png', 'image/jpeg'], getType: async type => ({type, size:6})}]);
  await tick();
  assert.deepEqual(h.output, [{kind:'reserve', names:['Clipboard image']}]);
  h.deliver([9]); await tick();
  assert.match(h.output[1].error, /5 MiB/);
  assert.deepEqual(h.reads, []);
  await h.close();
});

test('size/read failures settle rows; gating and unmount prevent further work', async () => {
  const h = mount(); await tick();
  h.root.dataset.imagePaste = 'false';
  h.paste([{ type: 'image/png' }]); await tick(); h.deliver([]); await tick(); h.output.length = 0;
  h.root.dataset.imagePaste = 'true';
  h.paste([{ name: 'large', type: 'image/png', size: 6 }, { name: 'bad', type: 'image/png', size: 1, fail: true }]);
  await tick();
  h.deliver([1, 2]); await tick();
  assert.deepEqual(h.reads, ['bad']);
  assert.match(h.output[1].error, /5 MiB/);
  assert.match(h.output[2].error, /Could not read/);
  h.paste([{ name: 'late', type: 'image/png', size: 1 }]);
  await h.close();
  assert.deepEqual(h.reads, ['bad']);
});

test('mixed text/image probe and cancellation during discovery preserve text and skip reads', async () => {
  let resolve;
  const h = mount(() => new Promise(done => { resolve = done; })); await tick();
  h.paste([], ['text/plain']); await tick();
  assert.equal(h.output.length, 0);
  h.deliver({cancel:3}); await tick();
  resolve([{types:['image/png'],getType:async()=>({name:'cancelled',size:1})}]); await tick();
  h.deliver([3]); await tick();
  assert.deepEqual(h.reads, []);
  assert.equal(h.output.length, 1);
  await h.close();
});

test('text-only discovery reserves no rows and produces no false overflow', async () => {
  const h = mount(async () => [{types:['text/plain']}]); await tick();
  h.paste([], ['text/plain']); await tick(); h.deliver([]); await tick();
  assert.deepEqual(h.output[0], {kind:'reserve',names:[]});
  h.paste([], ['text/plain']); await tick(); h.deliver([]); await tick();
  assert.equal(h.output.length, 2);
  await h.close();
});
