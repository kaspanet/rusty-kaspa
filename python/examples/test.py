from kaspapy import PrivateKeyGenerator

if __name__ == "__main__":
    x = PrivateKeyGenerator('xprv9s21ZrQH143K2hP7m1bU4ZT6tWgX1Qn2cWvtLVDX6sTJVyg3XBa4p1So4s7uEvVFGyBhQWWRe8JeLPeDZ462LggxkkJpZ9z1YMzmPahnaZA', False, 1)
    print(x.receive_key(2))
    print(x.change_key(2))
