let lastFilteredWorkers = [];
let lastFilteredBlocks = [];
let lastInternalCpuWorker = null;

const CACHE_KEYS = {
  status: 'ks_bridge_status_v1',
  stats: 'ks_bridge_stats_v1',
  updatedMs: 'ks_bridge_updated_ms_v1',
};

const WALLET_FILTER_KEY = 'ks_bridge_wallet_filter_v1';
const WORKER_ORDER_KEY = 'ks_bridge_worker_order_v1';
const BLOCKS_DAY_FILTER_KEY = 'ks_bridge_blocks_day_filter_v1';

function normalizeWalletFilter(value) {
  return String(value ?? '').trim();
}

function getWorkerKey(worker) {
  // Create a unique key for a worker based on instance, worker name, and wallet
  const instance = String(worker?.instance ?? '').trim();
  const workerName = String(worker?.worker ?? '').trim();
  const wallet = String(worker?.wallet ?? '').trim();
  return `${instance}|${workerName}|${wallet}`;
}

function readWorkerOrder() {
  try {
    const stored = localStorage.getItem(WORKER_ORDER_KEY);
    if (!stored) return [];
    return JSON.parse(stored);
  } catch {
    return [];
  }
}

function writeWorkerOrder(order) {
  try {
    localStorage.setItem(WORKER_ORDER_KEY, JSON.stringify(order || []));
  } catch {
    // ignore
  }
}

function maintainWorkerOrder(existingWorkers, newWorkers) {
  // existingWorkers: array of worker keys in the desired order
  // newWorkers: array of worker objects from API
  const order = [...existingWorkers];
  const seen = new Set(existingWorkers);
  const workerMap = new Map();
  
  // Create a map of worker key -> worker object
  for (const w of newWorkers) {
    const key = getWorkerKey(w);
    workerMap.set(key, w);
  }
  
  // Remove workers that no longer exist
  const filteredOrder = order.filter(key => workerMap.has(key));
  
  // Add new workers at the end
  for (const w of newWorkers) {
    const key = getWorkerKey(w);
    if (!seen.has(key)) {
      filteredOrder.push(key);
      seen.add(key);
    }
  }
  
  // Return sorted workers array based on the maintained order
  const sorted = [];
  for (const key of filteredOrder) {
    const worker = workerMap.get(key);
    if (worker) {
      sorted.push(worker);
    }
  }
  
  // Update stored order
  writeWorkerOrder(filteredOrder);
  
  return sorted;
}

function formatHashrateHs(hs) {
  if (!hs || !Number.isFinite(hs)) return '-';
  const units = ['H/s','kH/s','MH/s','GH/s','TH/s','PH/s','EH/s'];
  let v = hs;
  let i = 0;
  while (v >= 1000 && i < units.length - 1) { v /= 1000; i++; }
  return `${v.toFixed(2)} ${units[i]}`;
}

function setText(id, value) {
  const el = document.getElementById(id);
  if (!el) return;
  el.textContent = value == null || value === '' ? '-' : String(value);
}

function setInternalCpuCardsVisible(visible) {
  const hashrateEl = document.getElementById('internalCpuHashrate');
  const blocksEl = document.getElementById('internalCpuBlocks');
  const cards = [hashrateEl?.parentElement, blocksEl?.parentElement].filter(Boolean);
  for (const card of cards) {
    card.classList.toggle('hidden', !visible);
  }
}

function formatDifficulty(d) {
  const n = Number(d);
  if (!Number.isFinite(n) || n <= 0) return '-';
  // show in scientific-ish compact form similar to terminal
  if (n >= 1e12) return `${(n/1e12).toFixed(2)}T`;
  if (n >= 1e9) return `${(n/1e9).toFixed(2)}G`;
  if (n >= 1e6) return `${(n/1e6).toFixed(2)}M`;
  if (n >= 1e3) return `${(n/1e3).toFixed(2)}K`;
  return n.toFixed(2);
}

function shortHash(h) {
  if (!h) return '-';
  return h.length > 18 ? `${h.slice(0, 10)}...${h.slice(-6)}` : h;
}

function formatUnixSeconds(ts) {
  const n = Number(ts);
  if (!Number.isFinite(n) || n <= 0) return '-';
  try {
    return new Date(n * 1000).toLocaleString();
  } catch {
    return String(ts);
  }
}

function formatServerTime(date, isMobile = false) {
  if (!date || !(date instanceof Date)) return '-';
  try {
    if (isMobile) {
      // Compact format for mobile: "Jan 15, 3:45 PM"
      const options = { 
        month: 'short', 
        day: 'numeric',
        hour: 'numeric',
        minute: '2-digit',
        hour12: true
      };
      return date.toLocaleString('en-US', options);
    } else {
      // Full format for desktop: "Mon, Jan 15, 2024 3:45:30 PM"
      const options = { 
        weekday: 'short', 
        year: 'numeric', 
        month: 'short', 
        day: 'numeric',
        hour: 'numeric',
        minute: '2-digit',
        second: '2-digit',
        hour12: true
      };
      return date.toLocaleString('en-US', options);
    }
  } catch {
    return date.toLocaleString();
  }
}

function updateServerTime() {
  const el = document.getElementById('serverTime');
  if (!el) return;
  // Check if mobile based on window width (matches Tailwind's md breakpoint: 768px)
  const isMobile = window.innerWidth < 768;
  el.textContent = formatServerTime(new Date(), isMobile);
}

function getBlocksDayFilter() {
  const el = document.getElementById('blocksDayFilter');
  if (!el) return 0;
  const value = Number(el.value);
  return Number.isFinite(value) && value >= 0 ? value : 0;
}

function setBlocksDayFilter(value) {
  const el = document.getElementById('blocksDayFilter');
  if (!el) return;
  const v = Number(value);
  if (Number.isFinite(v) && v >= 0) {
    el.value = String(v);
    try {
      localStorage.setItem(BLOCKS_DAY_FILTER_KEY, String(v));
    } catch {
      // ignore
    }
  }
}

function getBlocksDayFilterFromStorage() {
  try {
    const stored = localStorage.getItem(BLOCKS_DAY_FILTER_KEY);
    if (!stored) return 0;
    const value = Number(stored);
    return Number.isFinite(value) && value >= 0 ? value : 0;
  } catch {
    return 0;
  }
}

function filterBlocksByDays(blocks, days) {
  if (!Array.isArray(blocks) || days <= 0) return blocks;
  const now = Math.floor(Date.now() / 1000);
  const cutoffSeconds = days * 24 * 60 * 60;
  const cutoffTime = now - cutoffSeconds;
  return blocks.filter(b => {
    const ts = Number(b?.timestamp);
    if (!Number.isFinite(ts) || ts <= 0) return false;
    return ts >= cutoffTime;
  });
}

function displayWorkerName(worker) {
  const w = String(worker ?? '').trim();
  if (w === 'InternalCPU') return 'RKStratum CPU Miner';
  return w || '-';
}

function escapeHtmlAttr(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function parseNonceToBigInt(nonce) {
  const s = String(nonce ?? '').trim();
  if (!s) return null;

  try {
    if (s.startsWith('0x') || s.startsWith('0X')) return BigInt(s);
  } catch {
    // fall through
  }

  if (/^[0-9]+$/.test(s)) {
    try {
      return BigInt(s);
    } catch {
      return null;
    }
  }

  if (/^[0-9a-fA-F]+$/.test(s)) {
    try {
      return BigInt('0x' + s);
    } catch {
      return null;
    }
  }

  return null;
}

function formatNonceInfo(nonce) {
  const bi = parseNonceToBigInt(nonce);
  if (!bi) {
    const raw = String(nonce ?? '');
    return { display: raw || '-', title: raw || '-' };
  }
  const dec = bi.toString(10);
  const hex = bi.toString(16);
  return {
    display: `0x${hex}`,
    title: `dec: ${dec}\nhex: 0x${hex}`,
  };
}

function escapeCsvCell(value) {
  const s = value == null ? '' : String(value);
  if (/[",\n\r]/.test(s)) return `"${s.replace(/"/g, '""')}"`;
  return s;
}

function downloadCsv(filename, rows) {
  const csv = rows.map(r => r.map(escapeCsvCell).join(',')).join('\n');
  const blob = new Blob([csv], { type: 'text/csv;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

async function copyToClipboard(text) {
  const value = String(text ?? '');
  if (!value) return false;
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(value);
      return true;
    }
  } catch {
    // fall back below
  }

  try {
    const ta = document.createElement('textarea');
    ta.value = value;
    ta.setAttribute('readonly', '');
    ta.style.position = 'fixed';
    ta.style.left = '-9999px';
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand('copy');
    ta.remove();
    return ok;
  } catch {
    return false;
  }
}

function showToast(message) {
  const el = document.getElementById('toast');
  if (!el) return;
  el.textContent = message;
  el.classList.remove('hidden');
  clearTimeout(showToast._t);
  showToast._t = setTimeout(() => el.classList.add('hidden'), 1600);
}

function isCoarsePointerDevice() {
  try {
    return window.matchMedia && window.matchMedia('(pointer: coarse)').matches;
  } catch {
    return false;
  }
}

function openRowDetailModal(title, rows) {
  const modal = document.getElementById('rowDetailModal');
  const body = document.getElementById('rowDetailBody');
  const titleEl = document.getElementById('rowDetailTitle');
  if (!modal || !body || !titleEl) return;

  titleEl.textContent = title || 'Details';
  body.innerHTML = (rows || []).map(({ label, value, copyValue }) => {
    const v = value == null || value === '' ? '-' : String(value);
    const copy = copyValue != null && String(copyValue) !== ''
      ? `<button type="button" class="bg-surface-2 border border-card px-3 py-2 rounded-lg text-sm font-medium text-white hover:border-kaspa-primary shrink-0" data-copy-text="${escapeHtmlAttr(copyValue)}">Copy</button>`
      : '';
    return `
      <div class="bg-surface-2 border border-card rounded-xl px-4 py-3">
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <div class="text-xs text-gray-400">${escapeHtmlAttr(label)}</div>
            <div class="text-sm text-white break-all">${escapeHtmlAttr(v)}</div>
          </div>
          ${copy}
        </div>
      </div>
    `;
  }).join('');

  modal.classList.remove('hidden');
  try { document.body.style.overflow = 'hidden'; } catch {}
}

function closeRowDetailModal() {
  const modal = document.getElementById('rowDetailModal');
  if (!modal) return;
  modal.classList.add('hidden');
  try { document.body.style.overflow = ''; } catch {}
}

function cacheReadJson(key) {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

function cacheWriteJson(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // ignore quota / disabled storage
  }
}

function readCachedSnapshot() {
  const status = cacheReadJson(CACHE_KEYS.status);
  const stats = cacheReadJson(CACHE_KEYS.stats);
  const updatedMs = Number(localStorage.getItem(CACHE_KEYS.updatedMs) || 0);
  if (!status || !stats) return null;
  return { status, stats, updatedMs };
}

function mergeBlockHistory(incomingBlocks, existingBlocks) {
  const byHash = new Map();
  for (const b of (existingBlocks || [])) {
    const h = b && b.hash;
    if (h) byHash.set(h, b);
  }
  for (const b of (incomingBlocks || [])) {
    const h = b && b.hash;
    if (h) byHash.set(h, b);
  }
  const merged = Array.from(byHash.values());
  merged.sort((a, b) => (Number(b.bluescore) || 0) - (Number(a.bluescore) || 0));
  return merged;
}

function cacheUpdate(status, stats) {
  const existing = cacheReadJson(CACHE_KEYS.stats);
  const mergedBlocks = mergeBlockHistory(stats?.blocks, existing?.blocks);
  const prevTotalBlocks = Number(existing?.totalBlocks);
  const incomingTotalBlocks = stats?.totalBlocks ?? stats?.total_blocks ?? stats?.totalblocks;
  const nextTotalBlocks = Number(incomingTotalBlocks);
  const mergedCount = Array.isArray(mergedBlocks) ? mergedBlocks.length : 0;
  const totalBlocksCandidates = [];
  if (Number.isFinite(prevTotalBlocks)) totalBlocksCandidates.push(prevTotalBlocks);
  if (Number.isFinite(nextTotalBlocks)) totalBlocksCandidates.push(nextTotalBlocks);
  if (Number.isFinite(mergedCount) && mergedCount > 0) totalBlocksCandidates.push(mergedCount);
  const totalBlocks = totalBlocksCandidates.length
    ? Math.max(...totalBlocksCandidates)
    : (incomingTotalBlocks ?? existing?.totalBlocks ?? mergedCount);

  // Render with full block history, but keep localStorage bounded to avoid quota issues.
  const CACHE_BLOCKS_MAX = 500;
  const statsToRender = { ...(stats || {}), totalBlocks, blocks: mergedBlocks };
  const statsToStore = { ...(stats || {}), totalBlocks, blocks: mergedBlocks.slice(0, CACHE_BLOCKS_MAX) };
  cacheWriteJson(CACHE_KEYS.status, status);
  cacheWriteJson(CACHE_KEYS.stats, statsToStore);
  try { localStorage.setItem(CACHE_KEYS.updatedMs, String(Date.now())); } catch {}
  return statsToRender;
}

function cacheClear() {
  try {
    localStorage.removeItem(CACHE_KEYS.status);
    localStorage.removeItem(CACHE_KEYS.stats);
    localStorage.removeItem(CACHE_KEYS.updatedMs);
  } catch {
    // ignore
  }
}

function setLastUpdated(updatedMs, isCached) {
  const el = document.getElementById('lastUpdated');
  if (!el) return;
  if (!updatedMs || !Number.isFinite(updatedMs) || updatedMs <= 0) {
    el.textContent = '-';
    return;
  }
  const s = new Date(updatedMs).toLocaleString();
  el.textContent = isCached ? `${s} (cached)` : s;
}

function displayTotalBlocksFromStats(stats) {
  const n = Number(stats?.totalBlocks ?? stats?.total_blocks ?? stats?.totalblocks);
  const blocksCount = Array.isArray(stats?.blocks) ? stats.blocks.length : 0;
  const candidates = [];
  if (Number.isFinite(n)) candidates.push(n);
  if (Number.isFinite(blocksCount) && blocksCount > 0) candidates.push(blocksCount);
  if (!candidates.length) return stats?.totalBlocks ?? stats?.total_blocks ?? stats?.totalblocks ?? blocksCount;
  return Math.max(...candidates);
}

function pickColors(n) {
  const base = ['#22c55e','#0ea5e9','#a855f7','#f59e0b','#ef4444','#14b8a6','#e11d48','#84cc16'];
  const out = [];
  for (let i = 0; i < n; i++) out.push(base[i % base.length]);
  return out;
}

function renderDonutChart(containerId, legendId, items, emptyMessage) {
  const container = document.getElementById(containerId);
  const legend = document.getElementById(legendId);
  if (!container || !legend) return;

  const total = items.reduce((a, b) => a + (Number(b.value) || 0), 0);
  if (!Number.isFinite(total) || total <= 0) {
    const msg = emptyMessage || 'No blocks mined data to chart yet.';
    container.innerHTML = `<div class="text-sm text-gray-400">${msg}</div>`;
    legend.innerHTML = '';
    return;
  }

  const radius = 15.9155;
  let offset = 25;

  const colors = pickColors(items.length);
  const segments = items.map((it, idx) => {
    const v = Number(it.value) || 0;
    const pct = (v / total) * 100;
    const dash = `${pct} ${100 - pct}`;
    const seg = `
      <circle
        r="${radius}"
        cx="21"
        cy="21"
        fill="transparent"
        stroke="${colors[idx]}"
        stroke-width="6"
        pathLength="100"
        stroke-dasharray="${dash}"
        stroke-dashoffset="${offset}"
        stroke-linecap="butt"
      />
    `;
    offset -= pct;
    return seg;
  }).join('');

  container.innerHTML = `
    <svg viewBox="0 0 42 42" width="256" height="256" style="width: 256px; height: 256px;" class="w-64 h-64">
      <circle r="${radius}" cx="21" cy="21" fill="transparent" stroke="#1f2937" stroke-width="6" pathLength="100" stroke-dasharray="100 0" />
      ${segments}
      <circle r="10" cx="21" cy="21" fill="#0b1220" />
      <text x="21" y="21" text-anchor="middle" dominant-baseline="central" fill="#ffffff" font-size="4" font-weight="600">${total}</text>
      <text x="21" y="26" text-anchor="middle" dominant-baseline="central" fill="#9ca3af" font-size="2.8">blocks</text>
    </svg>
  `;

  legend.innerHTML = items.map((it, idx) => {
    const v = Number(it.value) || 0;
    const pct = ((v / total) * 100).toFixed(1);
    return `
      <div class="flex items-center justify-between gap-3">
        <div class="flex items-center gap-3 min-w-0">
          <span class="inline-block w-3 h-3 rounded" style="background:${colors[idx]}"></span>
          <span class="truncate" title="${it.label}">${it.label}</span>
        </div>
        <div class="text-gray-300">${v} <span class="text-gray-500">(${pct}%)</span></div>
      </div>
    `;
  }).join('');
}

function getWalletFilter() {
  return normalizeWalletFilter(document.getElementById('walletFilter')?.value);
}

function setWalletFilter(value) {
  const v = normalizeWalletFilter(value);
  const el = document.getElementById('walletFilter');
  if (el) el.value = v;
  try {
    if (v) localStorage.setItem(WALLET_FILTER_KEY, v);
    else localStorage.removeItem(WALLET_FILTER_KEY);
  } catch {
    // ignore
  }
}

function getWalletFilterFromStorage() {
  try {
    return normalizeWalletFilter(localStorage.getItem(WALLET_FILTER_KEY));
  } catch {
    return '';
  }
}

function renderWalletSummary(stats, filter) {
  const el = document.getElementById('walletSummary');
  if (!el) return;
  const f = normalizeWalletFilter(filter);
  if (!f) {
    el.classList.add('hidden');
    el.innerHTML = '';
    return;
  }

  const workersAll = Array.isArray(stats?.workers) ? stats.workers : [];
  const blocksAll = Array.isArray(stats?.blocks) ? stats.blocks : [];
  const workers = workersAll.filter(w => (w.wallet || '').includes(f));
  const blocks = blocksAll.filter(b => (b.wallet || '').includes(f));

  const activeWorkers = workers.length;
  const totalShares = workers.reduce((a, w) => a + (Number(w.shares) || 0), 0);
  const totalInvalid = workers.reduce((a, w) => a + (Number(w.invalid) || 0), 0);
  const totalStale = workers.reduce((a, w) => a + (Number(w.stale) || 0), 0);
  const totalHashrateHs = workers.reduce((a, w) => a + ((Number(w.hashrate) || 0) * 1e9), 0);

  el.classList.remove('hidden');
  el.innerHTML = `
    <div class="flex items-start justify-between gap-4">
      <div class="min-w-0">
        <div class="text-xs text-gray-400">Wallet</div>
        <div class="text-sm text-white break-all">${escapeHtmlAttr(f)}</div>
      </div>
      <div class="shrink-0">
        <button type="button" class="bg-surface-1 border border-card px-3 py-1.5 rounded-lg text-xs font-medium text-white hover:border-kaspa-primary" data-copy-text="${escapeHtmlAttr(f)}">Copy</button>
      </div>
    </div>
    <div class="mt-3 grid grid-cols-2 lg:grid-cols-4 gap-3 text-sm">
      <div class="bg-surface-1 border border-card rounded-lg px-3 py-2">
        <div class="text-xs text-gray-400">Workers</div>
        <div class="text-white font-semibold tabular-nums">${activeWorkers}</div>
      </div>
      <div class="bg-surface-1 border border-card rounded-lg px-3 py-2">
        <div class="text-xs text-gray-400">Blocks</div>
        <div class="text-white font-semibold tabular-nums">${blocks.length}</div>
      </div>
      <div class="bg-surface-1 border border-card rounded-lg px-3 py-2">
        <div class="text-xs text-gray-400">Hashrate</div>
        <div class="text-white font-semibold tabular-nums">${formatHashrateHs(totalHashrateHs)}</div>
      </div>
      <div class="bg-surface-1 border border-card rounded-lg px-3 py-2">
        <div class="text-xs text-gray-400">Shares (S/I)</div>
        <div class="text-white font-semibold tabular-nums">${totalShares} <span class="text-gray-400">(${totalStale}/${totalInvalid})</span></div>
      </div>
    </div>
  `;
}

const COLLAPSE_KEY = 'ks_bridge_collapsed_sections_v1';

function readCollapsedSections() {
  try {
    return JSON.parse(localStorage.getItem(COLLAPSE_KEY) || '{}') || {};
  } catch {
    return {};
  }
}

function writeCollapsedSections(map) {
  try {
    localStorage.setItem(COLLAPSE_KEY, JSON.stringify(map || {}));
  } catch {
    // ignore
  }
}

function setSectionCollapsed(id, collapsed) {
  const body = document.querySelector(`[data-collapsible-body="${id}"]`);
  const icon = document.querySelector(`[data-collapsible-icon="${id}"]`);
  const label = document.querySelector(`[data-collapsible-label="${id}"]`);
  const toggle = document.querySelector(`[data-collapsible-toggle="${id}"]`);
  if (!body) return;

  body.classList.toggle('hidden', !!collapsed);
  if (toggle) toggle.setAttribute('aria-expanded', collapsed ? 'false' : 'true');

  if (icon) {
    icon.classList.toggle('rotate-180', !!collapsed);
  }
  if (label) {
    label.textContent = collapsed ? 'Expand' : 'Collapse';
  }
}

function initCollapsibles() {
  const saved = readCollapsedSections();
  const ids = new Set();
  for (const el of document.querySelectorAll('[data-collapsible-body]')) {
    ids.add(el.getAttribute('data-collapsible-body'));
  }
  for (const id of ids) {
    const defaultCollapsed = false;
    const collapsed = id === 'raw' ? true : (saved[id] != null ? !!saved[id] : defaultCollapsed);
    setSectionCollapsed(id, collapsed);
  }
}

function updateBlocksChartFromBlocks(blocks, totalAllBlocks, walletFilter) {
  const blockBuckets = new Map();
  for (const b of (blocks || [])) {
    const label = `${b.instance || '-'} / ${displayWorkerName(b.worker)}`;
    blockBuckets.set(label, (blockBuckets.get(label) || 0) + 1);
  }

  const items = Array.from(blockBuckets.entries())
    .map(([label, value]) => ({ label, value }))
    .sort((a, b) => (b.value || 0) - (a.value || 0));

  const top = items.slice(0, 7);
  const rest = items.slice(7);
  const restTotal = rest.reduce((a, b) => a + (Number(b.value) || 0), 0);
  if (restTotal > 0) top.push({ label: 'Other', value: restTotal });

  const filter = normalizeWalletFilter(walletFilter);
  const allCount = Number(totalAllBlocks) || 0;
  const emptyMessage = filter && allCount > 0
    ? 'No blocks match the current wallet filter. Clear the filter to see all blocks.'
    : 'No blocks mined data to chart yet.';

  renderDonutChart('blocksPie', 'blocksPieLegend', top, emptyMessage);
}

async function refresh() {
  const loader = document.getElementById('status-loader');
  const statusText = document.getElementById('status-text');
  const setDot = (state, title) => {
    if (!statusText) return;
    statusText.className = `status-dot status-dot--${state}`;
    statusText.title = title;
    statusText.textContent = '';
  };

  loader.style.display = 'inline-block';
  setDot('loading', 'Loading');

  try {
    const [statusRes, statsRes] = await Promise.all([
      fetch('api/status', { cache: 'no-store' }),
      fetch('api/stats', { cache: 'no-store' }),
    ]);

    if (!statusRes.ok) throw new Error(`status HTTP ${statusRes.status}`);
    if (!statsRes.ok) throw new Error(`stats HTTP ${statsRes.status}`);

    const status = await statusRes.json();
    const stats = await statsRes.json();

    const mergedStats = cacheUpdate(status, stats);

    loader.style.display = 'none';
    setDot('online', 'Online');

    document.getElementById('kaspad').textContent = status.kaspad_address;
    document.getElementById('kaspadVersion').textContent = status.kaspad_version ?? '-';
    document.getElementById('instances').textContent = status.instances;
    document.getElementById('web').textContent = status.web_bind;
    setLastUpdated(Date.now(), false);

    document.getElementById('totalBlocks').textContent = mergedStats.totalBlocks;
    document.getElementById('totalShares').textContent = mergedStats.totalShares;
    document.getElementById('activeWorkers').textContent = mergedStats.activeWorkers;
    document.getElementById('networkHashrate').textContent = formatHashrateHs(mergedStats.networkHashrate);
    document.getElementById('networkDifficulty').textContent = formatDifficulty(mergedStats.networkDifficulty);
    document.getElementById('networkBlockCount').textContent = mergedStats.networkBlockCount ?? '-';

    const icpu = mergedStats.internalCpu;
    if (icpu && typeof icpu === 'object') {
      setInternalCpuCardsVisible(true);
      setText('internalCpuHashrate', formatHashrateHs((Number(icpu.hashrateGhs) || 0) * 1e9));
      const accepted = Number(icpu.blocksAccepted) || 0;
      const submitted = Number(icpu.blocksSubmitted) || 0;
      setText('internalCpuBlocks', `${accepted} (${submitted} submitted)`);
    } else {
      setInternalCpuCardsVisible(false);
      setText('internalCpuHashrate', '-');
      setText('internalCpuBlocks', '-');
    }

    const filter = getWalletFilter();
    const dayFilter = getBlocksDayFilter();

    renderWalletSummary(mergedStats, filter);

    let blocks = (mergedStats.blocks || []).filter(b => !filter || (b.wallet || '').includes(filter));
    blocks = filterBlocksByDays(blocks, dayFilter);
    lastFilteredBlocks = blocks;
    const blocksBody = document.getElementById('blocksBody');
    blocksBody.innerHTML = '';
    blocks.forEach((b, idx) => {
      const nonceInfo = formatNonceInfo(b.nonce);
      const hashFull = b.hash || '';
      const hashShort = shortHash(hashFull);
      const workerDisplay = displayWorkerName(b.worker);
      const tr = document.createElement('tr');
      tr.className = 'border-b border-card/50 cursor-pointer';
      tr.setAttribute('data-row-kind', 'block');
      tr.setAttribute('data-row-index', String(idx));
      tr.innerHTML = `
        <td class="py-1.5 pr-3" title="${b.timestamp || ''}">${formatUnixSeconds(b.timestamp)}</td>
        <td class="py-1.5 pr-3" title="${escapeHtmlAttr(b.instance || '')}">${b.instance || '-'}</td>
        <td class="py-1.5 pr-3" title="${escapeHtmlAttr(b.bluescore || '')}">${b.bluescore || '-'}</td>
        <td class="py-1.5 pr-3" title="${escapeHtmlAttr(workerDisplay)}">${workerDisplay}</td>
        <td class="py-1.5 pr-3">
          <div class="flex items-center gap-2 min-w-0">
            <span class="min-w-0 truncate" title="${b.wallet || ''}">${b.wallet || '-'}</span>
            ${b.wallet ? `<button type="button" class="bg-surface-1 border border-card px-2 py-0.5 rounded text-xs hover:border-kaspa-primary shrink-0" data-copy-text="${escapeHtmlAttr(b.wallet)}">Copy</button>` : ''}
          </div>
        </td>
        <td class="py-1.5 pr-3 font-mono" title="${escapeHtmlAttr(nonceInfo.title)}">${nonceInfo.display || '-'}</td>
        <td class="py-1.5 pr-3">
          <div class="flex items-center gap-2 min-w-0">
            <span class="font-mono min-w-0 truncate" title="${hashFull}">${hashShort}</span>
            ${hashFull ? `<button type="button" class="bg-surface-1 border border-card px-2 py-0.5 rounded text-xs hover:border-kaspa-primary shrink-0" data-copy-text="${escapeHtmlAttr(hashFull)}">Copy</button>` : ''}
          </div>
        </td>
      `;
      blocksBody.appendChild(tr);
    });

    const allWorkers = mergedStats.workers || [];
    const existingOrder = readWorkerOrder();
    const orderedWorkers = maintainWorkerOrder(existingOrder, allWorkers);
    const workers = orderedWorkers.filter(w => !filter || (w.wallet || '').includes(filter));
    lastFilteredWorkers = workers;
    const workersBody = document.getElementById('workersBody');
    workersBody.innerHTML = '';
    lastInternalCpuWorker = null;

    // Render internal CPU miner row as a pseudo-worker (not affected by wallet filter).
    if (!filter && icpu && typeof icpu === 'object') {
      const tr = document.createElement('tr');
      tr.className = 'border-b border-card/50 cursor-pointer';
      const hashrateHs = (Number(icpu.hashrateGhs) || 0) * 1e9;
      const wallet = String(icpu.wallet ?? '').trim();
      const shares = Number(icpu.shares ?? icpu.blocksAccepted) || 0;
      const stale = Number(icpu.stale ?? ((Number(icpu.blocksSubmitted) || 0) - (Number(icpu.blocksAccepted) || 0))) || 0;
      const invalid = Number(icpu.invalid ?? 0) || 0;
      lastInternalCpuWorker = { wallet, hashrateHs, shares, stale, invalid, blocks: Number(icpu.blocksAccepted) || 0 };
      tr.setAttribute('data-row-kind', 'icpu');
      tr.setAttribute('data-row-index', '-1');
      tr.innerHTML = `
        <td class="py-1.5 pr-3">-</td>
        <td class="py-1.5 pr-3">${displayWorkerName('InternalCPU')}</td>
        <td class="py-1.5 pr-3">
          <div class="flex items-center gap-2 min-w-0">
            <span class="min-w-0 truncate" title="${escapeHtmlAttr(wallet)}">${wallet || '-'}</span>
            ${wallet ? `<button type="button" class="bg-surface-1 border border-card px-2 py-0.5 rounded text-xs hover:border-kaspa-primary shrink-0" data-copy-text="${escapeHtmlAttr(wallet)}">Copy</button>` : ''}
          </div>
        </td>
        <td class="py-1.5 pr-3">${formatHashrateHs(hashrateHs)}</td>
        <td class="py-1.5 pr-3">${shares}</td>
        <td class="py-1.5 pr-3">${stale}</td>
        <td class="py-1.5 pr-3">${invalid}</td>
        <td class="py-1.5 pr-3">${Number(icpu.blocksAccepted) || 0}</td>
      `;
      workersBody.appendChild(tr);
    }

    workers.forEach((w, idx) => {
      const tr = document.createElement('tr');
      tr.className = 'border-b border-card/50 cursor-pointer';
      tr.setAttribute('data-row-kind', 'worker');
      tr.setAttribute('data-row-index', String(idx));
      tr.innerHTML = `
        <td class="py-1.5 pr-3" title="${escapeHtmlAttr(w.instance || '')}">${w.instance || '-'}</td>
        <td class="py-1.5 pr-3" title="${escapeHtmlAttr(displayWorkerName(w.worker))}">${displayWorkerName(w.worker)}</td>
        <td class="py-1.5 pr-3">
          <div class="flex items-center gap-2 min-w-0">
            <span class="min-w-0 truncate" title="${w.wallet || ''}">${w.wallet || '-'}</span>
            ${w.wallet ? `<button type="button" class="bg-surface-1 border border-card px-2 py-0.5 rounded text-xs hover:border-kaspa-primary shrink-0" data-copy-text="${String(w.wallet).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/\"/g,'&quot;').replace(/'/g,'&#39;')}">Copy</button>` : ''}
          </div>
        </td>
        <td class="py-1.5 pr-3">${formatHashrateHs((w.hashrate || 0) * 1e9)}</td>
        <td class="py-1.5 pr-3">${w.shares ?? '-'}</td>
        <td class="py-1.5 pr-3">${w.stale ?? '-'}</td>
        <td class="py-1.5 pr-3">${w.invalid ?? '-'}</td>
        <td class="py-1.5 pr-3">${w.blocks ?? '-'}</td>
      `;
      workersBody.appendChild(tr);
    });

    updateBlocksChartFromBlocks(blocks, (mergedStats.blocks || []).length, filter);

    // raw view is on /raw.html
  } catch (e) {
    loader.style.display = 'none';
    setDot('offline', 'Offline');
    const cached = readCachedSnapshot();
    if (cached) {
      document.getElementById('kaspad').textContent = cached.status.kaspad_address ?? '-';
      document.getElementById('kaspadVersion').textContent = cached.status.kaspad_version ?? '-';
      document.getElementById('instances').textContent = cached.status.instances ?? '-';
      document.getElementById('web').textContent = cached.status.web_bind ?? '-';
      setLastUpdated(cached.updatedMs, true);

      document.getElementById('totalBlocks').textContent = displayTotalBlocksFromStats(cached.stats);
      document.getElementById('totalShares').textContent = cached.stats.totalShares;
      document.getElementById('activeWorkers').textContent = cached.stats.activeWorkers;
      document.getElementById('networkHashrate').textContent = formatHashrateHs(cached.stats.networkHashrate);
      document.getElementById('networkDifficulty').textContent = formatDifficulty(cached.stats.networkDifficulty);
      document.getElementById('networkBlockCount').textContent = cached.stats.networkBlockCount ?? '-';

      const icpu = cached.stats.internalCpu;
      if (icpu && typeof icpu === 'object') {
        setInternalCpuCardsVisible(true);
        setText('internalCpuHashrate', formatHashrateHs((Number(icpu.hashrateGhs) || 0) * 1e9));
        const accepted = Number(icpu.blocksAccepted) || 0;
        const submitted = Number(icpu.blocksSubmitted) || 0;
        setText('internalCpuBlocks', `${accepted} (${submitted} submitted)`);
      } else {
        setInternalCpuCardsVisible(false);
        setText('internalCpuHashrate', '-');
        setText('internalCpuBlocks', '-');
      }

      const filter = getWalletFilter();
      const dayFilter = getBlocksDayFilter();

      renderWalletSummary(cached.stats, filter);

      let blocks = (cached.stats.blocks || []).filter(b => !filter || (b.wallet || '').includes(filter));
      blocks = filterBlocksByDays(blocks, dayFilter);
      lastFilteredBlocks = blocks;
      const blocksBody = document.getElementById('blocksBody');
      blocksBody.innerHTML = '';
      blocks.forEach((b, idx) => {
        const nonceInfo = formatNonceInfo(b.nonce);
        const hashFull = b.hash || '';
        const hashShort = shortHash(hashFull);
      const workerDisplay = displayWorkerName(b.worker);
        const tr = document.createElement('tr');
        tr.className = 'border-b border-card/50 cursor-pointer';
        tr.setAttribute('data-row-kind', 'block');
        tr.setAttribute('data-row-index', String(idx));
        tr.innerHTML = `
          <td class="py-1.5 pr-3" title="${b.timestamp || ''}">${formatUnixSeconds(b.timestamp)}</td>
          <td class="py-1.5 pr-3" title="${escapeHtmlAttr(b.instance || '')}">${b.instance || '-'}</td>
          <td class="py-1.5 pr-3" title="${escapeHtmlAttr(b.bluescore || '')}">${b.bluescore || '-'}</td>
        <td class="py-1.5 pr-3" title="${escapeHtmlAttr(workerDisplay)}">${workerDisplay}</td>
          <td class="py-1.5 pr-3">
            <div class="flex items-center gap-2 min-w-0">
              <span class="min-w-0 truncate" title="${b.wallet || ''}">${b.wallet || '-'}</span>
              ${b.wallet ? `<button type="button" class="bg-surface-1 border border-card px-2 py-0.5 rounded text-xs hover:border-kaspa-primary shrink-0" data-copy-text="${escapeHtmlAttr(b.wallet)}">Copy</button>` : ''}
            </div>
          </td>
          <td class="py-1.5 pr-3 font-mono" title="${escapeHtmlAttr(nonceInfo.title)}">${nonceInfo.display || '-'}</td>
          <td class="py-1.5 pr-3">
            <div class="flex items-center gap-2 min-w-0">
              <span class="font-mono min-w-0 truncate" title="${hashFull}">${hashShort}</span>
              ${hashFull ? `<button type="button" class="bg-surface-1 border border-card px-2 py-0.5 rounded text-xs hover:border-kaspa-primary shrink-0" data-copy-text="${escapeHtmlAttr(hashFull)}">Copy</button>` : ''}
            </div>
          </td>
        `;
        blocksBody.appendChild(tr);
      });

      const allWorkers = cached.stats.workers || [];
      const existingOrder = readWorkerOrder();
      const orderedWorkers = maintainWorkerOrder(existingOrder, allWorkers);
      const workers = orderedWorkers.filter(w => !filter || (w.wallet || '').includes(filter));
      lastFilteredWorkers = workers;
      const workersBody = document.getElementById('workersBody');
      workersBody.innerHTML = '';
      lastInternalCpuWorker = null;

      // Render internal CPU miner row as a pseudo-worker (not affected by wallet filter).
      if (!filter && icpu && typeof icpu === 'object') {
        const tr = document.createElement('tr');
        tr.className = 'border-b border-card/50 cursor-pointer';
        const hashrateHs = (Number(icpu.hashrateGhs) || 0) * 1e9;
        const wallet = String(icpu.wallet ?? '').trim();
        const shares = Number(icpu.shares ?? icpu.blocksAccepted) || 0;
        const stale = Number(icpu.stale ?? ((Number(icpu.blocksSubmitted) || 0) - (Number(icpu.blocksAccepted) || 0))) || 0;
        const invalid = Number(icpu.invalid ?? 0) || 0;
        lastInternalCpuWorker = { wallet, hashrateHs, shares, stale, invalid, blocks: Number(icpu.blocksAccepted) || 0 };
        tr.setAttribute('data-row-kind', 'icpu');
        tr.setAttribute('data-row-index', '-1');
        tr.innerHTML = `
          <td class="py-1.5 pr-3">-</td>
          <td class="py-1.5 pr-3">${displayWorkerName('InternalCPU')}</td>
          <td class="py-1.5 pr-3">
            <div class="flex items-center gap-2 min-w-0">
              <span class="min-w-0 truncate" title="${escapeHtmlAttr(wallet)}">${wallet || '-'}</span>
              ${wallet ? `<button type="button" class="bg-surface-1 border border-card px-2 py-0.5 rounded text-xs hover:border-kaspa-primary shrink-0" data-copy-text="${escapeHtmlAttr(wallet)}">Copy</button>` : ''}
            </div>
          </td>
          <td class="py-1.5 pr-3">${formatHashrateHs(hashrateHs)}</td>
          <td class="py-1.5 pr-3">${shares}</td>
          <td class="py-1.5 pr-3">${stale}</td>
          <td class="py-1.5 pr-3">${invalid}</td>
          <td class="py-1.5 pr-3">${Number(icpu.blocksAccepted) || 0}</td>
        `;
        workersBody.appendChild(tr);
      }

      workers.forEach((w, idx) => {
        const tr = document.createElement('tr');
        tr.className = 'border-b border-card/50 cursor-pointer';
        tr.setAttribute('data-row-kind', 'worker');
        tr.setAttribute('data-row-index', String(idx));
        tr.innerHTML = `
          <td class="py-1.5 pr-3" title="${escapeHtmlAttr(w.instance || '')}">${w.instance || '-'}</td>
          <td class="py-1.5 pr-3" title="${escapeHtmlAttr(displayWorkerName(w.worker))}">${displayWorkerName(w.worker)}</td>
          <td class="py-1.5 pr-3">
            <div class="flex items-center gap-2 min-w-0">
              <span class="min-w-0 truncate" title="${w.wallet || ''}">${w.wallet || '-'}</span>
              ${w.wallet ? `<button type="button" class="bg-surface-1 border border-card px-2 py-0.5 rounded text-xs hover:border-kaspa-primary shrink-0" data-copy-text="${escapeHtmlAttr(w.wallet)}">Copy</button>` : ''}
            </div>
          </td>
          <td class="py-1.5 pr-3">${formatHashrateHs((w.hashrate || 0) * 1e9)}</td>
          <td class="py-1.5 pr-3">${w.shares ?? '-'}</td>
          <td class="py-1.5 pr-3">${w.stale ?? '-'}</td>
          <td class="py-1.5 pr-3">${w.invalid ?? '-'}</td>
          <td class="py-1.5 pr-3">${w.blocks ?? '-'}</td>
        `;
        workersBody.appendChild(tr);
      });

      updateBlocksChartFromBlocks(blocks, (cached.stats.blocks || []).length, filter);

      return;
    }

    setLastUpdated(0, false);
  } finally {
    loader.style.display = 'none';
  }
}

document.addEventListener('click', async (e) => {
  const collapseBtn = e.target.closest('[data-collapsible-toggle]');
  if (collapseBtn) {
    const id = collapseBtn.getAttribute('data-collapsible-toggle');
    if (!id) return;
    const isRaw = id === 'raw';
    const collapsedNow = collapseBtn.getAttribute('aria-expanded') === 'false';
    const collapsedNext = !collapsedNow;
    setSectionCollapsed(id, collapsedNext);
    if (!isRaw) {
      const saved = readCollapsedSections();
      saved[id] = collapsedNext;
      writeCollapsedSections(saved);
    }
    return;
  }

  const btn = e.target.closest('[data-copy-id],[data-copy-text]');
  if (btn) {
    let value = '';
    if (btn.dataset.copyText != null) {
      value = btn.dataset.copyText;
    } else if (btn.dataset.copyId) {
      const el = document.getElementById(btn.dataset.copyId);
      value = el ? (el.textContent || '') : '';
    }

    const ok = await copyToClipboard(value);
    showToast(ok ? 'Copied' : 'Copy failed');
    return;
  }

  // Tap-to-expand rows on mobile / coarse pointer devices.
  if (!isCoarsePointerDevice()) return;
  const row = e.target.closest('tr[data-row-kind]');
  if (!row) return;

  const kind = row.getAttribute('data-row-kind');
  const idx = Number(row.getAttribute('data-row-index') || -1);

  if (kind === 'block') {
    const b = lastFilteredBlocks[idx];
    if (!b) return;
    const nonceInfo = formatNonceInfo(b.nonce);
    const workerDisplay = displayWorkerName(b.worker);
    openRowDetailModal('Recent Block', [
      { label: 'Timestamp', value: formatUnixSeconds(b.timestamp), copyValue: b.timestamp },
      { label: 'Instance', value: b.instance || '-', copyValue: b.instance || '' },
      { label: 'Bluescore', value: b.bluescore || '-', copyValue: b.bluescore || '' },
      { label: 'Worker', value: workerDisplay, copyValue: workerDisplay },
      { label: 'Wallet', value: b.wallet || '-', copyValue: b.wallet || '' },
      { label: 'Nonce', value: nonceInfo.title || nonceInfo.display || '-', copyValue: b.nonce || '' },
      { label: 'Hash', value: b.hash || '-', copyValue: b.hash || '' },
    ]);
    return;
  }

  if (kind === 'worker') {
    const w = lastFilteredWorkers[idx];
    if (!w) return;
    const workerDisplay = displayWorkerName(w.worker);
    openRowDetailModal('Worker', [
      { label: 'Instance', value: w.instance || '-', copyValue: w.instance || '' },
      { label: 'Worker', value: workerDisplay, copyValue: workerDisplay },
      { label: 'Wallet', value: w.wallet || '-', copyValue: w.wallet || '' },
      { label: 'Hashrate', value: formatHashrateHs((w.hashrate || 0) * 1e9), copyValue: String((w.hashrate || 0) * 1e9) },
      { label: 'Shares', value: w.shares ?? '-', copyValue: w.shares ?? '' },
      { label: 'Stale', value: w.stale ?? '-', copyValue: w.stale ?? '' },
      { label: 'Invalid', value: w.invalid ?? '-', copyValue: w.invalid ?? '' },
      { label: 'Blocks', value: w.blocks ?? '-', copyValue: w.blocks ?? '' },
    ]);
    return;
  }

  if (kind === 'icpu') {
    const icpu = lastInternalCpuWorker;
    if (!icpu) return;
    openRowDetailModal('RKStratum CPU Miner', [
      { label: 'Worker', value: displayWorkerName('InternalCPU'), copyValue: displayWorkerName('InternalCPU') },
      { label: 'Wallet', value: icpu.wallet || '-', copyValue: icpu.wallet || '' },
      { label: 'Hashrate', value: formatHashrateHs(icpu.hashrateHs || 0), copyValue: String(icpu.hashrateHs || 0) },
      { label: 'Shares', value: icpu.shares ?? '-', copyValue: icpu.shares ?? '' },
      { label: 'Stale', value: icpu.stale ?? '-', copyValue: icpu.stale ?? '' },
      { label: 'Invalid', value: icpu.invalid ?? '-', copyValue: icpu.invalid ?? '' },
      { label: 'Blocks', value: icpu.blocks ?? '-', copyValue: icpu.blocks ?? '' },
    ]);
  }
});

document.getElementById('downloadWorkersCsv').addEventListener('click', () => {
  const rows = [
    ['instance','worker','wallet','hashrate_ghs','shares','stale','invalid','blocks'],
    ...lastFilteredWorkers.map(w => [
      w.instance ?? '',
      w.worker ?? '',
      w.wallet ?? '',
      ((Number(w.hashrate) || 0)).toFixed(6),
      w.shares ?? '',
      w.stale ?? '',
      w.invalid ?? '',
      w.blocks ?? '',
    ]),
  ];
  const ts = new Date().toISOString().replace(/[:.]/g, '-');
  downloadCsv(`workers-${ts}.csv`, rows);
});

document.getElementById('downloadBlocksCsv').addEventListener('click', () => {
  const rows = [
    ['timestamp_unix','timestamp_local','instance','bluescore','worker','wallet','nonce','hash'],
    ...lastFilteredBlocks.map(b => [
      b.timestamp ?? '',
      formatUnixSeconds(b.timestamp),
      b.instance ?? '',
      b.bluescore ?? '',
      b.worker ?? '',
      b.wallet ?? '',
      b.nonce ?? '',
      b.hash ?? '',
    ]),
  ];
  const ts = new Date().toISOString().replace(/[:.]/g, '-');
  downloadCsv(`blocks-${ts}.csv`, rows);
});

document.getElementById('refreshBtn').addEventListener('click', refresh);
(function initWalletSearch() {
  const input = document.getElementById('walletSearchInput');
  const searchBtn = document.getElementById('walletSearchBtn');
  const clearBtn = document.getElementById('walletClearBtn');
  const persisted = getWalletFilterFromStorage();

  if (input) input.value = persisted;
  setWalletFilter(persisted);

  const doSearch = () => {
    const v = normalizeWalletFilter(input?.value);
    setWalletFilter(v);
    refresh();
  };

  const doClear = () => {
    if (input) input.value = '';
    setWalletFilter('');
    refresh();
  };

  if (searchBtn) searchBtn.addEventListener('click', doSearch);
  if (clearBtn) clearBtn.addEventListener('click', doClear);
  if (input) {
    input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') doSearch();
    });
  }
})();

(function initBlocksDayFilter() {
  const select = document.getElementById('blocksDayFilter');
  if (!select) return;
  
  const persisted = getBlocksDayFilterFromStorage();
  setBlocksDayFilter(persisted);
  
  select.addEventListener('change', () => {
    const value = getBlocksDayFilter();
    setBlocksDayFilter(value);
    refresh();
  });
})();
document.getElementById('clearCacheBtn').addEventListener('click', () => {
  cacheClear();
  setLastUpdated(0, false);
  showToast('Cache cleared');
});

(function initRowDetailModalControls() {
  const closeBtn = document.getElementById('rowDetailClose');
  const backdrop = document.getElementById('rowDetailBackdrop');
  if (closeBtn) closeBtn.addEventListener('click', closeRowDetailModal);
  if (backdrop) backdrop.addEventListener('click', closeRowDetailModal);
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') closeRowDetailModal();
  });
})();
// Update server time every second
setInterval(() => {
  updateServerTime();
}, 1000);

// Initial server time update
updateServerTime();

// Update server time format on window resize (for responsive display)
let resizeTimeout;
window.addEventListener('resize', () => {
  clearTimeout(resizeTimeout);
  resizeTimeout = setTimeout(() => {
    updateServerTime();
  }, 100);
});

setInterval(() => {
  // avoid overlapping refresh calls if the network is slow
  if (document.hidden) return;
  refresh();
}, 2000);
// Restore cached data immediately, then refresh live
(function bootstrapFromCache() {
  const cached = readCachedSnapshot();
  if (!cached) return;
  document.getElementById('kaspad').textContent = cached.status.kaspad_address ?? '-';
  document.getElementById('kaspadVersion').textContent = cached.status.kaspad_version ?? '-';
  document.getElementById('instances').textContent = cached.status.instances ?? '-';
  document.getElementById('web').textContent = cached.status.web_bind ?? '-';
  setLastUpdated(cached.updatedMs, true);

  document.getElementById('totalBlocks').textContent = displayTotalBlocksFromStats(cached.stats);
  document.getElementById('totalShares').textContent = cached.stats.totalShares;
  document.getElementById('activeWorkers').textContent = cached.stats.activeWorkers;
  document.getElementById('networkHashrate').textContent = formatHashrateHs(cached.stats.networkHashrate);
  document.getElementById('networkDifficulty').textContent = formatDifficulty(cached.stats.networkDifficulty);
  document.getElementById('networkBlockCount').textContent = cached.stats.networkBlockCount ?? '-';

  const filter = getWalletFilter();
  const dayFilter = getBlocksDayFilter();

  renderWalletSummary(cached.stats, filter);

  let blocks = (cached.stats.blocks || []).filter(b => !filter || (b.wallet || '').includes(filter));
  blocks = filterBlocksByDays(blocks, dayFilter);
  lastFilteredBlocks = blocks;
  const blocksBody = document.getElementById('blocksBody');
  blocksBody.innerHTML = '';
  blocks.forEach((b, idx) => {
    const nonceInfo = formatNonceInfo(b.nonce);
    const hashFull = b.hash || '';
    const hashShort = shortHash(hashFull);
    const workerDisplay = displayWorkerName(b.worker);
    const tr = document.createElement('tr');
    tr.className = 'border-b border-card/50 cursor-pointer';
    tr.setAttribute('data-row-kind', 'block');
    tr.setAttribute('data-row-index', String(idx));
    tr.innerHTML = `
      <td class="py-1.5 pr-3" title="${b.timestamp || ''}">${formatUnixSeconds(b.timestamp)}</td>
      <td class="py-1.5 pr-3" title="${escapeHtmlAttr(b.instance || '')}">${b.instance || '-'}</td>
      <td class="py-1.5 pr-3" title="${escapeHtmlAttr(b.bluescore || '')}">${b.bluescore || '-'}</td>
      <td class="py-1.5 pr-3" title="${escapeHtmlAttr(workerDisplay)}">${workerDisplay}</td>
      <td class="py-1.5 pr-3">
        <div class="flex items-center gap-2 min-w-0">
          <span class="min-w-0 truncate" title="${b.wallet || ''}">${b.wallet || '-'}</span>
          ${b.wallet ? `<button type="button" class="bg-surface-1 border border-card px-2 py-0.5 rounded text-xs hover:border-kaspa-primary shrink-0" data-copy-text="${escapeHtmlAttr(b.wallet)}">Copy</button>` : ''}
        </div>
      </td>
      <td class="py-1.5 pr-3 font-mono" title="${escapeHtmlAttr(nonceInfo.title)}">${nonceInfo.display || '-'}</td>
      <td class="py-1.5 pr-3">
        <div class="flex items-center gap-2 min-w-0">
          <span class="font-mono min-w-0 truncate" title="${hashFull}">${hashShort}</span>
          ${hashFull ? `<button type="button" class="bg-surface-1 border border-card px-2 py-0.5 rounded text-xs hover:border-kaspa-primary shrink-0" data-copy-text="${escapeHtmlAttr(hashFull)}">Copy</button>` : ''}
        </div>
      </td>
    `;
    blocksBody.appendChild(tr);
  });

  const allWorkers = cached.stats.workers || [];
  const existingOrder = readWorkerOrder();
  const orderedWorkers = maintainWorkerOrder(existingOrder, allWorkers);
  const workers = orderedWorkers.filter(w => !filter || (w.wallet || '').includes(filter));
  lastFilteredWorkers = workers;
  const workersBody = document.getElementById('workersBody');
  workersBody.innerHTML = '';
  lastInternalCpuWorker = null;
  workers.forEach((w, idx) => {
    const tr = document.createElement('tr');
    tr.className = 'border-b border-card/50 cursor-pointer';
    tr.setAttribute('data-row-kind', 'worker');
    tr.setAttribute('data-row-index', String(idx));
    const workerDisplay = displayWorkerName(w.worker);
    tr.innerHTML = `
      <td class="py-1.5 pr-3" title="${escapeHtmlAttr(w.instance || '')}">${w.instance || '-'}</td>
      <td class="py-1.5 pr-3" title="${escapeHtmlAttr(workerDisplay)}">${workerDisplay || '-'}</td>
      <td class="py-1.5 pr-3">
        <div class="flex items-center gap-2 min-w-0">
          <span class="min-w-0 truncate" title="${w.wallet || ''}">${w.wallet || '-'}</span>
          ${w.wallet ? `<button type="button" class="bg-surface-1 border border-card px-2 py-0.5 rounded text-xs hover:border-kaspa-primary shrink-0" data-copy-text="${String(w.wallet).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/\"/g,'&quot;').replace(/'/g,'&#39;')}">Copy</button>` : ''}
        </div>
      </td>
      <td class="py-1.5 pr-3">${formatHashrateHs((w.hashrate || 0) * 1e9)}</td>
      <td class="py-1.5 pr-3">${w.shares ?? '-'}</td>
      <td class="py-1.5 pr-3">${w.stale ?? '-'}</td>
      <td class="py-1.5 pr-3">${w.invalid ?? '-'}</td>
      <td class="py-1.5 pr-3">${w.blocks ?? '-'}</td>
    `;
    workersBody.appendChild(tr);
  });

  updateBlocksChartFromBlocks(blocks, (cached.stats.blocks || []).length, filter);
})();
initCollapsibles();
refresh();
