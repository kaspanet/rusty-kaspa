from kaspa import Language, Mnemonic

if __name__ == "__main__":
    mnemonic1 = Mnemonic.random()
    print(f'mnemonic 1: {mnemonic1.phrase}')

    mnemonic2 = Mnemonic(phrase=mnemonic1.phrase)
    print(f'mnemonic 2: {mnemonic2.phrase}')

    # Create seed with a recovery password (25th word)
    seed1 = mnemonic1.to_seed("my_password")
    print(f'seed1: {seed1}')

    seed2 = mnemonic2.to_seed("my_password")
    print(f'seed2: {seed2}')

    # Create seed without recovery password
    seed3 = mnemonic1.to_seed()
    print(f'seed3 (no recovery password): {seed3}')