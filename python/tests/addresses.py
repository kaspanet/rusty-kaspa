from kaspapy import (
    PrivateKey, 
)

if __name__ == "__main__":
    private_key = PrivateKey('b7e151628aed2a6abf7158809cf4f3c762e7160f38b4da56a784d9045190cfef')
    print(f'Private Key: {private_key.to_hex()}')

    public_key = private_key.to_public_key()
    print(f'Public Key: {public_key.to_string_impl()}')

    address = public_key.to_address('mainnet')
    print(f'Address: {address.address_to_string()}')