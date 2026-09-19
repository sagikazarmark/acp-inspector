// One bridge for explicit navigation, layout handoff and return-to-latest.
// All destinations arrive as data, never as executable Agent text.
// Claim ownership before awaiting the channel: delayed delivery is not a new intent.
const token = {};
document.inspectorNavigation = token;
const active = document.activeElement;
const intent = await dioxus.recv();
const visible = target => target?.getClientRects().length;
for (let attempt = 0; attempt < 60; attempt += 1) {
  await new Promise(requestAnimationFrame);
  if (document.inspectorNavigation !== token) return;
  if (document.querySelector('dialog[open]')) return;
  let target;
  switch (intent.kind) {
    case 'cancel': return;
    case 'control':
      target = document.getElementById(intent.id);
      if (target?.getAttribute('aria-selected') === 'false') target = null;
      break;
    case 'entry':
      // Local waiting-request navigation does not set .sought.
      target = document.querySelector(`[data-frames~="${intent.ordinal}"]`);
      break;
    case 'frames':
      target = intent.ordinals
        .map(ordinal => document.querySelector(`[data-frame="${ordinal}"] .frame-row`))
        .find(row => visible(row) && row.dataset.revealed === 'true');
      break;
    case 'layout':
      if (!document.querySelector('.spine-wire')) continue;
      if (!active?.closest('.turns, .rail') || visible(active)) return;
      target = document.querySelector('.wire-tabs [aria-selected="true"]');
      break;
    case 'latest':
      target = document.querySelector(`[data-tail="${intent.list}"]`);
      if (!visible(target) || target.dataset.pageLatest === 'false') continue;
      target.scrollTo({ top: 0 });
      return;
  }
  if (!visible(target)) continue;
  if (intent.kind === 'entry' || intent.kind === 'frames') {
    target.scrollIntoView({ block: 'center' });
  }
  target.focus({ preventScroll: true });
  return;
}
