//! Tests the [`CidSpec`].
use carrier_pigeon::CId;
use carrier_pigeon::net::CIdSpec;

/// Tests [`CIdSpec::All`].
#[test]
fn all() {
    let spec = CIdSpec::All;

    let cid_vec = vec![0, 1, 2, 3, 10, 20, 1000, 102901];
    let expected_vec = vec![true; cid_vec.len()-1];
    for (cid, expected) in cid_vec.into_iter().zip(expected_vec) {
        assert_eq!(spec.matches(cid), expected)
    }
}

/// Tests [`CIdSpec::None`].
#[test]
fn none() {
    let spec = CIdSpec::None;

    let cid_vec = vec![0, 1, 2, 3, 10, 20, 1000, 102901];
    let expected_vec = vec![false; cid_vec.len()-1];
    for (cid, expected) in cid_vec.into_iter().zip(expected_vec) {
        assert_eq!(spec.matches(cid), expected)
    }
}

/// Tests [`CIdSpec::Only`].
#[test]
fn only() {
    let spec = CIdSpec::Only(12);

    let cid_vec = vec![0, 1, 2, 3, 10, 12, 20, 1000, 102901];
    let mut expected_vec = vec![false; cid_vec.len()-1];
    expected_vec[5] = true;

    for (cid, expected) in cid_vec.into_iter().zip(expected_vec) {
        assert_eq!(spec.matches(cid), expected)
    }
}

/// Tests [`CIdSpec::Except`].
#[test]
fn except() {
    let spec = CIdSpec::Except(12);

    let cid_vec = vec![0, 1, 2, 3, 10, 12, 20, 1000, 102901];
    let mut expected_vec = vec![true; cid_vec.len()-1];
    expected_vec[5] = false;

    for (cid, expected) in cid_vec.into_iter().zip(expected_vec) {
        assert_eq!(spec.matches(cid), expected)
    }
}

/// Tests [`CIdSpec::overlaps`].
#[test]
fn overlaps() {
    use CIdSpec::*;

    // (first, second, expected_result).
    let cases = vec![
        // None tests
        (None, None, false),
        (None, All, false),
        (All, None, false),
        (None, Only(1), false),
        (Only(1), None, false),
        (None, Except(1), false),
        (Except(1), None, false),

        // All tests
        (All, All, true),
        (All, Only(1), true),
        (Only(1), All, true),
        (All, Except(2), true),
        (Except(2), All, true),

        // Only tests
        (Only(1), Only(1), true),
        (Only(1), Only(2), false),

        // Except tests
        (Except(1), Except(1), true),
        (Except(1), Except(2), true),

        // Only & Except tests
        (Except(1), Only(1), false),
        (Only(1), Except(1), false),

        (Except(1), Only(2), true),
        (Only(1), Except(2), true),
    ];

    for (first, second, expected) in cases {
        assert_eq!(first.overlaps(second), expected);
        assert_eq!(second.overlaps(first), expected);
    }
}
