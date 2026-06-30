// Import a raw private key into the integrated wallet as its own account.
//
//   cd wasm/examples
//   KASPA_PRIVATE_KEY="<64 hex chars>" npx tsx recipes/wallet/import-from-secret-key.ts

import path from 'node:path';
import fs from 'node:fs';
import { Wallet, Resolver, setDefaultStorageFolder, initConsolePanicHook } from 'kaspa';
import { getNetworkId } from '../../shared/network';
import { requirePrivateKeyHex } from '../../shared/secrets';

initConsolePanicHook();

const storageFolder = path.join(__dirname, '.wallets');
fs.mkdirSync(storageFolder, { recursive: true });
setDefaultStorageFolder(storageFolder);

(async () => {
    const networkId = getNetworkId().toString();
    const walletSecret = process.env.KASPA_WALLET_SECRET ?? 'demo-secret';
    const filename = 'imported-wallet';
    const secretKey = requirePrivateKeyHex();

    const wallet = new Wallet({ resident: false, networkId, resolver: new Resolver() });

    if (!(await wallet.exists(filename))) {
        await wallet.walletCreate({ walletSecret, filename, title: 'Imported' });
    }
    await wallet.walletOpen({ walletSecret, filename, accountDescriptors: false });

    // Store the raw key as new private-key data, then create an account from it.
    const prvKeyData = await wallet.prvKeyDataCreate({ walletSecret, kind: 'secretKey', secretKey });
    await wallet.accountsCreate({
        walletSecret,
        type: 'kaspa-keypair-standard',
        accountName: 'Imported',
        prvKeyDataId: prvKeyData.prvKeyDataId,
    });

    await wallet.connect();
    await wallet.start();

    const { accountDescriptors } = await wallet.accountsEnumerate({});
    for (const account of accountDescriptors) {
        console.log(`${account.accountName}: ${account.receiveAddress}`);
    }

    process.on('SIGINT', () => process.exit(0));
})();
