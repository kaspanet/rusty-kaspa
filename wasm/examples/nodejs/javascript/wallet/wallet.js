// @ts-ignore
globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim


const path = require('path');
const fs = require('fs');
const kaspa = require('../../../../nodejs/kaspa');
const {
    Wallet, setDefaultStorageFolder,
    AccountKind, Mnemonic, Resolver,
    kaspaToSompi,
    sompiToKaspaString,
    Address
} = kaspa;

let storageFolder = path.join(__dirname, '../../../data/wallets').normalize();
if (!fs.existsSync(storageFolder)) {
    fs.mkdirSync(storageFolder);
}

setDefaultStorageFolder(storageFolder);

(async()=>{
    //const filename = "wallet-394";
    const filename = "wallet-395";
    // const FILE_TX = path.join(storageFolder, filename+"-transactions.json");

    // let txs = {};
    // if (fs.existsSync(FILE_TX)){
    //     txs = JSON.parse(fs.readFileSync(FILE_TX)+"")
    // }

    const balance = {};
    //const transactions = {};
    let wallet;

    const chalk = new ((await import('chalk')).Chalk)();

    function log_title(title){
        console.log(chalk.bold(chalk.green(`\n\n${title}`)))
    }
    // function saveTransactions(){
    //     Object.keys(transactions).forEach(id=>{
    //         txs[id] = [...transactions[id].entries()];
    //     })
    //     fs.writeFileSync(FILE_TX, JSON.stringify(txs))
    // }

    async function log_transactions(accountId){
        
        // saveTransactions();
        function value(tx){
            if (tx.data.type == "change"){
                return tx.data.data.changeValue
            }
            if (["external", "incoming"].includes(tx.data.type)){
                return tx.data.data.value
            }
            if (["transfer-outgoing", "transfer-incoming", "outgoing", "batch"].includes(tx.data.type)){
                return tx.data.data.paymentValue?? tx.data.data.changeValue
            }
            
        }
        let transactionsResult = await wallet.transactionsDataGet({
            accountId,
            networkId: "testnet-11",
            start:0,
            end:20
        })
        //console.log("transactions", transactionsResult.transactions)
        let list = [];
        transactionsResult.transactions.forEach((tx)=>{
            // console.log("ID:", id);
            // console.log("type:", tx.data.type, ", value:", value(tx));
            // console.log(chalk.dim("----------------------------"))
            // let addresses = tx.data.data.utxoEntries.map(utxo=>{
            //     return utxo.address.substring(0, 5)+"..."
            // });
            list.push({
                Id: tx.id,
                Type: tx.data.type,
                Value: sompiToKaspaString(value(tx)||0)
            });
            //console.log("tx.data", tx.id, tx.data)
        });

        log_title("Transactions")
        console.table(list)
        console.log("");

    }

    try {
        
        const walletSecret = "abc";
        wallet = new Wallet({resident: false, networkId: "testnet-11", resolver: new Resolver()});
        //console.log("wallet", wallet)
        // Ensure wallet file
        if (!await wallet.exists(filename)){
            let response = await wallet.walletCreate({
                walletSecret,
                filename,
                title: "W-1"
            });
            console.log("walletCreate : response", response)
        }

        wallet.addEventListener(({type, data})=>{

            switch (type){
                case "maturity":
                case "pending":
                case "discovery":
                    //console.log("transactions[data.binding.id]", data.binding.id, transactions[data.binding.id], transactions)
                    // console.log("record.hasAddress :receive:", data.hasAddress(firstAccount.receiveAddress));
                    // console.log("record.hasAddress :change:", data.hasAddress(firstAccount.changeAddress));
                    // console.log("record.data", data.data)
                    // console.log("record.blockDaaScore", data.blockDaaScore)
                    if (data.type != "change"){
                        //transactions[data.binding.id].set(data.id+"", data.serialize());
                        log_transactions(data.binding.id)
                    }
                break;
                case "reorg":
                    //transactions[data.binding.id].delete(data.id+"");
                    log_transactions(data.binding.id)
                break;
                case "balance":
                    balance[data.id] = data.balance;
                    log_title("Balance");
                    let list = [];
                    Object.keys(balance).map(id=>{
                        list.push({
                            Account: id.substring(0, 5)+"...",
                            Mature: sompiToKaspaString(data.balance.mature),
                            Pending: sompiToKaspaString(data.balance.pending),
                            Outgoing: sompiToKaspaString(data.balance.outgoing),
                            MatureUtxo: data.balance.matureUtxoCount,
                            PendingUtxo: data.balance.pendingUtxoCount,
                            StasisUtxo: data.balance.stasisUtxoCount
                        })
                    })
                    console.table(list)
                    console.log("");
                break;
                case "daa-score-change":
                    if (data.currentDaaScore%1000 == 0){
                        console.log(`[EVENT] ${type}:`, data.currentDaaScore)
                    }
                break;
                case "server-status":
                case "utxo-proc-start":
                case "sync-state":
                case "account-activation":
                case "utxo-proc-stop":
                case "connect":
                case "stasis":
                    //
                break;
                default:
                    console.log(`[EVENT] ${type}:`, data)
                
            }
        })

        // Open wallet
        await wallet.walletOpen({
            walletSecret,
            filename,
            accountDescriptors: false
        });

        // Ensure default account
        await wallet.accountsEnsureDefault({
            walletSecret,
            type: new AccountKind("bip32") // "bip32"
        });

        // // Create a new account
        // // create private key
        // let prvKeyData =  await wallet.prvKeyDataCreate({
        //     walletSecret,
        //     mnemonic: Mnemonic.random(24).phrase
        // });

        // //console.log("prvKeyData", prvKeyData);

        // let account = await wallet.accountsCreate({
        //     walletSecret,
        //     type:"bip32",
        //     accountName:"Account-B",
        //     prvKeyDataId: prvKeyData.prvKeyDataId
        // });

        // console.log("new account:", account);

        // Connect to rpc
        await wallet.connect();

        // Start wallet processing
        await wallet.start();

        // List accounts
        let accounts = await wallet.accountsEnumerate({});
        let firstAccount = accounts.accountDescriptors[0];

        //console.log("firstAccount:", firstAccount);

        // Activate Account
        await wallet.accountsActivate({
            accountIds:[firstAccount.accountId]
        });

        log_title("Accounts");
        accounts.accountDescriptors.forEach(a=>{
            console.log(`Account: ${a.accountId}`);
            console.log(`   Account type: ${a.kind.toString()}`);
            console.log(`   Account Name: ${a.accountName}`);
            console.log(`   Receive Address: ${a.receiveAddress}`);
            console.log(`   Change Address: ${a.changeAddress}`);
            console.log("")
        });

        // // Account sweep/compound transactions
        // let sweepResult = await wallet.accountsSend({
        //     walletSecret,
        //     accountId: firstAccount.accountId
        // });
        // console.log("sweepResult", sweepResult)

        // Send kaspa to address
        let sendResult = await wallet.accountsSend({
            walletSecret,
            accountId: firstAccount.accountId,
            priorityFeeSompi: kaspaToSompi("0.001"),
            destination:[{
                address: firstAccount.changeAddress,
                amount: kaspaToSompi("1.5")
            }]
        });
        console.log("sendResult", sendResult);

        // Transfer kaspa between accounts
        let transferResult = await wallet.accountsTransfer({
            walletSecret,
            sourceAccountId: firstAccount.accountId,
            destinationAccountId: firstAccount.accountId,
            transferAmountSompi: kaspaToSompi("2.4"),
        });
        console.log("transferResult", transferResult);

        
    } catch(ex) {
        console.error("Error:", ex);
    }
})();