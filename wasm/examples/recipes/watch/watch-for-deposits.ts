// Watch an address for incoming funds using UtxoProcessor + UtxoContext.
//
//   cd wasm/examples
//   npx tsx recipes/watch/watch-for-deposits.ts --address=kaspatest:...
//
// Ctrl-C to stop.

import { Resolver, RpcClient, UtxoProcessor, UtxoContext, sompiToKaspaString, initConsolePanicHook } from 'kaspa';
import { getNetworkId } from '../../shared/network';

initConsolePanicHook();

(async () => {
    const networkId = getNetworkId();

    const addressFlag = process.argv.slice(2).find((a) => a.startsWith('--address='));
    const address = addressFlag && addressFlag.split('=')[1];
    if (!address) {
        console.error('Pass an address to watch: --address=kaspatest:...');
        process.exit(1);
    }

    const rpc = new RpcClient({ resolver: new Resolver(), networkId });
    const processor = new UtxoProcessor({ rpc, networkId });
    const context = new UtxoContext({ processor });

    processor.addEventListener('balance', (event) => {
        const balance = event.data.balance;
        console.log(`balance: mature ${sompiToKaspaString(balance.mature)}, pending ${sompiToKaspaString(balance.pending)}`);
    });

    // Start tracking only once the processor is running.
    processor.addEventListener('utxo-proc-start', async () => {
        await context.trackAddresses([address]);
        console.log('watching', address);
    });

    await processor.start();
    await rpc.connect();

    process.on('SIGINT', async () => {
        await processor.stop();
        await rpc.disconnect();
        process.exit(0);
    });
})();
