from kaspa import (
    DerivationPath,
    Mnemonic,
    PublicKey,
    XPrv
)

if __name__ == "__main__":
    mnemonic = Mnemonic("hunt bitter praise lift buyer topic crane leopard uniform network inquiry over grain pass match crush marine strike doll relax fortune trumpet sunny silk")
    seed = mnemonic.to_seed()

    xprv = XPrv(seed)

    # Create receive wallet
    receive_wallet_xpub = xprv.derive_path("m/44'/111111'/0'/0").to_xpub()
    # Derive receive wallet for second address
    pubkey2 = receive_wallet_xpub.derive_child(1, False).to_public_key()
    print(f'Receive Address: {pubkey2.to_address("mainnet").to_string()}')

    # Create change wallet
    change_wallet_xpub = xprv.derive_path("m/44'/111111'/0'/1").to_xpub()
    # Derive change wallet for first address
    pubkey3 = change_wallet_xpub.derive_child(0, False).to_public_key()
    print(f'Change Address: {pubkey3.to_address("mainnet").to_string()}')

    # Derive address via public key
    private_key = xprv.derive_path("m/44'/111111'/0'/0/1").to_private_key()
    print(f'Address via private key: {private_key.to_address("mainnet").to_string()}')
    print(f'Private key: {private_key.to_string()}')

    # XPrv with ktrv prefix
    ktrv = xprv.into_string("ktrv")
    print(f'ktrv prefix: {ktrv}')

    # Create derivation path
    path = DerivationPath("m/1'")
    path.push(2, True)
    path.push(3, False)
    print(f'Derivation Path: {path.to_string()}')

    # Derive by path string
    print(f'{xprv.derive_path("m/1'/2'/3").into_string("xprv")}')
    # Derive by DerivationPath object
    print(f'{xprv.derive_path(path).into_string("xprv")}')
    # Create XPrv from ktrvx string and derive it
    print(f'{XPrv.from_xprv(ktrv).derive_path("m/1'/2'/3").into_string("xprv")}')

    # Get xpub
    xpub = xprv.to_xpub()
    # Derive xpub
    print(xpub.derive_path("m/1").into_string("xpub"))
    # Get public key from xpub
    print(xpub.to_public_key().to_string())


