false && process.versions["nw-flavor"] === "sdk" && chrome.developerPrivate.openDevTools({
	renderViewId: -1,
	renderProcessId: -1,
	extensionId: chrome.runtime.id,
});

(async()=>{
    window.kaspa = await import('/app/wasm/kaspa.js');
    const wasm = await window.kaspa.default('/app/wasm/kaspa_bg.wasm');
    await window.kaspa.init_core();
})();
