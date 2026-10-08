use yrs::encoding::read::Error;
use yrs::sync::AwarenessUpdate;
use yrs::updates::decoder::Decode;
use yrs::{IdSet, Snapshot, StateVector, Update};

fn refused<T>(result: Result<T, Error>) {
    assert!(matches!(result, Err(Error::UnexpectedValue)));
}

// Assemble literal v2 stream bytes, without using the encoder under test.
fn v2(streams: [&[u8]; 9], rest: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0];
    for stream in streams {
        assert!(stream.len() < 128);
        bytes.push(stream.len() as u8);
        bytes.extend_from_slice(stream);
    }
    bytes.extend_from_slice(rest);
    bytes
}

#[test]
fn delete_range_clock_overflow_is_refused() {
    let bytes = [1, 5, 1, 0xff, 0xff, 0xff, 0xff, 0x0f, 2];
    refused(IdSet::decode_v1(&bytes));
    let mut update = vec![0];
    update.extend_from_slice(&bytes);
    refused(Update::decode_v1(&update));
    // v2 stores length minus one. All three additions must be checked.
    for rest in [
        &[1, 5, 1, 0xff, 0xff, 0xff, 0xff, 0x0f, 1][..],
        &[1, 5, 1, 0, 0xff, 0xff, 0xff, 0xff, 0x0f],
        &[1, 5, 2, 0xfe, 0xff, 0xff, 0xff, 0x0f, 0, 1, 0],
    ] {
        refused(IdSet::decode_v2(&v2(
            [&[], &[], &[], &[], &[], &[0], &[], &[], &[]],
            rest,
        )));
    }
}

#[test]
fn gc_and_skip_clock_overflow_is_refused() {
    for tag in [0, 10] {
        let bytes = [1, 1, 5, 0xff, 0xff, 0xff, 0xff, 0x0f, tag, 2, 0];
        refused(Update::decode_v1(&bytes));
        let rest = if tag == 0 {
            vec![1, 1, 0xff, 0xff, 0xff, 0xff, 0x0f, 0]
        } else {
            vec![1, 1, 0xff, 0xff, 0xff, 0xff, 0x0f, 2, 0]
        };
        refused(Update::decode_v2(&v2(
            [&[], &[5], &[], &[], &[tag], &[0], &[], &[], &[2]],
            &rest,
        )));
    }
}

#[test]
fn item_and_block_chain_clock_overflow_is_refused() {
    // Deleted content and a one-character text item, each starting at u32::MAX.
    for content in [&[1, 1, 1, b't', 2][..], &[4, 1, 1, b't', 1, b'x']] {
        let mut bytes = vec![1, 1, 5, 0xff, 0xff, 0xff, 0xff, 0x0f];
        bytes.extend_from_slice(content);
        bytes.push(0);
        refused(Update::decode_v1(&bytes));
    }
    let bytes = [1, 2, 5, 0xfe, 0xff, 0xff, 0xff, 0x0f, 0, 1, 0, 1, 0];
    refused(Update::decode_v1(&bytes));
    // The highest representable end clock itself remains valid.
    assert!(Update::decode_v1(&[1, 1, 5, 0xfe, 0xff, 0xff, 0xff, 0x0f, 0, 1, 0]).is_ok());
}

#[test]
fn zero_length_gc_is_refused() {
    refused(Update::decode_v1(&[1, 1, 0x60, 0, 0, 0, 0, 0x2c]));
    refused(Update::decode_v2(&v2(
        [&[], &[0xa0, 1], &[], &[], &[0], &[0], &[], &[], &[0]],
        &[1, 1, 0, 0],
    )));
}

#[test]
fn zero_length_skip_is_refused() {
    refused(Update::decode_v1(&[1, 1, 0x60, 0, 10, 0, 0]));
    refused(Update::decode_v2(&v2(
        [&[], &[0xa0, 1], &[], &[], &[10], &[0], &[], &[], &[]],
        &[1, 1, 0, 0, 0],
    )));
}

#[test]
fn v2_block_length_above_u32_is_refused() {
    // Length 2^32 in the unsigned RLE stream must not narrow to zero.
    refused(Update::decode_v2(&v2(
        [
            &[],
            &[5],
            &[],
            &[],
            &[0],
            &[0],
            &[],
            &[],
            &[0x80, 0x80, 0x80, 0x80, 0x20],
        ],
        &[1, 1, 0, 0],
    )));
}

const INVALID_CLIENT: &[u8] = &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x10];

#[test]
fn update_client_at_53_bits_is_refused() {
    let mut bytes = vec![1, 1];
    bytes.extend_from_slice(INVALID_CLIENT);
    bytes.extend_from_slice(&[0, 0, 1, 0]);
    refused(Update::decode_v1(&bytes));
    // Signed magnitude RLE uses six bits in its first byte.
    let client_v2 = &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x20];
    refused(Update::decode_v2(&v2(
        [&[], client_v2, &[], &[], &[0], &[0], &[], &[], &[1]],
        &[1, 1, 0, 0],
    )));
}

#[test]
fn state_vector_and_snapshot_clients_at_53_bits_are_refused() {
    let mut bytes = vec![1];
    bytes.extend_from_slice(INVALID_CLIENT);
    bytes.push(0);
    refused(StateVector::decode_v1(&bytes));
    refused(StateVector::decode_v2(&v2(
        [&[], &[], &[], &[], &[], &[0], &[], &[], &[]],
        &bytes,
    )));
    bytes.insert(0, 0); // empty delete set
    refused(Snapshot::decode_v1(&bytes));
    refused(Snapshot::decode_v2(&v2(
        [&[], &[], &[], &[], &[], &[0], &[], &[], &[]],
        &bytes,
    )));
}

#[test]
fn delete_set_and_awareness_clients_at_53_bits_are_refused() {
    let mut bytes = vec![1];
    bytes.extend_from_slice(INVALID_CLIENT);
    bytes.extend_from_slice(&[1, 0, 1]);
    refused(IdSet::decode_v1(&bytes));
    refused(IdSet::decode_v2(&v2(
        [&[], &[], &[], &[], &[], &[0], &[], &[], &[]],
        &bytes,
    )));
    let mut bytes = vec![1];
    bytes.extend_from_slice(INVALID_CLIENT);
    bytes.extend_from_slice(&[0, 0]);
    refused(AwarenessUpdate::decode_v1(&bytes));
}

#[test]
fn embedded_reference_clients_at_53_bits_are_refused() {
    // Left origin, right origin, and explicit parent item.
    for prefix in [
        &[1, 1, 5, 0, 0x84][..],
        &[1, 1, 5, 0, 0x44],
        &[1, 1, 5, 0, 4, 0],
    ] {
        let mut bytes = prefix.to_vec();
        bytes.extend_from_slice(INVALID_CLIENT);
        bytes.extend_from_slice(&[0, 1, b'x', 0]);
        refused(Update::decode_v1(&bytes));
    }
    let clients_v2 = &[5, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x20];
    for tag in [0x84, 0x44, 4] {
        refused(Update::decode_v2(&v2(
            [
                &[],
                clients_v2,
                &[0],
                &[0],
                &[tag],
                &[1, b'x', 1],
                &[0],
                &[],
                &[],
            ],
            &[1, 1, 0, 0],
        )));
    }
    // A collapsed Move's start reference is also a wire client ID.
    let mut bytes = vec![1, 1, 5, 0, 11, 1, 1, b't', 1];
    bytes.extend_from_slice(INVALID_CLIENT);
    bytes.extend_from_slice(&[0, 0]);
    refused(Update::decode_v1(&bytes));
}

#[test]
#[cfg(not(feature = "small-client"))]
fn largest_valid_client_is_preserved() {
    let sv =
        StateVector::decode_v1(&[1, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x0f, 1]).unwrap();
    assert_eq!(sv.iter().next().unwrap().0.get(), (1u64 << 53) - 1);
}

#[test]
fn overflowing_and_overlong_client_varints_are_refused() {
    // u64 overflow bits and an eleventh byte previously folded into another ID.
    for client in [
        &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 2][..],
        &[
            0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x81, 1,
        ],
    ] {
        let mut bytes = vec![1, 1, 5, 0, 4, 0]; // explicit parent ID
        bytes.extend_from_slice(client);
        bytes.extend_from_slice(&[0, 1, b'x', 0]);
        assert!(matches!(
            Update::decode_v1(&bytes),
            Err(Error::InvalidVarInt)
        ));
    }
    // v2 client stream with a signed-magnitude value above u64.
    let client = &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 2];
    assert!(Update::decode_v2(&v2(
        [&[], client, &[], &[], &[0], &[0], &[], &[], &[1]],
        &[1, 1, 0, 0]
    ))
    .is_err());
}

#[test]
fn overflowing_clock_varint_is_refused() {
    assert!(matches!(
        Update::decode_v1(&[1, 1, 5, 0x80, 0x80, 0x80, 0x80, 0x10, 0, 1, 0]),
        Err(Error::InvalidVarInt)
    ));
}

#[test]
fn truncated_and_overflowing_v2_stream_lengths_are_errors() {
    for bytes in [
        &[0, 0x80][..],
        &[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 1],
        &[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 2],
    ] {
        assert!(Update::decode_v2(bytes).is_err());
        assert!(StateVector::decode_v2(bytes).is_err());
        assert!(Snapshot::decode_v2(bytes).is_err());
    }
}

#[test]
fn integer_limits_and_negative_zero_are_preserved() {
    use yrs::encoding::read::{Cursor, Read};
    let max = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 1];
    assert_eq!(Cursor::new(&max).read_var::<u64>().unwrap(), u64::MAX);
    let min = [0xc0, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 2];
    assert_eq!(Cursor::new(&min).read_var::<i64>().unwrap(), i64::MIN);
    let zero = Cursor::new(&[0x40]).read_var_signed::<i64>().unwrap();
    assert_eq!(zero.value(), 0);
    assert!(zero.is_negative());
}

#[test]
fn overflowing_v2_run_counts_are_refused() {
    let count = &[0x41, 0xff, 0xff, 0xff, 0xff, 0x0f];
    refused(Update::decode_v2(&v2(
        [&[], count, &[], &[], &[0], &[0], &[], &[], &[1]],
        &[1, 1, 0, 0],
    )));
    let diff = &[1, 0xff, 0xff, 0xff, 0xff, 0x0f];
    refused(Update::decode_v2(&v2(
        [
            &[],
            &[5, 6],
            diff,
            &[],
            &[0x84],
            &[1, b'x', 1],
            &[],
            &[],
            &[],
        ],
        &[1, 1, 0, 0],
    )));
}
