async function copyToClipboard(text) {
  try {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.style.position = 'fixed';
    ta.style.left = '-9999px';
    document.body.appendChild(ta);
    ta.focus();
    ta.select();
    const ok = document.execCommand('copy');
    document.body.removeChild(ta);
    return ok;
  } catch {
    return false;
  }
}

function showToast(msg) {
  let el = document.getElementById('toast');
  if (!el) {
    el = document.createElement('div');
    el.id = 'toast';
    el.className = 'hidden fixed right-4 top-4 z-50 bg-surface-1 border border-card px-4 py-2 rounded-lg text-sm text-white';
    document.body.appendChild(el);
  }
  el.textContent = msg;
  el.classList.remove('hidden');
  clearTimeout(window.__toastTimer);
  window.__toastTimer = setTimeout(() => el.classList.add('hidden'), 1800);
}

async function refreshRaw() {
  const pre = document.getElementById('raw');
  try {
    const [statusRes, statsRes] = await Promise.all([
      fetch('api/status', { cache: 'no-store' }),
      fetch('api/stats', { cache: 'no-store' }),
    ]);
    const statusText = await statusRes.text();
    const statsText = await statsRes.text();

    const status = statusRes.ok ? JSON.parse(statusText) : { error: statusText, http: statusRes.status };
    const stats = statsRes.ok ? JSON.parse(statsText) : { error: statsText, http: statsRes.status };

    pre.textContent = JSON.stringify({ status, stats }, null, 2);
  } catch (e) {
    pre.textContent = String(e);
  }
}

document.getElementById('refreshRawBtn').addEventListener('click', refreshRaw);

document.getElementById('copyRawBtn').addEventListener('click', async () => {
  const text = document.getElementById('raw').textContent || '';
  const ok = await copyToClipboard(text);
  showToast(ok ? 'Copied' : 'Copy failed');
});

setInterval(() => {
  if (document.hidden) return;
  const on = document.getElementById('autoRefresh').checked;
  if (!on) return;
  refreshRaw();
}, 2000);

refreshRaw();
