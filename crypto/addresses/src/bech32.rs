use crate::{Address, AddressError, Prefix};

const CHARSET: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
const REV_CHARSET: [u8; 123] = [
    100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100,
    100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100,
    100, 100, 15, 100, 10, 17, 21, 20, 26, 30, 7, 5, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100,
    100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100,
    100, 100, 100, 29, 100, 24, 13, 25, 9, 8, 23, 100, 18, 22, 31, 27, 19, 100, 1, 0, 3, 16, 11, 28, 12, 14, 6, 4, 2,
];

// Checksome for bech32
// https://bch.info/en/specifications
fn polymod<'data, I>(values: I) -> u64
where
    I: Iterator<Item = &'data u8>,
{
    let mut c = 1u64;
    for d in values {
        let c0 = c >> 35;
        c = ((c & 0x07ffffffff) << 5) ^ (*d as u64);

        if c0 & 0x01 != 0 {
            c ^= 0x98f2bc8e61;
        }
        if c0 & 0x02 != 0 {
            c ^= 0x79b76d99e2;
        }
        if c0 & 0x04 != 0 {
            c ^= 0xf33e5fb3c4;
        }
        if c0 & 0x08 != 0 {
            c ^= 0xae2eabe2a8;
        }
        if c0 & 0x10 != 0 {
            c ^= 0x1e4f43e470;
        }
    }
    c ^ 1
}

fn checksum(payload: &[u8], prefix: &[u8]) -> u64 {
    polymod(
        prefix
            .iter()
            .chain(&[0u8])
            .chain(payload)
            .chain(&vec![0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8]),
    )
}

// Convert 8bit array to 5bit array with right padding
fn conv8to5(payload: &[u8]) -> Vec<u8> {
    payload
        .chunks(5)
        .flat_map(
            // 5b
            |chunk| match chunk.len() {
                0 => vec![],
                1 => vec![chunk[0] >> 3, (chunk[0] & 0x07) << 2],
                2 => vec![
                    chunk[0] >> 3,
                    (chunk[0] & 0x07) << 2 | chunk[1] >> 6,
                    (chunk[1] & 0x3f) >> 1,
                    (chunk[1] & 0x01) << 4,
                ],
                3 => vec![
                    chunk[0] >> 3,
                    (chunk[0] & 0x07) << 2 | chunk[1] >> 6,
                    (chunk[1] & 0x3f) >> 1,
                    (chunk[1] & 0x01) << 4 | chunk[2] >> 4,
                    (chunk[2] & 0x0f) << 1,
                ],
                4 => vec![
                    chunk[0] >> 3,
                    (chunk[0] & 0x07) << 2 | chunk[1] >> 6,
                    (chunk[1] & 0x3f) >> 1,
                    (chunk[1] & 0x01) << 4 | chunk[2] >> 4,
                    (chunk[2] & 0x0f) << 1 | chunk[3] >> 7,
                    (chunk[3] & 0x7f) >> 2,
                    (chunk[3] & 0x03) << 3,
                ],
                5 => vec![
                    chunk[0] >> 3,
                    (chunk[0] & 0x07) << 2 | chunk[1] >> 6,
                    (chunk[1] & 0x3f) >> 1,
                    (chunk[1] & 0x01) << 4 | chunk[2] >> 4,
                    (chunk[2] & 0x0f) << 1 | chunk[3] >> 7,
                    (chunk[3] & 0x7f) >> 2,
                    (chunk[3] & 0x03) << 3 | chunk[4] >> 5,
                    (chunk[4] & 0x1f),
                ],
                _ => unreachable!(),
            },
        )
        .collect()
}

// Convert 5 bit array to 8 bit array, ignore right side padding
fn conv5to8(payload: &[u8]) -> Vec<u8> {
    payload
        .chunks(8)
        .flat_map(
            // 5b
            |chunk| match chunk.len() {
                0 | 1 => vec![],
                2 | 3 => vec![(chunk[0] << 3) | (chunk[1] >> 2)],
                4 => vec![(chunk[0] << 3) | (chunk[1] >> 2), (chunk[1] << 6) | (chunk[2] << 1) | (chunk[3] >> 4)],
                5 | 6 => vec![
                    (chunk[0] << 3) | (chunk[1] >> 2),
                    (chunk[1] << 6) | (chunk[2] << 1) | (chunk[3] >> 4),
                    (chunk[3] << 4) | (chunk[4] >> 1),
                ],
                7 => vec![
                    (chunk[0] << 3) | (chunk[1] >> 2),
                    (chunk[1] << 6) | (chunk[2] << 1) | (chunk[3] >> 4),
                    (chunk[3] << 4) | (chunk[4] >> 1),
                    (chunk[4] << 7) | (chunk[5] << 2) | (chunk[6] >> 3),
                ],
                8 => vec![
                    (chunk[0] << 3) | (chunk[1] >> 2),
                    (chunk[1] << 6) | (chunk[2] << 1) | (chunk[3] >> 4),
                    (chunk[3] << 4) | (chunk[4] >> 1),
                    (chunk[4] << 7) | (chunk[5] << 2) | (chunk[6] >> 3),
                    (chunk[6] << 5) | chunk[7],
                ],
                _ => unreachable!(),
            },
        )
        .collect()
}

impl Address {
    pub(crate) fn encode_payload(&self) -> String {
        // Convert into 5 bits vector
        let fivebit_payload = conv8to5(&[vec![self.version], self.payload.clone()].concat());
        let fivebit_prefix: Vec<u8> = self
            .prefix
            .to_string()
            .as_bytes()
            .iter()
            .map(|c| c & 0x1fu8)
            .collect();

        let checksum = checksum(fivebit_payload.as_slice(), fivebit_prefix.as_slice());

        String::from_utf8(
            [fivebit_payload, conv8to5(&checksum.to_be_bytes()[3..])]
                .concat()
                .iter()
                .map(|c| CHARSET[*c as usize])
                .collect(),
        )
        .expect("All character are valid utf-8")
    }

    pub(crate) fn decode_payload(prefix: Prefix, address: &str) -> Result<Self, AddressError> {
        // From letters to bytes
        let mut err = Ok(());
        let address_u5 = address
            .as_bytes()
            .iter()
            .scan(&mut err, |err, b| match REV_CHARSET[*b as usize] {
                100 => {
                    **err = Err(AddressError::DecodingError(*b as char));
                    None
                }
                i => Some(i),
            })
            .collect::<Vec<u8>>();
        err?;
        let (payload_u5, checksum_u5) = address_u5.split_at(address.len() - 8);
        let fivebit_prefix: Vec<u8> = prefix
            .to_string()
            .as_bytes()
            .iter()
            .map(|c| c & 0x1fu8)
            .collect();

        // Convert to number
        let checksum_ = u64::from_be_bytes(
            [vec![0u8; 3], conv5to8(checksum_u5)]
                .concat()
                .try_into()
                .expect("Is exactly 8 bytes"),
        );

        if checksum(payload_u5, fivebit_prefix.as_slice()) != checksum_ {
            return Err(AddressError::BadChecksum);
        }

        let payload_u8 = conv5to8(payload_u5);
        Ok(Self { prefix, version: payload_u8[0], payload: payload_u8[1..].into() })
    }
}
