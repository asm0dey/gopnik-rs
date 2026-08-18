use gopnik::locations::Places;
use std::path::Path;

#[test]
fn places_round_trips_the_real_file() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("orig").join("PLACES.SAV");
    let bytes = std::fs::read(p).unwrap();
    assert_eq!(bytes.len(), 7);

    let places = Places::from_bytes(&bytes);
    assert_eq!(places.to_bytes().to_vec(), bytes);
}

#[test]
fn new_district_hides_all_places() {
    let mut places = Places::from_bytes(&[1u8; 7]);
    places.reset_for_new_district();
    assert_eq!(places.to_bytes(), [0u8; 7]);
}
