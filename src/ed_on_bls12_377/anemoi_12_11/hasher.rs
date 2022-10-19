//! Sponge trait implementation for Anemoi

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::digest::AnemoiDigest;
use super::{apply_permutation, DIGEST_SIZE, NUM_COLUMNS, RATE_WIDTH, STATE_WIDTH};
use super::{Jive, Sponge};

use super::Felt;
use super::{One, Zero};

use ark_ff::FromBytes;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
/// An Anemoi hash instantiation
pub struct AnemoiHash {
    state: [Felt; STATE_WIDTH],
    idx: usize,
}

impl Sponge<Felt> for AnemoiHash {
    type Digest = AnemoiDigest;

    fn hash(bytes: &[u8]) -> Self::Digest {
        // Compute the number of field elements required to represent this
        // sequence of bytes.
        let num_elements = if bytes.len() % 31 == 0 {
            bytes.len() / 31
        } else {
            bytes.len() / 31 + 1
        };

        let sigma = if num_elements % RATE_WIDTH == 0 {
            Felt::one()
        } else {
            Felt::zero()
        };

        // Initialize the internal hash state to all zeroes.
        let mut state = [Felt::zero(); STATE_WIDTH];

        // Absorption phase

        // Break the string into 31-byte chunks, then convert each chunk into a field element,
        // and absorb the element into the rate portion of the state. The conversion is
        // guaranteed to succeed as we spare one last byte to ensure this can represent a valid
        // element encoding.
        let mut i = 0;
        let mut num_hashed = 0;
        let mut buf = [0u8; 32];
        for chunk in bytes.chunks(31) {
            if num_hashed + i < num_elements - 1 {
                buf[..31].copy_from_slice(chunk);
            } else {
                // The last chunk may be smaller than the others, which requires a special handling.
                // In this case, we also append a byte set to 1 to the end of the string, padding the
                // sequence in a way that adding additional trailing zeros will yield a different hash.
                let chunk_len = chunk.len();
                buf = [0u8; 32];
                buf[..chunk_len].copy_from_slice(chunk);
                // [Different to paper]: We pad the last chunk with 1 to prevent length extension attack.
                if chunk_len < 31 {
                    buf[chunk_len] = 1;
                }
            }

            // Convert the bytes into a field element and absorb it into the rate portion of the
            // state. An Anemoi permutation is applied to the internal state if all the the rate
            // registers have been filled with additional values. We then reset the insertion index.
            state[i] += Felt::read(&buf[..]).unwrap();
            i += 1;
            if i % RATE_WIDTH == 0 {
                apply_permutation(&mut state);
                i = 0;
                num_hashed += RATE_WIDTH;
            }
        }

        // We then add sigma to the last register of the capacity.
        state[STATE_WIDTH - 1] += sigma;

        // If the message length is not a multiple of RATE_WIDTH, we append 1 to the rate cell
        // next to the one where we previously appended the last message element. This is
        // guaranted to be in the rate registers (i.e. to not require an extra permutation before
        // adding this constant) if sigma is equal to zero. We then apply a final Anemoi permutation
        // to the whole state.
        if sigma.is_zero() {
            state[i] += Felt::one();
            apply_permutation(&mut state);
        }

        // Squeezing phase

        // Finally, return the first DIGEST_SIZE elements of the state.
        Self::Digest::new(state[..DIGEST_SIZE].try_into().unwrap())
    }

    fn hash_field(elems: &[Felt]) -> Self::Digest {
        // initialize state to all zeros
        let mut state = [Felt::zero(); STATE_WIDTH];

        let sigma = if elems.len() % RATE_WIDTH == 0 {
            Felt::one()
        } else {
            Felt::zero()
        };

        let mut i = 0;
        for &element in elems.iter() {
            state[i] += element;
            i += 1;
            if i % RATE_WIDTH == 0 {
                apply_permutation(&mut state);
                i = 0;
            }
        }

        // We then add sigma to the last register of the capacity.
        state[STATE_WIDTH - 1] += sigma;

        // If the message length is not a multiple of RATE_WIDTH, we append 1 to the rate cell
        // next to the one where we previously appended the last message element. This is
        // guaranted to be in the rate registers (i.e. to not require an extra permutation before
        // adding this constant) if sigma is equal to zero. We then apply a final Anemoi permutation
        // to the whole state.
        if sigma.is_zero() {
            state[i] += Felt::one();
            apply_permutation(&mut state);
        }

        // Squeezing phase

        Self::Digest::new(state[..DIGEST_SIZE].try_into().unwrap())
    }

    fn merge(digests: &[Self::Digest; 2]) -> Self::Digest {
        // initialize state to all zeros
        let mut state = [Felt::zero(); STATE_WIDTH];

        // 2*DIGEST_SIZE < RATE_SIZE so we can safely store
        // the digests into the rate registers at once
        state[0..DIGEST_SIZE].copy_from_slice(digests[0].as_elements());
        state[DIGEST_SIZE..2 * DIGEST_SIZE].copy_from_slice(digests[0].as_elements());

        // Apply internal Anemoi permutation
        apply_permutation(&mut state);

        Self::Digest::new(state[..DIGEST_SIZE].try_into().unwrap())
    }
}

impl Jive<Felt> for AnemoiHash {
    fn compress(elems: &[Felt]) -> Vec<Felt> {
        assert!(elems.len() == STATE_WIDTH);

        let mut state = elems.try_into().unwrap();
        apply_permutation(&mut state);

        let mut result = [Felt::zero(); NUM_COLUMNS];
        for (i, r) in result.iter_mut().enumerate() {
            *r = elems[i] + elems[i + NUM_COLUMNS] + state[i] + state[i + NUM_COLUMNS];
        }

        result.to_vec()
    }

    fn compress_k(elems: &[Felt], k: usize) -> Vec<Felt> {
        assert!(elems.len() == STATE_WIDTH);
        assert!(STATE_WIDTH % k == 0);
        assert!(k % 2 == 0);

        let mut state = elems.try_into().unwrap();
        apply_permutation(&mut state);

        let mut result = vec![Felt::zero(); STATE_WIDTH / k];
        let c = result.len();
        for (i, r) in result.iter_mut().enumerate() {
            for j in 0..k {
                *r += elems[i + c * j] + state[i + c * j];
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::super::BigInteger256;
    use super::*;
    use ark_ff::to_bytes;

    #[test]
    fn test_anemoi_hash() {
        // Generated from https://github.com/anemoi-hash/anemoi-hash/
        let input_data = [
            vec![
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
            ],
            vec![
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
            ],
            vec![
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
            ],
            vec![
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
            ],
            vec![Felt::new(BigInteger256([
                0x887510ecc73c2e89,
                0x2d61826d0b49c3b2,
                0x30e0c2807f35df12,
                0x0d2a6177c437a466,
            ]))],
            vec![
                Felt::new(BigInteger256([
                    0x5de8485ed12ffbe4,
                    0x08f03c349de663e4,
                    0x372a195f2e88d58c,
                    0x1146eb016735db19,
                ])),
                Felt::new(BigInteger256([
                    0xbf847b3430c558f8,
                    0xe6b45c44f391b0c6,
                    0x9fb7219ecae153d0,
                    0x080c76886c2eaddd,
                ])),
            ],
            vec![
                Felt::new(BigInteger256([
                    0x27e5a3c21fb4747f,
                    0xd22de7982151c8d9,
                    0x2fef4af593eb1a6d,
                    0x0572fa8e07936766,
                ])),
                Felt::new(BigInteger256([
                    0x388349e328d9e125,
                    0x9d7cc7900a1afe55,
                    0x9b86b3635012d878,
                    0x0c9f496a2b427b99,
                ])),
                Felt::new(BigInteger256([
                    0xbd9ad5b1ab33bf52,
                    0xf5f23b003d5c1437,
                    0x8f138ddbf4033569,
                    0x0be804a3e8eca5d4,
                ])),
            ],
            vec![
                Felt::new(BigInteger256([
                    0xe2261dcee03cccff,
                    0x82b9ee98790ecc00,
                    0x4f4baaa1f0bafc6d,
                    0x0e11def9585845cc,
                ])),
                Felt::new(BigInteger256([
                    0x7fe60e705fdd6970,
                    0xba171535d05d70a4,
                    0x7c77c5378b6afa1d,
                    0x1142b64a92ceb3fe,
                ])),
                Felt::new(BigInteger256([
                    0x1a29fb32407c54ed,
                    0x62f0b8c58fc997d5,
                    0x77c78c8af60b47ed,
                    0x04210636c8f7f46a,
                ])),
                Felt::new(BigInteger256([
                    0x9ec23e99d30cd9bf,
                    0xdc3ae2118c867413,
                    0xcf76a6329af7fe2b,
                    0x0dd1844f0dbdd4ac,
                ])),
            ],
            vec![
                Felt::new(BigInteger256([
                    0xf766b3ec2928e0d8,
                    0x8a8dda1d6f294976,
                    0x359d5d6c06c51ab0,
                    0x015a37d8fd4431bc,
                ])),
                Felt::new(BigInteger256([
                    0x99fbbbf09c12ff47,
                    0x20ca8bf5d7a31974,
                    0x47d3a9998b60acde,
                    0x018ae44370f648d4,
                ])),
                Felt::new(BigInteger256([
                    0xe8b08d129c67354d,
                    0x762a9d7046242d63,
                    0x790f9eeba9fa4586,
                    0x0a520fea9306f64a,
                ])),
                Felt::new(BigInteger256([
                    0x84f7abecc7bb0f89,
                    0x2aacf41acd69453e,
                    0x442df672d02af928,
                    0x099808e2280fadf8,
                ])),
                Felt::new(BigInteger256([
                    0xc072e429a4fc811d,
                    0x18e07a0926f54d2e,
                    0xba39516e2dffecbb,
                    0x125fcfa9764776e8,
                ])),
            ],
            vec![
                Felt::new(BigInteger256([
                    0x713abd228c332cae,
                    0xc714edb822a5c323,
                    0x16d6a94ccb3ff27c,
                    0x00a7d5760a7ed5d4,
                ])),
                Felt::new(BigInteger256([
                    0x7f14942baaeb2d09,
                    0x17d80f4708fa3080,
                    0x86d71c2abb4bd91b,
                    0x0c5bcef3479528e5,
                ])),
                Felt::new(BigInteger256([
                    0x862907495ca49bf1,
                    0x777b152c34084afb,
                    0x33bf613936fe094a,
                    0x074aaa02aa0a22fd,
                ])),
                Felt::new(BigInteger256([
                    0xe8a84c88e529fda5,
                    0x27ed66a6b41a0476,
                    0x20c76c13f85db711,
                    0x06c410f16d7d1b74,
                ])),
                Felt::new(BigInteger256([
                    0x13f8005caa113f82,
                    0xaa54fd9c05b1a1f6,
                    0xdda43aec7124db47,
                    0x0b0768f7b8e3c8c9,
                ])),
                Felt::new(BigInteger256([
                    0x63df083a8b45ca69,
                    0x90c0945bb89b617b,
                    0x494a7511b9996e5b,
                    0x07690ea47863980b,
                ])),
            ],
        ];

        let output_data = [
            [Felt::new(BigInteger256([
                0x3fc8428cce6674d1,
                0x797d5996040d4961,
                0xb8610beda36f2d01,
                0x058fe3e86ec4ec8c,
            ]))],
            [Felt::new(BigInteger256([
                0x5e340cdc522721a5,
                0x684618ee9a8515d9,
                0xc0a08fc55905930d,
                0x002d01262e2b85ad,
            ]))],
            [Felt::new(BigInteger256([
                0x9d4a813839b15efe,
                0xb7d18d622a8caf53,
                0x0c861f7b252c4a18,
                0x0e95ba082d255386,
            ]))],
            [Felt::new(BigInteger256([
                0x19abbf6423f61dd8,
                0xd64662845048c91d,
                0x441b52dbcdc8c1a4,
                0x06c994226286c1c9,
            ]))],
            [Felt::new(BigInteger256([
                0xba95d08b68ac83e4,
                0x9e99c25e694329b2,
                0x4119d4d81fd88970,
                0x04a8d17f384a05b8,
            ]))],
            [Felt::new(BigInteger256([
                0xcd754b1cb2d1772d,
                0x20f0d4c74d6756ee,
                0xb9f607343cd5d500,
                0x061450305a286429,
            ]))],
            [Felt::new(BigInteger256([
                0x482f63af39ef5255,
                0x85421d04f9331dac,
                0x38eaebbf3ede3d81,
                0x019e183c48993b51,
            ]))],
            [Felt::new(BigInteger256([
                0xea540546b5eddec0,
                0x4e2c34cfaa3ec64e,
                0xcbd1ca0cc9365352,
                0x10938334218773e2,
            ]))],
            [Felt::new(BigInteger256([
                0x4a8966e6902a4527,
                0x4080b9e33844cf74,
                0x7d6a16946face2ba,
                0x02ff8bce36f4ec4f,
            ]))],
            [Felt::new(BigInteger256([
                0x0a8b4068b92f6b51,
                0xa9661da2a2e4e26f,
                0xfd979eca2fe4ef2a,
                0x0ebb52a0b33d5e2d,
            ]))],
        ];

        for (input, expected) in input_data.iter().zip(output_data) {
            assert_eq!(expected, AnemoiHash::hash_field(input).to_elements());
        }
    }

    #[test]
    fn test_anemoi_hash_bytes() {
        // Generated from https://github.com/anemoi-hash/anemoi-hash/
        let input_data = [
            vec![Felt::zero(); 12],
            vec![Felt::one(); 12],
            vec![
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
            ],
            vec![
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
            ],
        ];

        let output_data = [
            [Felt::new(BigInteger256([
                0x3fc8428cce6674d1,
                0x797d5996040d4961,
                0xb8610beda36f2d01,
                0x058fe3e86ec4ec8c,
            ]))],
            [Felt::new(BigInteger256([
                0x5e340cdc522721a5,
                0x684618ee9a8515d9,
                0xc0a08fc55905930d,
                0x002d01262e2b85ad,
            ]))],
            [Felt::new(BigInteger256([
                0x9d4a813839b15efe,
                0xb7d18d622a8caf53,
                0x0c861f7b252c4a18,
                0x0e95ba082d255386,
            ]))],
            [Felt::new(BigInteger256([
                0x19abbf6423f61dd8,
                0xd64662845048c91d,
                0x441b52dbcdc8c1a4,
                0x06c994226286c1c9,
            ]))],
        ];

        // The inputs can all be represented with at least 1 byte less than the field size,
        // hence computing the Anemoi hash digest from the byte sequence yields the same
        // result as treating the inputs as field elements.
        for (input, expected) in input_data.iter().zip(output_data) {
            let mut bytes = [0u8; 372];
            bytes[0..31].copy_from_slice(&to_bytes!(input[0]).unwrap()[0..31]);
            bytes[31..62].copy_from_slice(&to_bytes!(input[1]).unwrap()[0..31]);
            bytes[62..93].copy_from_slice(&to_bytes!(input[2]).unwrap()[0..31]);
            bytes[93..124].copy_from_slice(&to_bytes!(input[3]).unwrap()[0..31]);
            bytes[124..155].copy_from_slice(&to_bytes!(input[4]).unwrap()[0..31]);
            bytes[155..186].copy_from_slice(&to_bytes!(input[5]).unwrap()[0..31]);
            bytes[186..217].copy_from_slice(&to_bytes!(input[6]).unwrap()[0..31]);
            bytes[217..248].copy_from_slice(&to_bytes!(input[7]).unwrap()[0..31]);
            bytes[248..279].copy_from_slice(&to_bytes!(input[8]).unwrap()[0..31]);
            bytes[279..310].copy_from_slice(&to_bytes!(input[9]).unwrap()[0..31]);
            bytes[310..341].copy_from_slice(&to_bytes!(input[10]).unwrap()[0..31]);
            bytes[341..372].copy_from_slice(&to_bytes!(input[11]).unwrap()[0..31]);

            assert_eq!(expected, AnemoiHash::hash(&bytes).to_elements());
        }
    }

    #[test]
    fn test_anemoi_jive() {
        // Generated from https://github.com/anemoi-hash/anemoi-hash/
        let input_data = [
            vec![
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
            ],
            vec![
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
            ],
            vec![
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
            ],
            vec![
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
            ],
            vec![
                Felt::new(BigInteger256([
                    0x9a15332d3ab1b345,
                    0x6af55a2b36d46ba7,
                    0xb3d516c0f31ddae5,
                    0x0130e66e7d52a089,
                ])),
                Felt::new(BigInteger256([
                    0xd9ca616b014c5f9a,
                    0x2f926306b79854ac,
                    0x67c64975c47fab2a,
                    0x07769f9fc2e64028,
                ])),
                Felt::new(BigInteger256([
                    0xaf6cb35ef7fca18e,
                    0x446718ca10f891eb,
                    0x4354eeb398e76a4a,
                    0x0c554047b64b8255,
                ])),
                Felt::new(BigInteger256([
                    0x3d03ea7fb0df6ef2,
                    0xac4a502101d96ff5,
                    0xe44729bb20f7573d,
                    0x0d25a2ad90bfac0d,
                ])),
                Felt::new(BigInteger256([
                    0xe3d22f916f259330,
                    0xfd974b99a45604af,
                    0xc1d6635f91d37bd9,
                    0x113920e49eecfd63,
                ])),
                Felt::new(BigInteger256([
                    0x164650b921ef63b2,
                    0x66c8ec5800b5c6fb,
                    0xc76263fa6ecaf79f,
                    0x03bf1ea65784aed7,
                ])),
                Felt::new(BigInteger256([
                    0xe61596b37c293c3c,
                    0xa939763e2d2704a2,
                    0xe1f851a450bf2af2,
                    0x081b5859eab46b19,
                ])),
                Felt::new(BigInteger256([
                    0x4ddf47ded1e15741,
                    0xa66900045d5c5ed3,
                    0x7a127c45ae2216c0,
                    0x02def4b44416662e,
                ])),
                Felt::new(BigInteger256([
                    0x6a80257d5d562924,
                    0xd4a037a34cfe5389,
                    0x8b740d849f1b1369,
                    0x071bb31bae3a8566,
                ])),
                Felt::new(BigInteger256([
                    0x84ea071ea97dab9f,
                    0x0028e6bf3fdb9014,
                    0x56cb621bcd2b33b0,
                    0x030309427d3fe803,
                ])),
                Felt::new(BigInteger256([
                    0x91907902e0c2d306,
                    0x678795614066299c,
                    0xd95c18c3cee7bc6d,
                    0x06c340c30d596ea5,
                ])),
                Felt::new(BigInteger256([
                    0x93cdd922697d4b15,
                    0x31ef49f7f2580592,
                    0x1b41e7e9e2f9da07,
                    0x05b5b5553b725163,
                ])),
            ],
            vec![
                Felt::new(BigInteger256([
                    0xb6679c4ef0efbf0c,
                    0x5e9780918a9879c0,
                    0x5c6dc92d8dc17517,
                    0x09c8228e1da683f2,
                ])),
                Felt::new(BigInteger256([
                    0x4c35496076a1c838,
                    0x9f4c2858d0639a9a,
                    0x4705eb2a3be68906,
                    0x0d3aac48816b00a6,
                ])),
                Felt::new(BigInteger256([
                    0x056fd1481e090ed2,
                    0x028f59206de9936b,
                    0xdafa9d73bcaf5b04,
                    0x0b84681c2eaab9bd,
                ])),
                Felt::new(BigInteger256([
                    0x65ccda37c68c9633,
                    0x4ffdb774458d6726,
                    0xd71bcb308adff1f7,
                    0x105f9f368f385b6d,
                ])),
                Felt::new(BigInteger256([
                    0xde9dfba586da252a,
                    0x09f0b36274dc8044,
                    0x9637c6cc8caf6b4a,
                    0x0a8f7a7f4385e23d,
                ])),
                Felt::new(BigInteger256([
                    0x5c6002bf66eaf583,
                    0xcbbf956217e82f1e,
                    0x51106e6aca795a13,
                    0x04e4b0b7dbbe6b49,
                ])),
                Felt::new(BigInteger256([
                    0x1caf2f4a4714ace2,
                    0x51a6b88d9d4ad297,
                    0xc4e1a15e41c065bc,
                    0x08edadda569d2b08,
                ])),
                Felt::new(BigInteger256([
                    0x5899419e8011b610,
                    0xa3f5dabf749d10ac,
                    0x82a7f2545fe2bbbb,
                    0x005ea3cdf6609275,
                ])),
                Felt::new(BigInteger256([
                    0xffa3c667738f4f27,
                    0xe17cd1a78fad7653,
                    0xf34aa2f2c947e050,
                    0x10f5d1cd845586a7,
                ])),
                Felt::new(BigInteger256([
                    0x4ae95757548d46e5,
                    0x4523568a361b5091,
                    0xc7deee94266e6d69,
                    0x0e75e0f0ca9375ff,
                ])),
                Felt::new(BigInteger256([
                    0x0ada3ffb09567928,
                    0xb6cc1acde9638592,
                    0x05398bf0a6f47c1f,
                    0x038d8dbb89d4e9ec,
                ])),
                Felt::new(BigInteger256([
                    0x4ac844cc17880d19,
                    0xe25bc20a200eaae3,
                    0x1183434a4ab97027,
                    0x03f267fe9ea6f3a9,
                ])),
            ],
            vec![
                Felt::new(BigInteger256([
                    0x2325d793166c800a,
                    0x99d5a901e7d83a07,
                    0x16d1611c78d58223,
                    0x035ac7c241286beb,
                ])),
                Felt::new(BigInteger256([
                    0xeaecbb860a9308cc,
                    0x50a84112dfc5adeb,
                    0x536e91073221cefd,
                    0x0e6323445e25222b,
                ])),
                Felt::new(BigInteger256([
                    0x86466254331567c4,
                    0xc874f0c1a0053d2f,
                    0x010c49c60e638119,
                    0x032f61e7c80386e1,
                ])),
                Felt::new(BigInteger256([
                    0xb4182cc3f4db5557,
                    0xa47ff4ce5b64bb8e,
                    0x36ee0d64f818b877,
                    0x11d78a8dd6ce400c,
                ])),
                Felt::new(BigInteger256([
                    0xf35fbbb099261e98,
                    0xe09abab8314d0f06,
                    0x9f87f69603634130,
                    0x0a3401108d358312,
                ])),
                Felt::new(BigInteger256([
                    0x40a40a122f1c29cb,
                    0xa00d9b0893d0a818,
                    0x296edb2feaf13e94,
                    0x0958c4f302dc060f,
                ])),
                Felt::new(BigInteger256([
                    0x9314cf5feb3c6130,
                    0x87f6550d2841667b,
                    0xb52f1b9ca0ba2be5,
                    0x0a3110e241654f13,
                ])),
                Felt::new(BigInteger256([
                    0xf9d329585a559569,
                    0x4ad890a9cb247a07,
                    0x99f8123870b7e45b,
                    0x01e4967a18a23437,
                ])),
                Felt::new(BigInteger256([
                    0x48d4bd8987e940a3,
                    0xf1d24b6316c017b7,
                    0x5d6c953ef796349b,
                    0x090c5f96aa717c0f,
                ])),
                Felt::new(BigInteger256([
                    0x860d9ed06a82f949,
                    0xe1e9258af6b3b384,
                    0xc811b53afcb519f8,
                    0x0e35d0241c780ec3,
                ])),
                Felt::new(BigInteger256([
                    0xa7bce5bbc5d64dc7,
                    0xcbcdeec26a8d275f,
                    0xf76926379ee6c2f6,
                    0x09f829e177c41b22,
                ])),
                Felt::new(BigInteger256([
                    0x557797d9dbf6eba0,
                    0xe6c47e72bb0103f5,
                    0xa0b45ed63f02749b,
                    0x0d4ba8cf2d5378a7,
                ])),
            ],
            vec![
                Felt::new(BigInteger256([
                    0x78b6fe83b23380f3,
                    0xae5f59f67f2aefa8,
                    0xfc5be1f55e392393,
                    0x0a7e6d8d0e11d4f7,
                ])),
                Felt::new(BigInteger256([
                    0x4925abaa6706fb66,
                    0xede3ae70982542e7,
                    0xa498436ca0a2c983,
                    0x11fee9df2caec0f6,
                ])),
                Felt::new(BigInteger256([
                    0x099e821e95aaafd4,
                    0x9ffeb3819e01d462,
                    0x5630265e3001012f,
                    0x0ffd784c42d39093,
                ])),
                Felt::new(BigInteger256([
                    0xee92dae751746f2d,
                    0x949195042aade54b,
                    0x861249871dfaced5,
                    0x02d2cac5f37c6c6a,
                ])),
                Felt::new(BigInteger256([
                    0x7ad03a99e8ed6478,
                    0x1f518db515385edc,
                    0xb2fb865c7f562a62,
                    0x07fe8ef90a455634,
                ])),
                Felt::new(BigInteger256([
                    0xb35595e746d1307f,
                    0x5bc047ac20678699,
                    0x833b7e3fe1d11e50,
                    0x0d7953151b128332,
                ])),
                Felt::new(BigInteger256([
                    0xe46d9ff85c74f560,
                    0xdc2041173f822896,
                    0xfb16990645bed9b9,
                    0x000f26729f8194fb,
                ])),
                Felt::new(BigInteger256([
                    0xeabad0a062ed4490,
                    0xf78524bcd5f4d322,
                    0x9ce3c36b76962591,
                    0x0074e63682ff8ec0,
                ])),
                Felt::new(BigInteger256([
                    0xd01bf59402431d32,
                    0x138c1d2fdb776fd4,
                    0xc07bcad57214862d,
                    0x08f51c6dc7e63c25,
                ])),
                Felt::new(BigInteger256([
                    0xfc35cf7e09848bda,
                    0xf11ac579b65d8bae,
                    0x5b84b47001463362,
                    0x0a4fbadc7912085b,
                ])),
                Felt::new(BigInteger256([
                    0x1a67e99f0b696633,
                    0x80e1046ef68ca29c,
                    0x2f57bdf49585806c,
                    0x011ddaac7cb5453c,
                ])),
                Felt::new(BigInteger256([
                    0x43f7ead61ae01cb6,
                    0xb74d0d56a74f5d5c,
                    0xc624f0e6f90678f5,
                    0x00132893973242ec,
                ])),
            ],
            vec![
                Felt::new(BigInteger256([
                    0x26cf53a75a2a3b1c,
                    0xb6a8a8406d45055f,
                    0x21eca19f636bd09f,
                    0x06a403bb68be9a36,
                ])),
                Felt::new(BigInteger256([
                    0xca7d0c49bcdc1544,
                    0x862a3958b7d845d2,
                    0xe26c09151cc83370,
                    0x072d0533fb630075,
                ])),
                Felt::new(BigInteger256([
                    0xcb715bf2407ac8b5,
                    0xc136cd495d0687b5,
                    0xe839c9d44523a908,
                    0x0861547b822431e8,
                ])),
                Felt::new(BigInteger256([
                    0x38b123f503276c5f,
                    0xe86ea513263c7b7f,
                    0x44cffcf74c6f713f,
                    0x07fadf2e56208481,
                ])),
                Felt::new(BigInteger256([
                    0x32a3c54e15d7025a,
                    0x285c9e2f126beb78,
                    0x243017a0569a796f,
                    0x007e80993941271a,
                ])),
                Felt::new(BigInteger256([
                    0x661937b166b081fb,
                    0xa8ca7dfefd76acdf,
                    0x20a8d44efe8b31cc,
                    0x03ef9fa1a1a56f57,
                ])),
                Felt::new(BigInteger256([
                    0x45ff3ed1445847a2,
                    0xf3bb2a9c33d381ad,
                    0x3eaab07c354459a0,
                    0x0e8846f0384251f0,
                ])),
                Felt::new(BigInteger256([
                    0xb9820209ba50429f,
                    0x1422ef0ebbab6507,
                    0x5229f449de4674cc,
                    0x0d2a5fe57f9bb6d0,
                ])),
                Felt::new(BigInteger256([
                    0x59b19de3fb52d211,
                    0x99a7ddfadb0c4487,
                    0xb9f9f7f4e4fae358,
                    0x0f9580d6963f3cd6,
                ])),
                Felt::new(BigInteger256([
                    0x35f0190b8c7b8ea0,
                    0x53858bed901daeb8,
                    0xc26140e4c7ada13f,
                    0x029e1877d907f1bf,
                ])),
                Felt::new(BigInteger256([
                    0xde6d96129dd73e95,
                    0xc609dbc296505aeb,
                    0xabe084632deff209,
                    0x0fa3cdbbe659e50e,
                ])),
                Felt::new(BigInteger256([
                    0xa2b56634682616d8,
                    0x923ca508506a9b8f,
                    0x7a221cd7fa7486f8,
                    0x0e7f032df39bdff9,
                ])),
            ],
            vec![
                Felt::new(BigInteger256([
                    0x2c2ea1dd135619fd,
                    0x18e9b144b082afb6,
                    0x03b6e366cb125f79,
                    0x01086c14cec74295,
                ])),
                Felt::new(BigInteger256([
                    0x9199435f3782e7c5,
                    0x6d7bfbc5be4e07ce,
                    0x6b1820a969da5bea,
                    0x0c83970397d5ae4b,
                ])),
                Felt::new(BigInteger256([
                    0xb3d574116a355c24,
                    0x16ddb1cb62f18fa3,
                    0x104de33684d29348,
                    0x01f852bf162623fc,
                ])),
                Felt::new(BigInteger256([
                    0x42d42e0a6cfadc58,
                    0x232df57eafc2ca56,
                    0xc75f4c9b67c53253,
                    0x1226c2c31087cac3,
                ])),
                Felt::new(BigInteger256([
                    0x6917b4fd1523c80f,
                    0x0acbc0d918588a65,
                    0xc5f264c11ef15e65,
                    0x0b24043537ba1759,
                ])),
                Felt::new(BigInteger256([
                    0x3f2c4eaa624a196a,
                    0x6d614c1fb4558110,
                    0x85e660a331fa80a5,
                    0x0ed4bed67dbd42f0,
                ])),
                Felt::new(BigInteger256([
                    0x38ee0df8bd41acd1,
                    0xa32243d609464450,
                    0x342dc63b8071cce5,
                    0x032150d1383d91c0,
                ])),
                Felt::new(BigInteger256([
                    0xc1d84321b8b5b75f,
                    0x3ebdeb0d74e3d9c3,
                    0xb9aa4f50fab2d392,
                    0x01238b8b49d3d289,
                ])),
                Felt::new(BigInteger256([
                    0x6fe5cc1c1ab5fecf,
                    0x9032a0ec8b1f38d2,
                    0x356c4a15d21832fc,
                    0x05c14559e3294fe5,
                ])),
                Felt::new(BigInteger256([
                    0x6e680420294a9bae,
                    0x915cee5d35f8e50b,
                    0x28361566f0bd743b,
                    0x00b0092974957403,
                ])),
                Felt::new(BigInteger256([
                    0x66a89d7137e65728,
                    0x93ab81e877eccb41,
                    0x3dae25313f5c51b8,
                    0x0c422b8ef3aca6d2,
                ])),
                Felt::new(BigInteger256([
                    0xb5e6bea0bcb6efc9,
                    0xf5dfa9e75b702e4d,
                    0x17d60529ac51c4e4,
                    0x0b2629ee8673f360,
                ])),
            ],
        ];

        let output_data = [
            [
                Felt::new(BigInteger256([
                    0xf29c3dfd1aaa3958,
                    0xe7d91c915f8d63ea,
                    0x88a92c0ce095573d,
                    0x11c1552b657f4728,
                ])),
                Felt::new(BigInteger256([
                    0x0abdfaf09cd6dc92,
                    0x837facd4b96deb11,
                    0x464a68a390d60bd7,
                    0x0f7711ea7f9de0b1,
                ])),
                Felt::new(BigInteger256([
                    0x51a198a4053fe741,
                    0xfe93e239c11c4738,
                    0x155bd9c0996fb36c,
                    0x0a898c3a0f5dca20,
                ])),
                Felt::new(BigInteger256([
                    0x2088b51730655ad0,
                    0xb9833d23cc564e20,
                    0xd18fa4fa6b41ad52,
                    0x08b5a5ed09631993,
                ])),
                Felt::new(BigInteger256([
                    0x0a7bda654257f86f,
                    0x794ce55aad2f73c2,
                    0x0d4e08c297822f74,
                    0x08408d4c36383887,
                ])),
                Felt::new(BigInteger256([
                    0x2979802819377055,
                    0x83c07d6fd7ab28f5,
                    0xb1b501d42186416a,
                    0x0f4e826d5abbb99d,
                ])),
            ],
            [
                Felt::new(BigInteger256([
                    0x7b5e6a5d4429b540,
                    0x71e81664e0cec6ee,
                    0xf04e64abbe344256,
                    0x08ddac20519fcf23,
                ])),
                Felt::new(BigInteger256([
                    0xe8d9d5152e7d1ec2,
                    0xce0e39cd95734a63,
                    0xcfeeeb8095e54815,
                    0x05b03ea289f15a9e,
                ])),
                Felt::new(BigInteger256([
                    0xd2c793f5d5243702,
                    0x603f44aae26ba60d,
                    0xfb184265e8eb4d05,
                    0x12169e78032ab829,
                ])),
                Felt::new(BigInteger256([
                    0x79238a7662b6865c,
                    0x920e648dc8b19865,
                    0x517f410c69e32ec2,
                    0x075a9995f387f14e,
                ])),
                Felt::new(BigInteger256([
                    0x66c1825d4120e828,
                    0x19f67a742640bd04,
                    0x91de2a80f9d6a84a,
                    0x0c0c0e8bde7464d5,
                ])),
                Felt::new(BigInteger256([
                    0x1abe21553604a8da,
                    0xc3aac4991b7a3add,
                    0x1ce0726ee5b213bd,
                    0x01d70342cb7ebaa0,
                ])),
            ],
            [
                Felt::new(BigInteger256([
                    0xcd3b37808fb699a4,
                    0x06992319687f5538,
                    0xf53ba391b130cc0a,
                    0x0fe8204cc678f17b,
                ])),
                Felt::new(BigInteger256([
                    0x7fa72919326fd5ab,
                    0xbba0e237b3f4bc9e,
                    0xf9732acdb602b829,
                    0x111d9be0360bb1f8,
                ])),
                Felt::new(BigInteger256([
                    0x9f69d074058ec5c0,
                    0x8ad5cff9bf088c90,
                    0xfcd429cf8249017f,
                    0x03dda715d3ff8a9d,
                ])),
                Felt::new(BigInteger256([
                    0x5f7c8be5b3e6691f,
                    0x2d30ac7419f77a2f,
                    0xb98174531c4250a3,
                    0x06a2380da061fbf1,
                ])),
                Felt::new(BigInteger256([
                    0x2598909a782e65de,
                    0x016d6e876d8310b2,
                    0xe4c5307cc1c18ae2,
                    0x093225c1dd44eb30,
                ])),
                Felt::new(BigInteger256([
                    0x9f81bb2413d9debf,
                    0xe0e83c911f150721,
                    0x589e6a3434b6f430,
                    0x0fede1bcde8eecca,
                ])),
            ],
            [
                Felt::new(BigInteger256([
                    0x179a284bf234c816,
                    0x430bb23698d83596,
                    0xcbe62530e61e8003,
                    0x11aeb6d2d8e357f4,
                ])),
                Felt::new(BigInteger256([
                    0x3e00247271283e17,
                    0x76c688f20a002c44,
                    0x31e2e2172d1c7349,
                    0x0d91d756a7772dda,
                ])),
                Felt::new(BigInteger256([
                    0xc1903648faeb5051,
                    0xc22dcbea58b17455,
                    0xec477cb0fcb4722f,
                    0x0c403302eef4e2c7,
                ])),
                Felt::new(BigInteger256([
                    0x9b5ecc6e5c5082bd,
                    0x3dd235967bf5979d,
                    0x9c278764cad335b7,
                    0x05f312b1a202df1a,
                ])),
                Felt::new(BigInteger256([
                    0xda7fda94a5a50b18,
                    0xce822d55c787b327,
                    0xd8443e6bdfd387dd,
                    0x04bf60a4ec0fbe80,
                ])),
                Felt::new(BigInteger256([
                    0x435498df4a87995c,
                    0xeeb6a2243d49ae8d,
                    0x3d92eb9d0ab9ffec,
                    0x088853ccdd53c74e,
                ])),
            ],
            [
                Felt::new(BigInteger256([
                    0x7c6d6a6d294f019d,
                    0xf5a6fbb7bb4e6ad4,
                    0x5ae61355a84e8e4f,
                    0x0b36945ecf5e1df1,
                ])),
                Felt::new(BigInteger256([
                    0x83d143516cbfbf9c,
                    0xd1ff2e4d43308e20,
                    0x7b30683e1ea68560,
                    0x0f5ebfa7bec8f96a,
                ])),
                Felt::new(BigInteger256([
                    0x07cc5e093791c4f2,
                    0x34669af05a469976,
                    0x9f19749792ef204b,
                    0x0a74ad3892809b32,
                ])),
                Felt::new(BigInteger256([
                    0x735e9c955aecb294,
                    0xda05dd3bec1e1161,
                    0xf4f73b4cad3f07f6,
                    0x0ec519ddabf7ae1f,
                ])),
                Felt::new(BigInteger256([
                    0x1197451abafba0ea,
                    0x398114ee0b4fb8c5,
                    0xdd242161f3fda67a,
                    0x0124e4a06d7c3d9f,
                ])),
                Felt::new(BigInteger256([
                    0x6e5c0cd8ad05ca21,
                    0xafa034f596aad86b,
                    0x062ee933d311f748,
                    0x0833d21e8d7db5a3,
                ])),
            ],
            [
                Felt::new(BigInteger256([
                    0x052eb39502863709,
                    0xa5f6f72e2f038a02,
                    0xc48fda881450be4a,
                    0x006a03ac8cbe4833,
                ])),
                Felt::new(BigInteger256([
                    0xc7f9f1771192fe76,
                    0x01e3c325f9de4d20,
                    0xa3e5bddb35d46964,
                    0x0966cb180a416478,
                ])),
                Felt::new(BigInteger256([
                    0xd68c893394f5d13b,
                    0xcd2903be30531e3e,
                    0x5575e0b67afb8207,
                    0x00412be626331e4a,
                ])),
                Felt::new(BigInteger256([
                    0x050b9437e9812a8b,
                    0xf9116ed93cfbe52d,
                    0x9ef98a1d9c04c9c9,
                    0x11a1bba9673987c5,
                ])),
                Felt::new(BigInteger256([
                    0x895fd047576e30ae,
                    0xef48116d6f60c6c1,
                    0x53b7511c51b5502f,
                    0x0298973e00483c85,
                ])),
                Felt::new(BigInteger256([
                    0x960609e4ddff8137,
                    0x465af9809812a692,
                    0xfd91665d60793c6b,
                    0x11b272ee373caba1,
                ])),
            ],
            [
                Felt::new(BigInteger256([
                    0xadbe02a7641b2406,
                    0x599c3ba27a174b2c,
                    0x5ae5a3ffe75e612b,
                    0x0d5b756e143f22c8,
                ])),
                Felt::new(BigInteger256([
                    0x6c0f9c81b139aa15,
                    0xd43fe4912f1bcd97,
                    0x0b8de973a9e902eb,
                    0x00d9e4ba03820d2e,
                ])),
                Felt::new(BigInteger256([
                    0x0339a607f6581693,
                    0xa392d0a1bd468067,
                    0x74cc325c3efd9dae,
                    0x00aca705bc2b7f9f,
                ])),
                Felt::new(BigInteger256([
                    0x5a74a4293c16b4f2,
                    0xbc91f313e0981262,
                    0x9aa213dfec0dc4e9,
                    0x0d9fac4cd27c8762,
                ])),
                Felt::new(BigInteger256([
                    0x14fce857211dc8b4,
                    0xc5f2e42c58a40e4a,
                    0x48684c31d7abd349,
                    0x059d14e6802446ec,
                ])),
                Felt::new(BigInteger256([
                    0xa5934bfa96e396d9,
                    0x02362c0f73e45971,
                    0x1ed672d2dfbbc953,
                    0x0ceb08fcc600cb37,
                ])),
            ],
            [
                Felt::new(BigInteger256([
                    0xf31bc96d4b489d59,
                    0x93b7f75f7fb03c0e,
                    0x8a05600e2480f4b1,
                    0x0a995a89282cbfa1,
                ])),
                Felt::new(BigInteger256([
                    0x539706018e70e33e,
                    0x5aab9970c79e62e6,
                    0xc13189dc2385da18,
                    0x00d92cd6b6f4f137,
                ])),
                Felt::new(BigInteger256([
                    0xd7782de2988e1293,
                    0x17968203adb2623e,
                    0x9d8fca9014165521,
                    0x07301f04eb47af61,
                ])),
                Felt::new(BigInteger256([
                    0xf3c3ac3a34d73a1e,
                    0x2d9762a86fffa521,
                    0x73c3467d3107def4,
                    0x0a180b68bae20f6f,
                ])),
                Felt::new(BigInteger256([
                    0xde562fe5b298148d,
                    0x744869245b6893e2,
                    0x210381cb706e029f,
                    0x0d1ff148f191eaa1,
                ])),
                Felt::new(BigInteger256([
                    0x3b5fca94ef69002b,
                    0x618703252077cc0c,
                    0xbfd0606606ab54a0,
                    0x11bf26bb161a78e8,
                ])),
            ],
            [
                Felt::new(BigInteger256([
                    0xbb8e635aa170b853,
                    0x73baa7b3f4297bfa,
                    0xea092970ee14aa56,
                    0x059c0dfd4b0b2046,
                ])),
                Felt::new(BigInteger256([
                    0x61e09d871598018d,
                    0x5376fc03e6bfd0a0,
                    0x4da02b512f9c0f47,
                    0x0bb98bdc0343e258,
                ])),
                Felt::new(BigInteger256([
                    0x8cf8ec7b26c41afe,
                    0x176257424a1748c6,
                    0x1f91bd3468ec8bed,
                    0x0a3da1690f5fe01a,
                ])),
                Felt::new(BigInteger256([
                    0x469e21d731955b85,
                    0x4b813cf0fad46c30,
                    0x17c2106408f3bc91,
                    0x07b6161b585db79b,
                ])),
                Felt::new(BigInteger256([
                    0x844536f1b8f838d6,
                    0x27f435c181ca6431,
                    0xdef5f6d45109ffc0,
                    0x088f521888e081ab,
                ])),
                Felt::new(BigInteger256([
                    0x75be6d021edd1a79,
                    0xb7e43d297e64ab00,
                    0x4238aeac54623860,
                    0x03c505b3d5d45802,
                ])),
            ],
            [
                Felt::new(BigInteger256([
                    0xd7480f8d52f72cff,
                    0xa58134a4614c7045,
                    0x132634fecefbbc29,
                    0x116a37fcb76a975d,
                ])),
                Felt::new(BigInteger256([
                    0x50414b7b599c5b5a,
                    0xc3227df00dca2d1e,
                    0x053946330ae973a3,
                    0x11ad8107803d78f3,
                ])),
                Felt::new(BigInteger256([
                    0x4816b042eb237f8a,
                    0x3a2236ecdc32e657,
                    0x36fa1b8d12cb91ea,
                    0x0beb9fcb275c6075,
                ])),
                Felt::new(BigInteger256([
                    0x2e14d2b086618364,
                    0x1a43ad29c4a632a6,
                    0x075bf24d60ab2528,
                    0x0bab936319b67950,
                ])),
                Felt::new(BigInteger256([
                    0xbd38510aa4a25671,
                    0x5175e0c4756bb4cc,
                    0x28be16c89051f1af,
                    0x12913f1a7e2387d0,
                ])),
                Felt::new(BigInteger256([
                    0x2e3a4f5166363515,
                    0xb7f7debe9770fe1a,
                    0x351275a6ef6fda19,
                    0x0ae34cf6e9120caf,
                ])),
            ],
        ];

        for (input, expected) in input_data.iter().zip(output_data) {
            assert_eq!(expected.to_vec(), AnemoiHash::compress(input));
        }

        for (input, expected) in input_data.iter().zip(output_data) {
            assert_eq!(expected.to_vec(), AnemoiHash::compress_k(input, 2));
        }

        let input_data = [
            vec![
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
            ],
            vec![
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
            ],
            vec![
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
            ],
            vec![
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::one(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
                Felt::zero(),
            ],
            vec![
                Felt::new(BigInteger256([
                    0x085122cda3c0240d,
                    0x08118062f024e781,
                    0xd75717308a43daa4,
                    0x0fc6ad7ac671579a,
                ])),
                Felt::new(BigInteger256([
                    0x46033ae59d107016,
                    0xa97507b6e1a149f6,
                    0xf6d16600b53725a4,
                    0x00aadabd60b45dc6,
                ])),
                Felt::new(BigInteger256([
                    0x6e1c240bdacffd91,
                    0xa7a11e2c6df9de4d,
                    0xcfd0dbade48195d6,
                    0x0e3d68d069af7b37,
                ])),
                Felt::new(BigInteger256([
                    0xacb9245e1a5ab2a0,
                    0x00511afd8e820e2b,
                    0x00e3ac3de30ece56,
                    0x042133026cc4f231,
                ])),
                Felt::new(BigInteger256([
                    0x9faa36690ff3b5a5,
                    0x863725f276e62fe8,
                    0x01d389ecc653b1b5,
                    0x0faca4ed7daefce0,
                ])),
                Felt::new(BigInteger256([
                    0xca05076ecee12b82,
                    0x6191c792de1c5c34,
                    0xdb538aa67a2ce0ff,
                    0x0451a3fc9003bae5,
                ])),
                Felt::new(BigInteger256([
                    0x8c07e11bd19ca2f6,
                    0xae182eb9059985a0,
                    0x018b994423946ab4,
                    0x0f5fcf12b75223ba,
                ])),
                Felt::new(BigInteger256([
                    0x36633589bc0597ae,
                    0x3a2374f48a551499,
                    0xe8706d3c32cbb5b1,
                    0x088db1c6d8244c9a,
                ])),
                Felt::new(BigInteger256([
                    0x26bcb33223275180,
                    0x10da07a6ee3a08bb,
                    0xf0de24f262d9273a,
                    0x0280126e53b765d4,
                ])),
                Felt::new(BigInteger256([
                    0x88c2276f5ee60139,
                    0x0d197d08c5adc7c8,
                    0x0f2b3097f584b0c9,
                    0x00654514978d1640,
                ])),
                Felt::new(BigInteger256([
                    0x9e48712160c0cc7c,
                    0x7e1fed7f01c0a87b,
                    0x27e88b79d4cc5f36,
                    0x0346ad9ec885813f,
                ])),
                Felt::new(BigInteger256([
                    0xbb00485644bcebec,
                    0x8a9bf538474fc7ec,
                    0x5a95f6c1b8b4c8f2,
                    0x078b23955ac55242,
                ])),
            ],
            vec![
                Felt::new(BigInteger256([
                    0x0e62a9ac1bf24e58,
                    0x10d18d68a4fe9ec9,
                    0xcd84bdea96f66074,
                    0x02bef0d6af93d086,
                ])),
                Felt::new(BigInteger256([
                    0x48120ccf7f66069a,
                    0x12fbccde7d060647,
                    0x61e50b998d6ccfe3,
                    0x0b3d783bcf850d2e,
                ])),
                Felt::new(BigInteger256([
                    0x0cb233a6ebfc4d59,
                    0xe9b7552a6712102e,
                    0x010d61d2e7a048cc,
                    0x10121557249cac4e,
                ])),
                Felt::new(BigInteger256([
                    0x9aa8f0b96065fbd4,
                    0xdeac6894997ef8e2,
                    0x0fc4efe1264ebe22,
                    0x0439f3e5d0c3fe8a,
                ])),
                Felt::new(BigInteger256([
                    0xf669a7cd93764030,
                    0xd8ec205312aeb819,
                    0xdb31917f167d31a9,
                    0x005503d5c5616fa2,
                ])),
                Felt::new(BigInteger256([
                    0x24895494dbfa0782,
                    0x89c6142c3e7546ca,
                    0xf7696e22dd0043ff,
                    0x04a34554c6a1719f,
                ])),
                Felt::new(BigInteger256([
                    0x3611d92cdb7eb316,
                    0xb499039594c76bfe,
                    0x805b59a58e63d15f,
                    0x01396f3f299fd73d,
                ])),
                Felt::new(BigInteger256([
                    0x6fd84cb8de7d04c3,
                    0x9c28dac3e82824c0,
                    0xd12e43778dd03e45,
                    0x0e84d6a0aa716fa6,
                ])),
                Felt::new(BigInteger256([
                    0xb607d7090d25f3aa,
                    0x204af57d2574cfde,
                    0x307f67a90115fa27,
                    0x10ddc7138b032134,
                ])),
                Felt::new(BigInteger256([
                    0xe225efafd608ab90,
                    0x344af5043c387585,
                    0x19441cc54a474fe6,
                    0x108415ef695e7a1f,
                ])),
                Felt::new(BigInteger256([
                    0xf75d520ccef58f7c,
                    0xf687c234141d7b7f,
                    0x579d43bbe1a1b109,
                    0x0d17c3e18ad6367d,
                ])),
                Felt::new(BigInteger256([
                    0x29b4f80861326601,
                    0xce422159835c2622,
                    0x94236e52c99f3727,
                    0x0f230c896e00be00,
                ])),
            ],
            vec![
                Felt::new(BigInteger256([
                    0x965237cfc693b909,
                    0x8a0d18427c086907,
                    0x5ab0169cdd0d3990,
                    0x0999077ceae9e1f3,
                ])),
                Felt::new(BigInteger256([
                    0x8272fa39bc9816af,
                    0x5763b212f1ca5805,
                    0x0ea835aca62458c8,
                    0x0eff64661e580a22,
                ])),
                Felt::new(BigInteger256([
                    0x8ee001d91473e563,
                    0xd29f2f97d4b28c85,
                    0xe58277132ee9bfbb,
                    0x04c1593e28c5338a,
                ])),
                Felt::new(BigInteger256([
                    0xe9f1c67c1407463b,
                    0x1d7ddd2497e07fb8,
                    0x8baff7a0ceb9be5f,
                    0x0b01960324691ecf,
                ])),
                Felt::new(BigInteger256([
                    0xfe7fc7f0e531dd1c,
                    0x293e12ba786d0719,
                    0x918fcc7808858c46,
                    0x03e6be0a260732a4,
                ])),
                Felt::new(BigInteger256([
                    0x77a1d3b620ea2eb9,
                    0x8365b5c56cad47d5,
                    0xe9b611b7f6401289,
                    0x129d22d0d1d46952,
                ])),
                Felt::new(BigInteger256([
                    0x9becd1008a6f88a1,
                    0xbfd5c77574b69dfe,
                    0x8b29f2758448f069,
                    0x00018b7d4533c1d4,
                ])),
                Felt::new(BigInteger256([
                    0x7008a64e81207609,
                    0x98f02d566146088e,
                    0xdc86f4a3f30eb010,
                    0x0bfc065db87fd584,
                ])),
                Felt::new(BigInteger256([
                    0x8ab2ca7b13c5a675,
                    0x8501e757e2800545,
                    0x295f0e465c251117,
                    0x051c28577c013360,
                ])),
                Felt::new(BigInteger256([
                    0xda16201a2661ca72,
                    0x12b94e9f7602f1d3,
                    0x4fd8b9b22ac29aa2,
                    0x10a6c3c8497b8248,
                ])),
                Felt::new(BigInteger256([
                    0x45d6d1567ed124c1,
                    0xa5f2fab8e8d3103b,
                    0x6800562b19e608dc,
                    0x01acca90d798d18b,
                ])),
                Felt::new(BigInteger256([
                    0x2806e6d00eba8964,
                    0xcc0848bbd2b79bf9,
                    0xd60eda7e83f2b7e7,
                    0x07170824fe6e7078,
                ])),
            ],
            vec![
                Felt::new(BigInteger256([
                    0x9b90a25a7f74c5a8,
                    0x7eacb5b0e9ca0ba6,
                    0xaef09cc9cd297f34,
                    0x0a2a66464e69c3c3,
                ])),
                Felt::new(BigInteger256([
                    0x2cb9cb504ff19937,
                    0x241859a7416ffa4c,
                    0xf8fd9a5780ac50bf,
                    0x129561bb41873f1d,
                ])),
                Felt::new(BigInteger256([
                    0x3fcfaa2befe0e3be,
                    0x6511823d89e1b4f7,
                    0x522d632c0871ae17,
                    0x0b9273a8d0ac667c,
                ])),
                Felt::new(BigInteger256([
                    0x52f50c73d800cfeb,
                    0x122fd5570cfbcaaa,
                    0x60805b025d4080bc,
                    0x09c68cc239af4929,
                ])),
                Felt::new(BigInteger256([
                    0xb4d5663b7c5a1c73,
                    0x78a780ce3d0fdaea,
                    0x713bb22060b7c71c,
                    0x017f023474999247,
                ])),
                Felt::new(BigInteger256([
                    0x755572187c4f0f48,
                    0x457433943e70a915,
                    0xadc5d9cda6265f66,
                    0x0a3ea18bdf7b73a3,
                ])),
                Felt::new(BigInteger256([
                    0xe815275badf0de72,
                    0x29a3f529f05ac164,
                    0xb01f896eacd4f3d2,
                    0x0d62f6c442aba368,
                ])),
                Felt::new(BigInteger256([
                    0xc0ab0179e235d3ca,
                    0x0c6eb1508a8f25de,
                    0xadaeb197f339d99b,
                    0x013033ed789d41a6,
                ])),
                Felt::new(BigInteger256([
                    0x8a9617efd992aea0,
                    0x3c9cec17f85f44a9,
                    0x47bd1d89c886f9f0,
                    0x0337220e93d78957,
                ])),
                Felt::new(BigInteger256([
                    0xc04a20593ec9f274,
                    0xd0f737a08ae8abbf,
                    0x48017caee39c0eb8,
                    0x072f6f311a4aec16,
                ])),
                Felt::new(BigInteger256([
                    0xd81db8534d595971,
                    0x8348aa20d653b3af,
                    0x32638b21d821b8b4,
                    0x01eaa0c9cb332066,
                ])),
                Felt::new(BigInteger256([
                    0x71a0fca01462a7d2,
                    0x32629fc71d90ff51,
                    0x208889ecc3b2638b,
                    0x0d6aa490808247d5,
                ])),
            ],
            vec![
                Felt::new(BigInteger256([
                    0x74668e1ba20c4288,
                    0x26c48a715fe7839f,
                    0x850c02484be055bc,
                    0x0114b1b2d522351f,
                ])),
                Felt::new(BigInteger256([
                    0x7820af77057b5689,
                    0xff77af9d713fb459,
                    0xd3b460de55d2fbe8,
                    0x07873e76d920e28b,
                ])),
                Felt::new(BigInteger256([
                    0x6bd487259f5e3263,
                    0x6105165d4ba024cd,
                    0x0ce469582ede0a5d,
                    0x0537efa1cc3d5dd3,
                ])),
                Felt::new(BigInteger256([
                    0x963a9af8a137b0e9,
                    0x8d1483fbe62edfc7,
                    0x355618bc8cbae462,
                    0x123572ddd27b657f,
                ])),
                Felt::new(BigInteger256([
                    0x166ba27954f9cdeb,
                    0x5e7491755ef100fe,
                    0x297552e83f4b07ae,
                    0x0cb8656f5c6e846c,
                ])),
                Felt::new(BigInteger256([
                    0x579f1c10afa23b8b,
                    0x9b50387593dcba7a,
                    0x9a474c9f97ba9306,
                    0x0f29475b64b9fefe,
                ])),
                Felt::new(BigInteger256([
                    0x76cf003c4dd99d4b,
                    0xf56396638936954e,
                    0xd26833c83aa4ca19,
                    0x0f2f69371e869401,
                ])),
                Felt::new(BigInteger256([
                    0xe626d2fc1652ea9f,
                    0xcf95776190a12ad5,
                    0x832898c2cd78b6d9,
                    0x1267fb8fb3315359,
                ])),
                Felt::new(BigInteger256([
                    0x27690bd64026bc81,
                    0xa2dd65ff4e3fee35,
                    0x1470dc51a35e94d6,
                    0x02157840a516b457,
                ])),
                Felt::new(BigInteger256([
                    0x16f8815622311940,
                    0x6b81d0a905944857,
                    0x86eb918fdb158ba0,
                    0x10402141e3fbcb21,
                ])),
                Felt::new(BigInteger256([
                    0x0a9f29ba681c1fd3,
                    0xd3d7cd0a105c2a84,
                    0x2e3c2cfbcf97f5c6,
                    0x0e2d672c8f7113fb,
                ])),
                Felt::new(BigInteger256([
                    0xf3d11b66ce4f7c6a,
                    0xfdb363fb385c7bbe,
                    0x10f1f6d55c6d793e,
                    0x1059d019479fff91,
                ])),
            ],
            vec![
                Felt::new(BigInteger256([
                    0xf1b20b8da5280e1d,
                    0x664d82824b7b33b4,
                    0xc350f417f63a2eff,
                    0x0ebe168db4ca568d,
                ])),
                Felt::new(BigInteger256([
                    0xe8138cc4e1c158ca,
                    0xd7ed8150b924d32b,
                    0x5f079b44a2aecb16,
                    0x085a5847c3ceffbe,
                ])),
                Felt::new(BigInteger256([
                    0x9daad83907ec63d3,
                    0x3d1f7e5fe84bcfd2,
                    0x388ba5fa85592d3e,
                    0x112b24241e56edf6,
                ])),
                Felt::new(BigInteger256([
                    0x87174759e6990769,
                    0x0b45b4f64c0d6bb1,
                    0xbff6e0b84142eb26,
                    0x0b3928a72dee59e0,
                ])),
                Felt::new(BigInteger256([
                    0xb6a74996644e8b2b,
                    0xb1673df1999a37c1,
                    0xa2dad9b83d922652,
                    0x12288451e61e76da,
                ])),
                Felt::new(BigInteger256([
                    0x8104d72a5d4210e8,
                    0x6df47a99b109303a,
                    0x3b939406a630b1e4,
                    0x03f968fd7aef24af,
                ])),
                Felt::new(BigInteger256([
                    0x0abd4b8de5ea0734,
                    0x2f335e51d665bb82,
                    0x5d4777092c7b4a72,
                    0x11fba5d13543deb6,
                ])),
                Felt::new(BigInteger256([
                    0x1e27f23fd44be399,
                    0x2c19b0a53d7564d1,
                    0x6097066f6c15b636,
                    0x0ea0e9f313bf53d3,
                ])),
                Felt::new(BigInteger256([
                    0xb158a2da9f289ee9,
                    0x7736317f25120672,
                    0x62ac8018f29feadb,
                    0x0f594e420d4c1f67,
                ])),
                Felt::new(BigInteger256([
                    0xc976d3f59a3bb913,
                    0x52cfc1d18a68c5ae,
                    0x46657c887ee43501,
                    0x0aecae70829c3603,
                ])),
                Felt::new(BigInteger256([
                    0x6858d4b43d918acd,
                    0xe8244c877952c4ca,
                    0x5f3fe945a1855467,
                    0x0745d726ad9830e8,
                ])),
                Felt::new(BigInteger256([
                    0x09da0b80a15ff1a0,
                    0x8f1d1d0319a9c792,
                    0x6e7e6c7ae2798cf4,
                    0x06b037b0254a4fcc,
                ])),
            ],
        ];

        let output_data = [
            [Felt::new(BigInteger256([
                0x7b33e13648b5c0bb,
                0xb9d36f92eb488107,
                0xf210e988be4674ae,
                0x0159137c261f6858,
            ]))],
            [Felt::new(BigInteger256([
                0x1d80019121a72260,
                0x5c904a7ac31a47a5,
                0xfa2ad651ce016239,
                0x108b69e247dda803,
            ]))],
            [Felt::new(BigInteger256([
                0xf2ae88b207a3e2c8,
                0x4f96c7db120c3067,
                0xc04b1fd7e7904565,
                0x0ca372b35e3411fc,
            ]))],
            [Felt::new(BigInteger256([
                0xb22942e9aac57dac,
                0x6a0ba7270c50cf7f,
                0x79f24e0bb0a912fa,
                0x06b958340c2fdd7d,
            ]))],
            [Felt::new(BigInteger256([
                0xa00f9162460dd06d,
                0x02ce6bba79b5c9aa,
                0xe95fa96a2663a326,
                0x05e8c47acd3ce70d,
            ]))],
            [Felt::new(BigInteger256([
                0x9f29ea1e4941a297,
                0x3ea90bec24bfceea,
                0x89efa4194ee854d4,
                0x006c104dfce49a7b,
            ]))],
            [Felt::new(BigInteger256([
                0xc08994fc02e18628,
                0x48e9a742f8437f98,
                0xf47f13508f393831,
                0x026b717657d3ec41,
            ]))],
            [Felt::new(BigInteger256([
                0xe6719bf88455a16a,
                0x24106b83a8b3f726,
                0x16a7226a9fc80781,
                0x0837d3820af80f43,
            ]))],
            [Felt::new(BigInteger256([
                0x89881dded4b5fb08,
                0x148f714322daf10d,
                0x01f8ababc5b7a96d,
                0x073b80ce44bb80b8,
            ]))],
            [Felt::new(BigInteger256([
                0x3e4338e7cd29206c,
                0x5fb4f7a40d802819,
                0xf57dce408cb5e890,
                0x002839b3cc0ab585,
            ]))],
        ];

        for (input, expected) in input_data.iter().zip(output_data) {
            assert_eq!(expected.to_vec(), AnemoiHash::compress_k(input, 12));
        }
    }
}
