from kaspa import PrivateKey, PublicKey, sign_message, verify_message

if __name__ == "__main__":
    message = "Hello Kaspa!"
    private_key = PrivateKey("b7e151628aed2a6abf7158809cf4f3c762e7160f38b4da56a784d9045190cfef")
    public_key = PublicKey("dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659")

    signature = sign_message(message, private_key)
    print(f'Signature: {signature}')

    valid_sig = verify_message(message, signature, public_key) 
    print('Valid sig' if valid_sig else 'Invalid sig')
