//! The engine loads the network file the Python trainer's exporter writes.
//!
//! `tools/trainer/export.py` serialises a quantized network into the `SBNN`
//! format this crate reads. This test closes that language loop: it loads a
//! fixture the exporter produced and asserts the loader accepts it and decodes it
//! to exactly the network the exporter encoded, so the trainer and engine agree on
//! the byte layout in practice, not just on paper.
//!
//! The fixture is a deterministic, patterned network (not a trained one) so its
//! weights are reproducible from a formula both sides compute. Regenerate it with:
//!
//! ```text
//! cd tools/trainer && .venv/bin/python export.py \
//!     --emit-fixture ../../engine/tests/fixtures/exported_v1.sbnn
//! ```
//!
//! The same formula lives in `export.py::_demo_network`; if the format or the
//! pattern changes, regenerate the fixture and update `expected_network` together.

use engine::nnue::{Network, Parameters, INPUT_DIM};

const FIXTURE: &[u8] = include_bytes!("fixtures/exported_v1.sbnn");

const HIDDEN: u32 = 16;
const QA: u16 = 255;
const QB: u16 = 64;
const SCALE: i32 = 400;

/// The network the exporter's `_demo_network` encodes, rebuilt here from the same
/// pattern so the test asserts the loaded weights, not merely that a load
/// succeeded.
fn expected_network() -> Network {
    let h = HIDDEN as usize;
    let mut w_ft = vec![0i16; INPUT_DIM as usize * h];
    for feature in 0..INPUT_DIM as usize {
        for unit in 0..h {
            // Feature-major: feature `f`'s H weights are contiguous at `f * H`.
            w_ft[feature * h + unit] = ((feature * 31 + unit * 7) % 41) as i16 - 20;
        }
    }
    let b_ft: Vec<i16> = (0..h).map(|unit| (unit % 7) as i16 - 3).collect();
    let w_out: Vec<i16> = (0..2 * h).map(|j| ((j * 13) % 49) as i16 - 24).collect();
    let b_out = vec![0i32];
    Network::new(
        HIDDEN,
        QA,
        QB,
        SCALE,
        Parameters {
            w_ft,
            b_ft,
            w_out,
            b_out,
        },
    )
    .expect("the fixture pattern satisfies the network build invariant")
}

#[test]
fn engine_loads_the_exported_network_file() {
    let net = Network::read(&mut &FIXTURE[..]).expect("engine loader accepts the exported file");

    assert_eq!(net.hidden_width(), HIDDEN);
    assert_eq!(net.qa(), QA);
    assert_eq!(net.qb(), QB);
    assert_eq!(net.scale(), SCALE);
    // Every decoded weight matches the network the exporter encoded.
    assert_eq!(net, expected_network());
}
