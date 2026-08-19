// Create or open an integrated wallet, list its accounts, and stream balances.
//
//   cd wasm/examples
//   npx tsx recipes/wallet/open-and-use-wallet.ts
//   KASPA_WALLET_SECRET=... npx tsx recipes/wallet/open-and-use-wallet.ts
//
// Ctrl-C to stop.

import path from 'node:path';
import fs from 'node:fs';
import { Wallet, AccountKind, Resolver, setDefaultStorageFolder, sompiToKaspaString, initConsolePanicHook } from 'kaspa';
import { getNetworkId } from '../../shared/network';

initConsolePanicHook();

// Wallet files live here (gitignored). A real app uses an OS-appropriate path.
const storageFolder = path.join(import.meta.dirname, '.wallets');
fs.mkdirSync(storageFolder, { recursive: true });
setDefaultStorageFolder(storageFolder);

(async () => {
    const networkId = getNetworkId().toString();
    // The wallet secret decrypts the wallet file. Read it from the environment;
    // the fallback exists only so the example runs out of the box.
    const walletSecret = process.env.KASPA_WALLET_SECRET ?? 'demo-secret';
    const filename = 'example-wallet';

    const wallet = new Wallet({ resident: false, networkId, resolver: new Resolver() });

    wallet.addEventListener((event) => {
        if (event.type === 'balance') {
            console.log('balance:', sompiToKaspaString(event.data.balance.mature));
        }
    });

    if (!(await wallet.exists(filename))) {
        await wallet.walletCreate({ walletSecret, filename, title: 'Example' });
        console.log('created wallet file:', filename);
    }

    await wallet.walletOpen({ walletSecret, filename, accountDescriptors: false });
    await wallet.accountsEnsureDefault({ walletSecret, type: new AccountKind('bip32') });

    await wallet.connect();
    await wallet.start();

    const { accountDescriptors } = await wallet.accountsEnumerate({});
    for (const account of accountDescriptors) {
        await wallet.accountsActivate({ accountIds: [account.accountId] });
        console.log(`account ${account.accountName ?? '(default)'}: ${account.receiveAddress}`);
    }

    process.on('SIGINT', () => process.exit(0));
})();
