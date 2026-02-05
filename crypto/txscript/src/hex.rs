/// Hex decode helper backed by `faster-hex`.
#[inline]
pub fn decode<T: AsRef<[u8]>>(data: T) -> Result<Vec<u8>, faster_hex::Error> {
    let input = data.as_ref();
    if input.len() % 2 != 0 {
        return Err(faster_hex::Error::InvalidLength(input.len()));
    }

    let mut bytes = vec![0u8; input.len() / 2];
    faster_hex::hex_decode(input, &mut bytes)?;
    Ok(bytes)
}
