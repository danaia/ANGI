use super::*;

#[test]
fn a_bound_driver_without_an_endpoint_is_reported_as_pending() {
    let inventory = Inventory {
        model: "Test PC".into(),
        kernel: "test".into(),
        groups: [(
            Kind::Sound,
            vec![Device {
                name: "Audio controller".into(),
                id: None,
                compatible: String::new(),
                bound: "snd_hda_intel".into(),
                candidates: Vec::new(),
                endpoints: Vec::new(),
            }],
        )]
        .into(),
    };
    let (rows, missing) = summarize(&inventory);
    assert!(missing);
    assert!(rows[1].contains("no active device"));
    assert!(rows[3].contains("No device detected"));
}
