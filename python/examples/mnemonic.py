from kaspa import Language, Mnemonic

if __name__ == "__main__":
    mnemonic1 = Mnemonic.random()
    print(mnemonic1.phrase)

    mnemonic2 = Mnemonic(phrase=mnemonic1.phrase)

    print(mnemonic1.entropy)
    print(mnemonic1.to_seed())