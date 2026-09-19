// Clipboard bytes belong to the composer scope, never Dioxus's native path cache.
const [id, maxBytes] = await dioxus.recv();
const root = document.getElementById(id)?.closest('[data-slot=composer]');
if (root) {
  let pending = 0;
  const batches = [];
  let stop;
  const stopped = new Promise(resolve => { stop = resolve; });
  const cancelled = new Set();
  const readers = new Map();
  let work = Promise.resolve();
  let admission = Promise.resolve();
  const paste = event => {
    const allowed = root.dataset.imagePaste === 'true';
    const files = allowed ? Array.from(event.clipboardData?.files || []).filter(file => file.type.startsWith('image/')) : [];
    let discovery = Promise.resolve(files);
    if (allowed && !files.length && navigator.clipboard?.read) {
      // WebKitGTK exposes no DataTransfer types for native bitmap clipboards.
      // Read only during the user's paste gesture, never by polling.
      // The synchronous Rust paste fence preserves order while discovering
      // whether this snapshot actually contains an image. No speculative row.
      const snapshot = navigator.clipboard.read().then(items => {
        for (const item of items) {
          const type = item.types.find(type => type.startsWith('image/'));
          if (type) return {item, type};
        }
        return null;
      }).catch(error => { if (event.clipboardData?.types?.includes('text/plain')) return null; throw error; });
      snapshot.catch(() => {});
      discovery = snapshot.then(found => found ? [{name:'Clipboard image', load:() => found.item.getType(found.type)}] : [],
        error => [{name:'Clipboard image', load:() => Promise.reject(error)}]);
    }
    if (!root.isConnected) return;
    // Do not cancel paste: the browser owns text, including text alongside images.
    pending++;
    // Let Dioxus's synchronous paste handler mark the ordering fence first.
    queueMicrotask(() => {
      admission = admission.then(async () => {
        const files = await Promise.race([discovery, stopped]);
        if (!files || !root.isConnected) return;
        batches.push(files);
        dioxus.send({kind:'reserve', names:files.map(file => file.name || 'Clipboard image')});
      });
    });
  };
  // Native paste and Send can arrive before the reservation reaches Rust.
  const guard = event => {
    if (pending && (event.type === 'click' && event.target.closest('[aria-label="Send prompt"]') ||
        event.type === 'keydown' && event.key === 'Enter' && !event.shiftKey)) {
      event.preventDefault();
      event.stopImmediatePropagation();
    }
  };
  root.addEventListener('paste', paste);
  root.addEventListener('click', guard, true);
  root.addEventListener('keydown', guard, true);
  const removed = new MutationObserver(() => {
    if (!root.isConnected) {
      root.removeEventListener('paste', paste);
      root.removeEventListener('click', guard, true);
      root.removeEventListener('keydown', guard, true);
      batches.length = 0;
      for (const reader of readers.values()) reader.abort();
      stop(null);
      removed.disconnect();
    }
  });
  removed.observe(document.body, {childList:true, subtree:true});
  while (root.isConnected) {
    const reply = await Promise.race([dioxus.recv(), stopped]);
    if (!reply) break;
    if (reply.cancel !== undefined) {
      cancelled.add(reply.cancel);
      readers.get(reply.cancel)?.abort();
      continue;
    }
    const ids = reply;
    const files = batches.shift();
    if (!files || !root.isConnected) break;
    pending--;
    // Only admitted rows are read. Each batch reads serially; Rust settles all
    // completions in reservation order, including intervening picker/drop work.
    work = work.then(async () => {
      for (let index = 0; index < ids.length; index++) {
        const id = ids[index];
        if (id === null) continue;
        if (cancelled.has(id) || !root.isConnected) continue;
        let file = files[index];
        let data = null, error = null;
        try {
          if (file.load) file = await Promise.race([file.load(), stopped]);
          if (cancelled.has(id) || !root.isConnected) continue;
          if (file.size > maxBytes) throw Error('A media file may be at most 5 MiB.');
          data = await new Promise((resolve, reject) => {
            const reader = new FileReader();
            readers.set(id, reader);
            reader.onload = () => resolve(reader.result.split(',')[1]);
            reader.onerror = () => reject(Error('Could not read clipboard image.'));
            reader.onabort = () => reject(Error('Clipboard image read was cancelled.'));
            reader.readAsDataURL(file);
          });
        } catch (problem) { error = String(problem.message || problem); }
        readers.delete(id);
        if (root.isConnected && !cancelled.has(id)) dioxus.send({kind:'finish', id, data, error});
      }
    });
  }
}
