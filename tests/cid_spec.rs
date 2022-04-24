//! Tests the [`CidSpec`].
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
