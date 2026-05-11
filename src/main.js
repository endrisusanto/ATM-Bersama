// ─── Tauri API (lazy, safe access) ──────────────────
function tauriAvailable() { return !!(window.__TAURI__); }
function invoke(...args) { return window.__TAURI__.core.invoke(...args); }
function listen(...args) { return window.__TAURI__.event.listen(...args); }
function getWin() { return window.__TAURI__.window.getCurrentWindow(); }
async function openDialog(opts) { return window.__TAURI__.dialog.open(opts); }

// ─── State ──────────────────────────────────────────
let devices = [];
let selectedDevices = new Set();
let availableTests = [];
let selectedTests = new Set();
let testStatuses = {};  // key: "serial|testId"
let logs = [];
let logFilter = "all";
let isRunning = false;
let toolsVersion = "N/A";

// ─── Init ───────────────────────────────────────────
document.addEventListener("DOMContentLoaded", async () => {
  setupEventListeners();

  // Wait for Tauri to be ready
  if (!tauriAvailable()) {
    console.log("Tauri not available yet, waiting...");
    await new Promise(resolve => {
      const check = setInterval(() => {
        if (tauriAvailable()) { clearInterval(check); resolve(); }
      }, 100);
      setTimeout(() => { clearInterval(check); resolve(); }, 5000);
    });
  }

  if (tauriAvailable()) {
    console.log("Tauri API available ✓");
    setupTitlebar();
    await setupTauriListeners();
    await loadAtmPath();
    await refreshDevices();
    await loadTests();
    await fetchToolsVersion();
  } else {
    console.log("Running in browser mode (no Tauri backend)");
    availableTests = [
      { id: "bvt", name: "BVT", jar: "BVT.jar", test_type: "auto", description: "Build Verification Test" },
      { id: "svt", name: "SVT", jar: "SVT.jar", test_type: "auto", description: "Screenshot Verification Tool" },
      { id: "sdt", name: "SDT", jar: "SDT.jar", test_type: "auto", description: "Samsung Device Test" },
      { id: "getprop", name: "Getprop", jar: "Getprop.jar", test_type: "auto", description: "Property Verifier" },
      { id: "cscchecker", name: "CSCChecker", jar: "CSCChecker.jar", test_type: "optional", description: "CSC Checker" },
    ];
    renderTests();
  }

  setTimeout(hideSplashScreen, 800);
});

function hideSplashScreen() {
  const splash = document.getElementById("splash-screen");
  if (splash) {
    splash.classList.add("fade-out");
    setTimeout(() => splash.remove(), 500);
  }
}

// ─── Titlebar ───────────────────────────────────────
function setupTitlebar() {
  const win = getWin();
  document.getElementById("btn-minimize").addEventListener("click", () => win.minimize());
  document.getElementById("btn-maximize").addEventListener("click", async () => {
    (await win.isMaximized()) ? win.unmaximize() : win.maximize();
  });
  document.getElementById("btn-close").addEventListener("click", () => win.close());
}

// ─── Event Listeners ────────────────────────────────
function setupEventListeners() {
  document.getElementById("btn-set-path").addEventListener("click", selectAtmPath);
  document.getElementById("btn-refresh-devices").addEventListener("click", refreshDevices);
  document.getElementById("btn-start").addEventListener("click", startTests);
  document.getElementById("btn-stop").addEventListener("click", stopTests);
  document.getElementById("btn-clear-logs").addEventListener("click", clearLogs);

  // Update Modal
  document.getElementById("btn-open-update").addEventListener("click", openUpdateModal);
  document.getElementById("btn-close-update").addEventListener("click", closeUpdateModal);
  document.querySelector(".modal-overlay").addEventListener("click", closeUpdateModal);

  document.querySelectorAll(".log-filter-btn").forEach(btn => {
    if (btn.id === "btn-clear-logs") return; // Skip clear button
    btn.addEventListener("click", () => {
      document.querySelectorAll(".log-filter-btn").forEach(b => b.classList.remove("active"));
      btn.classList.add("active");
      logFilter = btn.dataset.filter;
      renderLogs();
    });
  });
}

// ─── Tauri Event Listeners ──────────────────────────
async function setupTauriListeners() {
  await listen("test-status", (event) => {
    const s = event.payload;
    const key = `${s.device_serial}|${s.test_id}`;
    testStatuses[key] = s;
    updateTestCard(s);
    updateStats();
  });

  await listen("log-entry", (event) => {
    addLog(event.payload);
  });

  await listen("execution-complete", () => {
    isRunning = false;
    updateActionButtons();
  });
}

// ─── Tools Version ──────────────────────────────────
async function fetchToolsVersion() {
  try {
    toolsVersion = await invoke("get_tools_version");
    document.getElementById("tools-version-text").textContent = toolsVersion;
    document.getElementById("current-ver-modal").textContent = toolsVersion;
  } catch (e) { console.error(e); }
}

// ─── Update Modal ───────────────────────────────────
function openUpdateModal() {
  document.getElementById("update-modal").classList.add("active");
  // Simulate checking
  setTimeout(() => {
    document.getElementById("update-status-text").textContent = "Tools are up to date";
    document.getElementById("update-log-container").innerHTML = `<div>[${now()}] Checked version: ${toolsVersion}</div><div>[${now()}] No updates available at this time.</div>`;
  }, 1500);
}

function closeUpdateModal() {
  document.getElementById("update-modal").classList.remove("active");
}

// ─── ATM Path ───────────────────────────────────────
async function loadAtmPath() {
  try {
    const path = await invoke("get_atm_path");
    if (path) updatePathDisplay(path);
  } catch (e) { console.error(e); }
}

async function selectAtmPath() {
  try {
    const selected = await openDialog({ directory: true, multiple: false, title: "Select ATM Folder (containing ATM_v5.jar)" });
    if (!selected) return;
    const valid = await invoke("set_atm_path", { path: selected });
    if (valid) {
      updatePathDisplay(selected);
      await loadTests();
      await fetchToolsVersion();
    } else {
      addLog({ device_serial: "SYSTEM", test_id: "", timestamp: now(), level: "error", message: "Invalid ATM folder" });
    }
  } catch (e) { console.error(e); }
}

function updatePathDisplay(path) {
  const el = document.getElementById("atm-path-display");
  const text = document.getElementById("atm-path-text");
  const parts = path.split("/");
  const short = parts.length > 2 ? parts.slice(-2).join("/") : path;
  text.textContent = short;
  text.title = path;
  el.classList.add("configured");
}

// ─── Devices ────────────────────────────────────────
async function refreshDevices() {
  const btn = document.getElementById("btn-refresh-devices");
  btn.classList.add("spinning");
  try {
    devices = await invoke("get_devices");
    renderDevices();
  } catch (e) {
    console.error(e);
    addLog({ device_serial: "SYSTEM", test_id: "", timestamp: now(), level: "error", message: `ADB error: ${e}` });
  }
  setTimeout(() => btn.classList.remove("spinning"), 800);
}

function renderDevices() {
  const container = document.getElementById("device-list");
  if (devices.length === 0) {
    container.innerHTML = `
      <div class="empty-state">
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="empty-icon-svg"><path d="M18.36 6.64a9 9 0 1 1-12.73 0"></path><line x1="12" y1="2" x2="12" y2="12"></line></svg>
        <span>No devices detected</span>
      </div>`;
    selectedDevices.clear();
    updateActionButtons();
    return;
  }
  container.innerHTML = devices.map(d => {
    const checked = selectedDevices.has(d.serial);
    const statusClass = d.status === "device" ? "online" : "offline";
    return `<div class="device-card ${checked ? 'selected' : ''}" data-serial="${d.serial}">
      <input type="checkbox" class="device-checkbox" ${checked ? 'checked' : ''} ${d.status !== 'device' ? 'disabled' : ''} />
      <div class="device-info">
        <div class="device-model">${d.model || 'Unknown'}</div>
        <div class="device-serial">${d.serial}</div>
        <div class="device-pda">${d.pda || ''}</div>
      </div>
      <div class="device-meta">
        <span class="device-android">${d.android_version}</span>
        <div class="device-status ${statusClass}"></div>
      </div>
    </div>`;
  }).join("");

  container.querySelectorAll(".device-card").forEach(card => {
    card.addEventListener("click", (e) => {
      if (isRunning) return;
      const serial = card.dataset.serial;
      const dev = devices.find(d => d.serial === serial);
      if (dev && dev.status !== "device") return;
      const cb = card.querySelector(".device-checkbox");
      if (e.target !== cb) cb.checked = !cb.checked;
      if (cb.checked) { selectedDevices.add(serial); card.classList.add("selected"); }
      else { selectedDevices.delete(serial); card.classList.remove("selected"); }
      updateActionButtons();
      updateStats();
    });
  });
  updateStats();
}

// ─── Tests ──────────────────────────────────────────
async function loadTests() {
  try {
    availableTests = await invoke("get_available_tests");
    renderTests();
  } catch (e) { console.error(e); }
}

function renderTests() {
  const container = document.getElementById("test-list");
  container.innerHTML = availableTests.map(t => {
    const checked = selectedTests.has(t.name);
    return `<div class="test-item ${checked ? 'selected' : ''}" data-test="${t.name}" title="${t.description}">
      <input type="checkbox" class="device-checkbox" ${checked ? 'checked' : ''} />
      <span class="test-name">${t.name}</span>
      <span class="test-badge ${t.test_type}">${t.test_type}</span>
    </div>`;
  }).join("");

  container.querySelectorAll(".test-item").forEach(item => {
    item.addEventListener("click", (e) => {
      if (isRunning) return;
      const name = item.dataset.test;
      const cb = item.querySelector("input");
      if (e.target !== cb) cb.checked = !cb.checked;
      if (cb.checked) { selectedTests.add(name); item.classList.add("selected"); }
      else { selectedTests.delete(name); item.classList.remove("selected"); }
      updateActionButtons();
      updateStats();
    });
  });
}

// ─── Dashboard ──────────────────────────────────────
function buildDashboard() {
  const container = document.getElementById("test-dashboard");
  if (selectedDevices.size === 0 || selectedTests.size === 0) {
    container.innerHTML = `
      <div class="empty-state-large">
        <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="empty-icon-large-svg"><path d="M4.5 16.5c-1.5 1.26-2 5-2 5s3.74-.5 5-2c.71-.84.7-2.13-.09-2.91a2.18 2.18 0 0 0-2.91-.09z"></path><path d="m12 15-3-3a22 22 0 0 1 2-3.95A12.88 12.88 0 0 1 22 2c0 2.72-.78 7.5-6 11a22.35 22.35 0 0 1-4 2z"></path><path d="M9 12H4s.55-3.03 2-5c1.62-2.2 5-3 5-3"></path><path d="M12 15v5s3.03-.55 5-2c2.2-1.62 3-5 3-5"></path></svg>
        <h3>Ready to Test</h3>
      </div>`;
    return;
  }
  const testsArr = [...selectedTests];
  container.innerHTML = [...selectedDevices].map(serial => {
    const dev = devices.find(d => d.serial === serial) || { model: serial, android_version: "?", pda: "" };
    return `<div class="device-row" data-device="${serial}">
      <div class="device-row-header">
        <span class="device-row-name">${dev.model || serial}</span>
        <span class="device-row-serial">${serial}</span>
        <span class="device-row-ver">${dev.android_version}</span>
      </div>
      <div class="test-cards">${testsArr.map(t => {
        const key = `${serial}|${t}`;
        const st = testStatuses[key] || { status: "idle", progress: 0, message: "" };
        return renderTestCard(serial, t, st);
      }).join("")}</div>
    </div>`;
  }).join("");
}

function renderTestCard(serial, testId, st) {
  return `<div class="test-card ${st.status}" id="card-${serial}-${testId}" style="--progress: ${st.progress * 100}%">
    <div class="test-card-name">${testId}</div>
    <div class="test-card-status">${st.status.toUpperCase()}</div>
  </div>`;
}

function updateTestCard(st) {
  const id = `card-${st.device_serial}-${st.test_id}`;
  const card = document.getElementById(id);
  if (!card) {
    buildDashboard();
    return;
  }
  card.className = `test-card ${st.status}`;
  card.style.setProperty("--progress", `${st.progress * 100}%`);
  card.querySelector(".test-card-status").textContent = st.status.toUpperCase();
}

// ─── Stats ──────────────────────────────────────────
function updateStats() {
  document.getElementById("stat-devices").textContent = selectedDevices.size;
  document.getElementById("stat-tests").textContent = selectedTests.size;
  let pass = 0, fail = 0, running = 0;
  for (const st of Object.values(testStatuses)) {
    if (st.status === "pass") pass++;
    else if (st.status === "fail" || st.status === "error") fail++;
    else if (st.status === "running") running++;
  }
  document.getElementById("stat-pass").textContent = pass;
  document.getElementById("stat-fail").textContent = fail;
  document.getElementById("stat-running").textContent = running;
}

// ─── Actions ────────────────────────────────────────
function updateActionButtons() {
  const canStart = selectedDevices.size > 0 && selectedTests.size > 0 && !isRunning;
  document.getElementById("btn-start").disabled = !canStart;
  document.getElementById("btn-stop").disabled = !isRunning;
}

async function startTests() {
  if (isRunning) return;
  isRunning = true;
  testStatuses = {};
  updateActionButtons();
  buildDashboard();

  for (const serial of selectedDevices) {
    for (const test of selectedTests) {
      const key = `${serial}|${test}`;
      testStatuses[key] = { device_serial: serial, test_id: test, status: "idle", progress: 0, message: "Queued" };
    }
  }
  buildDashboard();

  addLog({ device_serial: "SYSTEM", test_id: "", timestamp: now(), level: "info",
    message: `Starting tests on ${selectedDevices.size} device(s)` });

  try {
    await invoke("run_test_sequence", {
      devices: [...selectedDevices],
      tests: [...selectedTests],
    });
  } catch (e) {
    addLog({ device_serial: "SYSTEM", test_id: "", timestamp: now(), level: "error", message: `Error: ${e}` });
    isRunning = false;
    updateActionButtons();
  }
}

async function stopTests() {
  try {
    await invoke("stop_tests");
    isRunning = false;
    updateActionButtons();
    addLog({ device_serial: "SYSTEM", test_id: "", timestamp: now(), level: "warn", message: "Tests stopped" });
  } catch (e) { console.error(e); }
}

// ─── Logs ───────────────────────────────────────────
function addLog(entry) {
  logs.push(entry);
  if (logs.length > 500) logs.shift();
  document.getElementById("log-count").textContent = logs.length;
  appendLogLine(entry);
}

function appendLogLine(entry) {
  if (logFilter !== "all" && entry.level !== logFilter) return;
  const console = document.getElementById("log-console");
  const empty = console.querySelector(".log-empty");
  if (empty) empty.remove();

  const line = document.createElement("div");
  line.className = `log-line ${entry.level}`;
  const shortSerial = entry.device_serial.length > 10 ? entry.device_serial.substring(0, 10) + "…" : entry.device_serial;
  line.innerHTML = `<span class="log-time">${entry.timestamp}</span><span class="log-device">${shortSerial}</span><span class="log-msg">${escapeHtml(entry.message)}</span>`;
  console.appendChild(line);
  console.scrollTop = console.scrollHeight;
}

function renderLogs() {
  const console = document.getElementById("log-console");
  console.innerHTML = "";
  const filtered = logFilter === "all" ? logs : logs.filter(l => l.level === logFilter);
  if (filtered.length === 0) {
    console.innerHTML = '<div class="log-empty">No matching logs</div>';
    return;
  }
  filtered.forEach(entry => appendLogLine(entry));
}

function clearLogs() {
  logs = [];
  document.getElementById("log-count").textContent = "0";
  document.getElementById("log-console").innerHTML = '<div class="log-empty">Logs cleared</div>';
}

// ─── Helpers ────────────────────────────────────────
function now() {
  const d = new Date();
  return `${d.getHours().toString().padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}:${d.getSeconds().toString().padStart(2, "0")}`;
}

function escapeHtml(s) {
  const div = document.createElement("div");
  div.textContent = s;
  return div.innerHTML;
}
