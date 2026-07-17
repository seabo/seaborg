use crate::masks::{FILE_A, FILE_H, RANK_1, RANK_8};

/// Size of the magic rook table.
const ROOK_M_SIZE: usize = 102_400;
/// Size of the magic bishop table.
const BISHOP_M_SIZE: usize = 5248;

const B_DELTAS: [i8; 4] = [7, 9, -9, -7];
const R_DELTAS: [i8; 4] = [8, 1, -8, -1];

const BISHOP_MAGIC_NUMBERS: [u64; 64] = [
    4616295173873598496,
    9008308706935072,
    2310347717482119297,
    11265597217226816,
    565218098806784,
    2306143313595466752,
    612772128644759680,
    1166575244564038912,
    19800369931264,
    2305845225484189764,
    9223451204177133568,
    917173119746049,
    2852142021869632,
    9224640881898971136,
    6794192996009984,
    72057870024183872,
    1125968693962784,
    4503634255609928,
    1158551287743648512,
    2342014227382026754,
    18163983640953088,
    36099174358458384,
    281481554955012,
    36069006536475140,
    1154117773326025744,
    335353730966022,
    2398254762522050688,
    4774105876367688448,
    4620838903039926272,
    1157446548217413888,
    2252092442018436,
    72350133940945024,
    578721365390532897,
    578721365390532897,
    40834221179145216,
    144713356794724612,
    10090880122694664325,
    117094142226530368,
    615112986214934528,
    1155456987580989706,
    290510797996564480,
    114426652938240,
    9327099964909717506,
    720576223855577093,
    590042212123869440,
    2738787261918241026,
    1173240507413833256,
    2738787261918241026,
    432909618525569088,
    666815353715961856,
    70660971889704,
    2291382778069025,
    288230513892950528,
    48431425638171168,
    153124655106922752,
    9530755940089933952,
    282025875473472,
    1337298474242048,
    2305930974506062128,
    1876876244516209193,
    19281040503742976,
    303008126096901156,
    216243460678648320,
    5190416231459072640,
];

const ROOK_MAGIC_NUMBERS: [u64; 64] = [
    756605012284543520,
    9241386710510608392,
    2341906990873214984,
    1224996690967199748,
    13979209048420122632,
    2377901702830359048,
    288239240972739617,
    144116305338075204,
    576601491943456804,
    351981164040192,
    3458905320036769793,
    4900057166428242051,
    649785138393645184,
    4629841171605225600,
    6896687792521472,
    4629841156572725504,
    18019346333138980,
    45036271153729536,
    3530963395371081728,
    5198562444649170976,
    5765874710319465473,
    2316118495198185984,
    1548662161277184,
    2377907200396910852,
    36099202270397473,
    4639307975314440832,
    74872661633695778,
    17594334052480,
    9288695706812432,
    19140307039817736,
    17609366438146,
    13844085604092216645,
    36028934462111808,
    141012374659076,
    19791779741824,
    74934333118353409,
    10380805939337888768,
    9529619012695098368,
    1163868021723137,
    313700384900,
    36028934463176704,
    2305878194928009216,
    5075895970824208,
    4504701287008256,
    37436240756277252,
    562958610497664,
    18086072957534216,
    53053600432132,
    36064256277348480,
    3461157326003175680,
    1157442698569842816,
    2308112409937739904,
    5764753792449970432,
    9512167081020752384,
    81224291869393920,
    550896730624,
    9153442893545490,
    9153442893545490,
    17729642039361,
    1443685172897644577,
    563122357182722,
    563122357182722,
    563122357182722,
    4631991800301625606,
];

/// Lookup metadata for a square's segment of an attack table.
#[derive(Copy, Clone, Debug)]
struct MagicHash {
    offset: usize,
    mask: u64,
    magic: u64,
    shift: u32,
}

impl MagicHash {
    const EMPTY: Self = Self {
        offset: 0,
        mask: 0,
        magic: 0,
        shift: 0,
    };
}

#[derive(Copy, Clone, Debug)]
struct SMagic {
    attacks: &'static u64,
    mask: u64,
    magic: u64,
    shift: u32,
}

static BISHOP_HASHES: [MagicHash; 64] = gen_magics(&B_DELTAS, &BISHOP_MAGIC_NUMBERS);
static ROOK_HASHES: [MagicHash; 64] = gen_magics(&R_DELTAS, &ROOK_MAGIC_NUMBERS);

#[allow(long_running_const_eval)]
static BISHOP_ATTACKS: [u64; BISHOP_M_SIZE] = gen_attack_table(&B_DELTAS, &BISHOP_HASHES);
#[allow(long_running_const_eval)]
static ROOK_ATTACKS: [u64; ROOK_M_SIZE] = gen_attack_table(&R_DELTAS, &ROOK_HASHES);

static BISHOP_MAGICS: [SMagic; 64] = bind_magics(&BISHOP_HASHES, &BISHOP_ATTACKS);
static ROOK_MAGICS: [SMagic; 64] = bind_magics(&ROOK_HASHES, &ROOK_ATTACKS);

#[inline]
pub fn bishop_attacks(mut occupied: u64, square: u8) -> u64 {
    debug_assert!(square < 64);
    let magic = unsafe { BISHOP_MAGICS.get_unchecked(square as usize) };
    occupied &= magic.mask;
    occupied = occupied.wrapping_mul(magic.magic);
    occupied = occupied.wrapping_shr(magic.shift);
    unsafe { *(magic.attacks as *const u64).add(occupied as usize) }
}

#[inline]
pub fn rook_attacks(mut occupied: u64, square: u8) -> u64 {
    debug_assert!(square < 64);
    let magic = unsafe { ROOK_MAGICS.get_unchecked(square as usize) };
    occupied &= magic.mask;
    occupied = occupied.wrapping_mul(magic.magic);
    occupied = occupied.wrapping_shr(magic.shift);
    unsafe { *(magic.attacks as *const u64).add(occupied as usize) }
}

const fn gen_magics(deltas: &[i8; 4], magic_numbers: &[u64; 64]) -> [MagicHash; 64] {
    let mut magics = [MagicHash::EMPTY; 64];
    let mut offset = 0;
    let mut square = 0;

    while square < 64 {
        let rank = 0xff_u64 << ((square / 8) * 8);
        let file = FILE_A << (square % 8);
        let edges = ((RANK_1 | RANK_8) & !rank) | ((FILE_A | FILE_H) & !file);
        let mask = sliding_attack(deltas, square as u8, 0) & !edges;
        let shift = 64 - mask.count_ones();
        magics[square] = MagicHash {
            offset,
            mask,
            magic: magic_numbers[square],
            shift,
        };
        offset += 1 << mask.count_ones();
        square += 1;
    }

    magics
}

const fn gen_attack_table<const N: usize>(deltas: &[i8; 4], magics: &[MagicHash; 64]) -> [u64; N] {
    let mut attacks = [0; N];
    let mut square = 0;

    while square < 64 {
        let magic = &magics[square];
        let mut occupied = 0_u64;
        loop {
            let index = occupied.wrapping_mul(magic.magic).wrapping_shr(magic.shift) as usize;
            attacks[magic.offset + index] = sliding_attack(deltas, square as u8, occupied);
            occupied = occupied.wrapping_sub(magic.mask) & magic.mask;
            if occupied == 0 {
                break;
            }
        }
        square += 1;
    }

    attacks
}

const fn bind_magics<const N: usize>(
    hashes: &[MagicHash; 64],
    attacks: &'static [u64; N],
) -> [SMagic; 64] {
    let mut magics = [SMagic {
        attacks: &attacks[0],
        mask: 0,
        magic: 0,
        shift: 0,
    }; 64];
    let mut square = 0;

    while square < magics.len() {
        let hash = &hashes[square];
        magics[square] = SMagic {
            attacks: &attacks[hash.offset],
            mask: hash.mask,
            magic: hash.magic,
            shift: hash.shift,
        };
        square += 1;
    }

    magics
}

/// Returns sliding attacks given an array of four single-square deltas.
/// The origin is excluded and the first occupied square in each direction is included.
pub const fn sliding_attack(deltas: &[i8; 4], sq: u8, occupied: u64) -> u64 {
    assert!(sq < 64);
    let mut attack = 0;
    let mut direction = 0;

    while direction < deltas.len() {
        let mut current = sq as i16;
        loop {
            let next = current + deltas[direction] as i16;
            if next < 0 || next >= 64 {
                break;
            }

            let current_file = current as u8 % 8;
            let next_file = next as u8 % 8;
            if current_file > next_file + 1 || next_file > current_file + 1 {
                break;
            }

            let bit = 1_u64 << next as u32;
            attack |= bit;
            if occupied & bit != 0 {
                break;
            }
            current = next;
        }
        direction += 1;
    }

    attack
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_magic_attacks(
        deltas: &[i8; 4],
        magics: &[SMagic; 64],
        attack_fn: fn(u64, u8) -> u64,
    ) {
        for (square, magic) in magics.iter().enumerate() {
            let mut occupied = 0;
            loop {
                assert_eq!(
                    attack_fn(occupied, square as u8),
                    sliding_attack(deltas, square as u8, occupied),
                    "incorrect attacks for square {square} and occupancy {occupied:#018x}"
                );
                occupied = occupied.wrapping_sub(magic.mask) & magic.mask;
                if occupied == 0 {
                    break;
                }
            }
        }
    }

    #[test]
    fn magic_tables_match_sliding_attacks() {
        assert_magic_attacks(&B_DELTAS, &BISHOP_MAGICS, bishop_attacks);
        assert_magic_attacks(&R_DELTAS, &ROOK_MAGICS, rook_attacks);
    }
}
