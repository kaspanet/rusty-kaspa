const {
    Address,
    createTransactions,
    initConsolePanicHook,
    Mnemonic,
    XPrv,
    PrivateKeyGenerator,
    payToAddressScript,
} = require('../../../../nodejs/kaspa');


(async () => {

    const networkId = 'mainnet';

    const mnemonic = Mnemonic.random();
    const xprv = new XPrv(mnemonic.toSeed());
    const privateKey = new PrivateKeyGenerator(xprv, false, 0n).receiveKey(1);
    const address = privateKey.toAddress(networkId);
    const scriptPublicKey = payToAddressScript(address);
    const entries = [{
        address,
        outpoint: {
            transactionId: '1b84324c701b16c1cfbbd713a5ff87edf78bc5c92a92866f86d7e32ab5cd387d',
            index: 0
        },
        scriptPublicKey,
        amount: 50000000000n,
        isCoinbase: true,
        blockDaaScore: 342n
    }];

    const { transactions, summary } = await createTransactions({
        entries,
        outputs: [{
            address: 'kaspa:qpamkvhgh0kzx50gwvvp5xs8ktmqutcy3dfs9dc3w7lm9rq0zs76vf959mmrp',
            amount: 400000000n
        }],
        changeAddress: address,
        priorityFee: 0n,
        networkId
    });

    for (const pending of transactions) {
        const tx = pending.serializeToObject();
        console.log(tx);
    }
})();
