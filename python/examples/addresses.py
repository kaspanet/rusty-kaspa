from kaspa import (
    PublicKey,
    PublicKeyGenerator,
    PrivateKey, 
    Keypair,
    # create_address
)

def demo_generate_address_from_public_key_hex_string():
    # Compressed public key "02dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659"
    public_key = PublicKey("02dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659")
    print("\nGiven compressed public key: 02dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659")
    print(public_key.to_string())
    print(public_key.to_address("mainnet").to_string())

    # x-only public key: "dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659"
    x_only_public_key = PublicKey("dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659")
    print("\nGiven x-only public key: dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659")
    print(x_only_public_key.to_string())
    print(x_only_public_key.to_address("mainnet").to_string())

    # EDR public  key
    full_der_public_key = PublicKey("0421eb0c4270128b16c93c5f0dac48d56051a6237dae997b58912695052818e348b0a895cbd0c93a11ee7afac745929d96a4642a71831f54a7377893af71a2e2ae")
    print("\nGiven x-only public key: 0421eb0c4270128b16c93c5f0dac48d56051a6237dae997b58912695052818e348b0a895cbd0c93a11ee7afac745929d96a4642a71831f54a7377893af71a2e2ae")
    print(full_der_public_key.to_string())
    print(full_der_public_key.to_address("mainnet").to_string())

def demo_generate_address_from_private_key_hex_string():
    private_key = PrivateKey("b7e151628aed2a6abf7158809cf4f3c762e7160f38b4da56a784d9045190cfef")
    print("\nGiven private key b7e151628aed2a6abf7158809cf4f3c762e7160f38b4da56a784d9045190cfef")
    print(private_key.to_keypair().to_address("kaspa").to_string())

def demo_generate_random():
    keypair = Keypair.random()
    print("\nRandom Generation")
    print(keypair.private_key())
    print(keypair.public_key())
    print(keypair.to_address("kaspa").to_string())

if __name__ == "__main__":
    demo_generate_address_from_public_key_hex_string()
    demo_generate_address_from_private_key_hex_string()
    demo_generate_random()

    # HD Wallet style pub key gen
    xpub = PublicKeyGenerator.from_master_xprv(
        "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ",
        False,
        0
    )
    print(xpub.to_string())

    # Generates the first 10 Receive Public Keys and their addresses
    compressed_public_keys = xpub.receive_pubkeys(0, 10)
    print("\nreceive address compressed_public_keys")
    for key in compressed_public_keys:
        print(key.to_string(), key.to_address("mainnet").to_string())

    # Generates the first 10 Change Public Keys and their addresses
    compressed_public_keys = xpub.change_pubkeys(0, 10)
    print("\nchange address compressed_public_keys")
    for key in compressed_public_keys:
        print(key.to_string(), key.to_address("mainnet").to_string())

