document.body.innerHTML =
`<a href="index.html"><- Back</a> | Network: <span id="menu"></span><span id="actions"></span><br>&nbsp;<br>`;

// @ts-ignore
String.prototype.color = function(color) {
    console.log(this,color);
    return `<span style="color: ${color}">${this}</span>`;
}

// @ts-ignore
String.prototype.class = function(className) {
    return `<span class="${className}">${this}</span>`;
}

function currentNetwork() {
    return window.location.hash.replace(/^#/,'') || 'mainnet';
}

// @ts-ignore
window.changeNetwork = (network) => {
    console.log("network",network);
    window.location.hash = network;
    location.reload();
}

function createMenu() {
    console.log("adding menu...");
    let menu = document.getElementById('menu');
    [ 'mainnet', 'testnet-10', 'testnet-11' ].forEach((network) => {
        if (network === currentNetwork()) {
            let el = document.createElement('text');
            el.innerHTML = ` [${network}] `;
            menu.appendChild(el);
        } else {
            
            let el = document.createElement('a');
            el.id = network;
            el.href = `javascript: changeNetwork("${network}")`;
            el.innerHTML = ` ${network} `;
            menu.appendChild(el);
        }
    });
}

document.addEventListener('DOMContentLoaded', () => {
    createMenu();
});

function disconnectAction(rpc) {
    // @ts-ignore
    window.$rpc = rpc;
    let actions = document.getElementById('actions');
    actions.innerHTML = ` | <a href="javascript: disconnect()">Disconnect</a>`;
}

// @ts-ignore
window.disconnect = function() {
    // @ts-ignore
    $rpc.disconnect();
    document.getElementById('actions').innerHTML = ' | Disconnected';
    document.body.innerHTML += '\nDisconnected...';
}

function log(...args) {
    document.body.innerHTML += args.map((arg) => {
        return typeof arg === 'object' ? stringify(arg) : arg;
    }).join(' ') + "<br>";
}

function stringify(json) {
    if (typeof json != 'string') {
        json = JSON.stringify(json, (k, v) => { return typeof v === "bigint" ? v.toString() + 'n' : v; }, 2);
    }
    json = json.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"(\d+)n"/g,"$1n");
    return json.replace(/("(\\u[a-zA-Z0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+\-]?\d+)?n?)/g, function (match) {
        var cls = 'number';
        if (/^"/.test(match)) {
            if (/:$/.test(match)) {
                cls = 'key';
            } else {
                cls = 'string';
            }
        } else if (/true|false/.test(match)) {
            cls = 'boolean';
        } else if (/null/.test(match)) {
            cls = 'null';
        }
        return '<span class="' + cls + '">' + match + '</span>';
    });
}

export { log, stringify, currentNetwork, disconnectAction };