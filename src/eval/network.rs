//! The **Network**'s weights, embedded at build time and parsed once.
//!
//! The file is bullet's `quantised.bin` for a `(768 → HIDDEN)x2 → 1` network: `i16`
//! values, little-endian, in the order feature weights, feature biases, output weights,
//! output bias, then padding to a multiple of 64 bytes. The feature weights are
//! column-major, so each input feature's `HIDDEN` weights are stored together.

use std::sync::LazyLock;

/// Neurons in the hidden layer, per perspective. A multiple of 32, so that every row of
/// `HIDDEN` `i16`s fills whole 64-byte cache lines.
pub const HIDDEN: usize = 256;

/// Input features per perspective: 2 colours × 6 piece types × 64 squares.
pub const INPUTS: usize = 768;

/// The quantisation of the feature weights and biases, and the top of the activation's
/// clamp.
pub const QA: i16 = 255;

/// The quantisation of the output weights.
pub const QB: i32 = 64;

/// Centipawns per unit of the network's float output; the trainer's `eval_scale`.
pub const SCALE: i32 = 143;

const _: () = assert!(HIDDEN.is_multiple_of(32), "HIDDEN must be a multiple of 32");

/// The `i16` values in the network file, before padding.
const VALUES: usize = INPUTS * HIDDEN + HIDDEN + 2 * HIDDEN + 1;

/// The network file's size: its values padded to a multiple of 64 bytes.
const FILE_SIZE: usize = (VALUES * 2).next_multiple_of(64);

/// The network file named by `EVALFILE`, which `.cargo/config.toml` points at
/// `networks/default.bin` unless the environment sets it.
static FILE: &[u8; FILE_SIZE] = {
    const BYTES: &[u8] = include_bytes!(env!("EVALFILE"));
    assert!(
        BYTES.len() == FILE_SIZE,
        "the network file named by EVALFILE has the wrong size for HIDDEN"
    );
    match BYTES.first_chunk() {
        Some(file) => file,
        None => unreachable!(),
    }
};

/// The embedded network, parsed on first use.
pub(super) static NETWORK: LazyLock<Network> = LazyLock::new(|| Network::parse(FILE));

/// One row of `HIDDEN` values, aligned to a cache line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C, align(64))]
pub(super) struct Row(pub(super) [i16; HIDDEN]);

impl Row {
    const ZERO: Self = Self([0; HIDDEN]);
}

/// The weights of a `(768 → HIDDEN)x2 → 1` network.
#[derive(Debug)]
pub(super) struct Network {
    /// Each input feature's weights into the hidden layer.
    pub(super) feature_weights: Box<[Row]>,
    pub(super) feature_bias: Box<Row>,
    /// Output weights for the side to move's half of the hidden layer, then the other
    /// side's.
    pub(super) output_weights: Box<[Row; 2]>,
    pub(super) output_bias: i16,
}

impl Network {
    /// Parses a network file of exactly [`FILE_SIZE`] bytes.
    fn parse(file: &[u8; FILE_SIZE]) -> Self {
        let mut values = file
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&bytes| i16::from_le_bytes(bytes));
        let mut row = || {
            let mut row = Row::ZERO;
            for (slot, value) in row.0.iter_mut().zip(&mut values) {
                *slot = value;
            }
            row
        };
        let feature_weights = (0..INPUTS).map(|_| row()).collect();
        let feature_bias = Box::new(row());
        let output_weights = Box::new([row(), row()]);
        let output_bias = values
            .next()
            .expect("the file size leaves room for the output bias");
        Self {
            feature_weights,
            feature_bias,
            output_weights,
            output_bias,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_is_read_in_bullets_order() {
        let network = &*NETWORK;
        let value = |index: usize| i16::from_le_bytes([FILE[2 * index], FILE[2 * index + 1]]);
        assert_eq!(network.feature_weights.len(), INPUTS);
        assert_eq!(network.feature_weights[1].0[0], value(HIDDEN));
        assert_eq!(
            network.feature_weights[INPUTS - 1].0[HIDDEN - 1],
            value(INPUTS * HIDDEN - 1)
        );
        assert_eq!(network.feature_bias.0[0], value(INPUTS * HIDDEN));
        assert_eq!(network.output_weights[1].0[0], value((INPUTS + 2) * HIDDEN));
        assert_eq!(network.output_bias, value(VALUES - 1));
    }
}
