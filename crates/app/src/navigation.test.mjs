import { readFile } from 'node:fs/promises';
import { createContext, runInContext } from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';

const source = await readFile(new URL('./navigation.js', import.meta.url), 'utf8');
function mount() {
  const nodes = new Map(), paints = [], effects = [];
  const document = {
    activeElement: null,
    querySelector: selector => nodes.get(selector),
    getElementById: id => nodes.get(id),
  };
  const context = createContext({ document, requestAnimationFrame: resolve => paints.push(resolve) });
  function node(key, shown = true, dataset = {}) {
    const value = {
      shown, dataset,
      getClientRects() { return this.shown ? [1] : []; },
      getAttribute: () => null,
      closest: () => null,
      focus() { document.activeElement = value; effects.push(['focus', key]); },
      scrollIntoView() { effects.push(['reveal', key]); },
      scrollTo() { effects.push(['tail', key]); },
    };
    nodes.set(key, value);
    return value;
  }
  function request(intent, receive = Promise.resolve(intent)) {
    context.dioxus = { recv: () => receive };
    return runInContext(`(async () => {${source}})()`, context);
  }
  async function paint() {
    await new Promise(resolve => setImmediate(resolve));
    paints.splice(0).forEach(resolve => resolve());
    await new Promise(resolve => setImmediate(resolve));
  }
  return { nodes, effects, document, node, request, paint };
}

test('new control intent cancels an older hidden Frame reveal', async () => {
  const h = mount();
  const old = h.node('[data-frame="7"] .frame-row', false, { revealed: 'true' });
  const pending = h.request({ kind: 'frames', ordinals: [7] });
  await h.paint();
  h.node('diagnostics');
  const latest = h.request({ kind: 'control', id: 'diagnostics' });
  await h.paint(); await latest;
  old.shown = true;
  await h.paint(); await pending;
  assert.deepEqual(h.effects, [['focus', 'diagnostics']]);
});

test('local waiting request without .sought supersedes pending navigation', async () => {
  const h = mount();
  const old = h.node('rail', false);
  const pending = h.request({ kind: 'control', id: 'rail' });
  await h.paint();
  const key = '[data-frames~="19"]';
  h.node(key);
  const waiting = h.request({ kind: 'entry', ordinal: 19 });
  await h.paint(); await waiting;
  old.shown = true;
  await h.paint(); await pending;
  assert.deepEqual(h.effects, [['reveal', key], ['focus', key]]);
});

test('ordinary layout choice cancels pending focus and preserves the active control', async () => {
  const h = mount();
  const old = h.node('[data-frames~="2"]', false);
  h.document.activeElement = h.node('split');
  const pending = h.request({ kind: 'entry', ordinal: 2 });
  await h.paint();
  const layout = h.request({ kind: 'cancel' });
  await h.paint(); await layout;
  old.shown = true;
  await h.paint(); await pending;
  assert.deepEqual(h.effects, []);
  assert.equal(h.document.activeElement, h.nodes.get('split'));
});

test('full-Wire layout hands off only hidden focus and can itself be superseded', async () => {
  for (const superseded of [false, true]) {
    const h = mount();
    const active = h.node('composer', false);
    active.closest = () => true;
    h.document.activeElement = active;
    const tab = '.wire-tabs [aria-selected="true"]';
    h.node(tab);
    const pending = h.request({ kind: 'layout' });
    await h.paint();
    if (superseded) {
      const cancel = h.request({ kind: 'cancel' });
      await h.paint(); await cancel;
    }
    h.node('.spine-wire');
    await h.paint(); await pending;
    assert.deepEqual(h.effects, superseded ? [] : [['focus', tab]]);
  }
});

test('latest waits for the final rendered page before scrolling', async () => {
  const h = mount();
  const list = h.node('[data-tail="frames"]', true, { pageLatest: 'false' });
  const pending = h.request({ kind: 'latest', list: 'frames' });
  await h.paint();
  assert.deepEqual(h.effects, []);
  list.dataset.pageLatest = 'true';
  await h.paint(); await pending;
  assert.deepEqual(h.effects, [['tail', '[data-tail="frames"]']]);
});

test('Frame reveal waits for its marked row and repeated requests focus it again', async () => {
  const h = mount();
  const key = '[data-frame="7"] .frame-row';
  const row = h.node(key, true, { revealed: 'false' });
  const pending = h.request({ kind: 'frames', ordinals: [6, 7] });
  await h.paint();
  assert.deepEqual(h.effects, []);
  row.dataset.revealed = 'true';
  await h.paint(); await pending;
  const repeated = h.request({ kind: 'frames', ordinals: [7] });
  await h.paint(); await repeated;
  assert.deepEqual(h.effects, [['reveal', key], ['focus', key], ['reveal', key], ['focus', key]]);
});

test('a newer destination cancels a pending return-to-latest scroll', async () => {
  const h = mount();
  const list = h.node('[data-tail="lines"]', true, { pageLatest: 'false' });
  const pending = h.request({ kind: 'latest', list: 'lines' });
  await h.paint();
  h.node('timeline');
  const latest = h.request({ kind: 'control', id: 'timeline' });
  await h.paint(); await latest;
  list.dataset.pageLatest = 'true';
  await h.paint(); await pending;
  assert.deepEqual(h.effects, [['focus', 'timeline']]);
});

test('late channel delivery cannot revive an older intent', async () => {
  const h = mount();
  let deliver;
  const pending = h.request(null, new Promise(resolve => { deliver = resolve; }));
  const cancel = h.request({ kind: 'cancel' });
  await h.paint(); await cancel;
  h.node('old');
  deliver({ kind: 'control', id: 'old' });
  await h.paint(); await pending;
  assert.deepEqual(h.effects, []);
});

test('a dialog prevents focus theft, and absent destinations stop after 60 paints', async () => {
  const h = mount();
  h.node('target'); h.node('dialog[open]');
  const blocked = h.request({ kind: 'control', id: 'target' });
  await h.paint(); await blocked;
  h.nodes.delete('dialog[open]');
  const missing = h.request({ kind: 'entry', ordinal: 99 });
  for (let i = 0; i < 60; i++) await h.paint();
  await missing;
  assert.deepEqual(h.effects, []);
});
