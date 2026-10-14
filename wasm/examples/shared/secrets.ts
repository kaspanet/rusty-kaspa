import { Mnemonic } from 'kaspa';

/**
 * Build a `Mnemonic` from a phrase, exiting with a readable message instead of
 * the raw `Bip39 error` panic when the phrase is not a valid BIP39 mnemonic.
 */
function parseMnemonic(phrase: string): Mnemonic {
    try {
        return new Mnemonic(phrase);
    } catch {
        const words = phrase.trim().split(/\s+/).filter(Boolean).length;
        console.error(`KASPA_MNEMONIC is not a valid BIP39 mnemonic (got ${words} word(s)).`);
        if (words === 12 || words === 24) {
            console.error('Mnemonic is invalid.');
        } else {
            console.error('Expected 12 or 24 space-separated words from the BIP39 wordlist.');
        }
        process.exit(1);
    }
}

/**
 * Read a BIP39 mnemonic from `KASPA_MNEMONIC`, or exit with guidance.
 * Use this in examples that must operate on the caller's real wallet.
 */
export function requireMnemonic(): Mnemonic {
    const phrase = process.env.KASPA_MNEMONIC;
    if (!phrase) {
        console.error('Set KASPA_MNEMONIC to a BIP39 phrase:');
        console.error('  KASPA_MNEMONIC="<12 or 24 words>"');
        process.exit(1);
    }
    return parseMnemonic(phrase);
}

/**
 * Return the `KASPA_MNEMONIC` mnemonic, or a throwaway random one so the
 * example still runs out of the box. The phrase is never printed either way.
 */
export function mnemonicOrEphemeral(): { mnemonic: Mnemonic; ephemeral: boolean } {
    const phrase = process.env.KASPA_MNEMONIC;
    if (phrase) return { mnemonic: parseMnemonic(phrase), ephemeral: false };
    return { mnemonic: Mnemonic.random(), ephemeral: true };
}

/**
 * Read a private key (hex) from `KASPA_PRIVATE_KEY`, or exit with guidance.
 */
export function requirePrivateKeyHex(): string {
    const hex = process.env.KASPA_PRIVATE_KEY;
    if (!hex) {
        console.error('Set KASPA_PRIVATE_KEY to a hex-encoded private key:');
        console.error('  KASPA_PRIVATE_KEY="<64 hex chars>"');
        process.exit(1);
    }
    return hex;
}
