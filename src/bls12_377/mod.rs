pub use ark_bls12_377::Fq as Felt;
pub use ark_ff::BigInteger384;
use ark_ff::Field;

#[cfg(any(feature = "128_bits", feature = "256_bits"))]
mod sbox;

/// An instantiation of Anemoi with state width 2 and
/// rate 1 aimed at providing 128 bits security.
#[cfg(feature = "128_bits")]
pub mod anemoi_2_1_128;

/// An instantiation of Anemoi with state width 8 and
/// rate 7 aimed at providing 128 bits security.
#[cfg(feature = "128_bits")]
pub mod anemoi_8_7_128;

/// An instantiation of Anemoi with state width 12 and
/// rate 11 aimed at providing 128 bits security.
#[cfg(feature = "128_bits")]
pub mod anemoi_12_11_128;

/// An instantiation of Anemoi with state width 2 and
/// rate 1 aimed at providing 256 bits security.
#[cfg(feature = "256_bits")]
pub mod anemoi_2_1_256;

/// An instantiation of Anemoi with state width 8 and
/// rate 7 aimed at providing 256 bits security.
#[cfg(feature = "256_bits")]
pub mod anemoi_8_7_256;

/// An instantiation of Anemoi with state width 12 and
/// rate 11 aimed at providing 256 bits security.
#[cfg(feature = "256_bits")]
pub mod anemoi_12_11_256;

// HELPER FUNCTION
// ================================================================================================

#[inline(always)]
pub(crate) fn mul_by_generator(x: &Felt) -> Felt {
    let x2 = x.double();
    let x4 = x2.double();
    let x8 = x4.double();
    let x16 = x8.double();

    x16 - x
}
