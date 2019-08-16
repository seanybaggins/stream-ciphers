//! XChaCha20 is an adaptation of the XSalsa20 extended nonce construction
//! described in the paper "Extending the Salsa20 Nonce", but using the
//! 96-bit nonce variant of ChaCha20 rather than Salsa20 as the underlying
//! cipher, deriving a shorter key/nonce from an extended nonce:
//!
//! <https://cr.yp.to/snuffle/xsalsa-20081128.pdf>
//!
//! No authoritative specification exists for XChaCha20, however the
//! construction has "rough consensus and running code" in the form of
//! several interoperable libraries and protocols (e.g. libsodium, WireGuard)
//! and is documented in an (expired) IETF draft:
//!
//! <https://tools.ietf.org/html/draft-arciszewski-xchacha-03>

use super::{quarter_round, ChaCha20};
use block_cipher_trait::generic_array::typenum::{U16, U24, U32};
use block_cipher_trait::generic_array::GenericArray;
use byteorder::{ByteOrder, LE};
use core::ops::{Deref, DerefMut};
#[cfg(feature = "zeroize")]
use salsa20_core::zeroize::Zeroize;
use stream_cipher::NewStreamCipher;

/// XChaCha20 is an extended nonce variant of ChaCha20
pub struct XChaCha20(ChaCha20);

impl NewStreamCipher for XChaCha20 {
    /// Key size in bytes
    type KeySize = U32;

    /// Nonce size in bytes
    type NonceSize = U24;

    #[allow(unused_mut, clippy::let_and_return)]
    fn new(key: &GenericArray<u8, Self::KeySize>, iv: &GenericArray<u8, Self::NonceSize>) -> Self {
        let mut subkey = hchacha20(key, iv[..16].as_ref().into());
        let mut padded_iv = GenericArray::default();
        padded_iv[4..].copy_from_slice(&iv[16..]);

        let mut result = XChaCha20(ChaCha20::new(&subkey, &padded_iv));

        #[cfg(feature = "zeroize")]
        {
            subkey.as_mut_slice().zeroize();
        }

        result
    }
}

impl Deref for XChaCha20 {
    type Target = ChaCha20;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for XChaCha20 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// The HChaCha20 function: adapts the ChaCha20 core function in the same
/// manner that HSalsa20 adapts the Salsa20 function.
///
/// HChaCha20 takes 512-bits of input:
///
/// * Constants (`u32` x 4)
/// * Key (`u32` x 8)
/// * Nonce (`u32` x 4)
///
/// It produces 256-bits of output suitable for use as a ChaCha20 key
///
/// For more information on HSalsa20 on which HChaCha20 is based, see:
///
/// <http://cr.yp.to/snuffle/xsalsa-20110204.pdf>
fn hchacha20(key: &GenericArray<u8, U32>, input: &GenericArray<u8, U16>) -> GenericArray<u8, U32> {
    let mut state = [0u32; 16];

    state[0] = 0x6170_7865;
    state[1] = 0x3320_646e;
    state[2] = 0x7962_2d32;
    state[3] = 0x6b20_6574;

    for (i, chunk) in key.chunks(4).take(8).enumerate() {
        state[4 + i] = LE::read_u32(chunk);
    }

    for (i, chunk) in input.chunks(4).enumerate() {
        state[12 + i] = LE::read_u32(chunk);
    }

    // 20 rounds consisting of 10 column rounds and 10 diagonal rounds
    for _ in 0..10 {
        // column rounds
        quarter_round(0, 4, 8, 12, &mut state);
        quarter_round(1, 5, 9, 13, &mut state);
        quarter_round(2, 6, 10, 14, &mut state);
        quarter_round(3, 7, 11, 15, &mut state);

        // diagonal rounds
        quarter_round(0, 5, 10, 15, &mut state);
        quarter_round(1, 6, 11, 12, &mut state);
        quarter_round(2, 7, 8, 13, &mut state);
        quarter_round(3, 4, 9, 14, &mut state);
    }

    let mut output = GenericArray::default();

    for (i, chunk) in output.chunks_mut(4).take(4).enumerate() {
        LE::write_u32(chunk, state[i]);
    }

    for (i, chunk) in output.chunks_mut(4).skip(4).enumerate() {
        LE::write_u32(chunk, state[i + 12]);
    }

    output
}

#[cfg(test)]
mod hchacha20_tests {
    use super::*;

    //
    // Test vectors from:
    // https://tools.ietf.org/id/draft-arciszewski-xchacha-03.html#rfc.section.2.2.1
    //

    const KEY: [u8; 32] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
        0x1e, 0x1f,
    ];

    const INPUT: [u8; 16] = [
        0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x4a, 0x00, 0x00, 0x00, 0x00, 0x31, 0x41, 0x59,
        0x27,
    ];

    const OUTPUT: [u8; 32] = [
        0x82, 0x41, 0x3b, 0x42, 0x27, 0xb2, 0x7b, 0xfe, 0xd3, 0xe, 0x42, 0x50, 0x8a, 0x87, 0x7d,
        0x73, 0xa0, 0xf9, 0xe4, 0xd5, 0x8a, 0x74, 0xa8, 0x53, 0xc1, 0x2e, 0xc4, 0x13, 0x26, 0xd3,
        0xec, 0xdc,
    ];

    #[test]
    fn test_vector() {
        let actual = hchacha20(
            GenericArray::from_slice(&KEY),
            &GenericArray::from_slice(&INPUT),
        );
        assert_eq!(actual.as_slice(), &OUTPUT);
    }
}
