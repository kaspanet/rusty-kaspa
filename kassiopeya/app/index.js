
(async()=>{
    window.$kassiopeya = await import('/app/wasm/kassiopeya.js');
    const wasm = await window.$kassiopeya.default('/app/wasm/kassiopeya_bg.wasm');
    $kassiopeya.init_console_panic_hook();
    window.$kassiopeya.initialize_kassiopeya_application();
})();
