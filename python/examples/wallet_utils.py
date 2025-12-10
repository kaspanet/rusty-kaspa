from kaspa import kaspa_to_sompi, sompi_to_kaspa, sompi_to_kaspa_string_with_suffix

if __name__ == "__main__":
    print(kaspa_to_sompi(100.833))

    sompi = 499_922_100
    print(sompi_to_kaspa(sompi))
    print(sompi_to_kaspa_string_with_suffix(sompi, "mainnet"))