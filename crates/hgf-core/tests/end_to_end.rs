//! Real gw captures run through the parser and the disk map.

use hgf_core::disk::diskmap::{DiskMap, MapMode, TrackState};
use hgf_core::disk::trackset::TrackSet;
use hgf_core::gw::parse::{Output, OutputParser};

fn replay(log: &str, mode: MapMode) -> (DiskMap, Vec<Output>) {
    let mut map = DiskMap::new(mode, TrackSet::dense(80, 2));
    let mut p = OutputParser::new();
    let mut outs = Vec::new();
    for line in log.lines() {
        let o = p.feed(line);
        map.apply(&o);
        outs.push(o);
    }
    (map, outs)
}

#[test]
fn good_amiga_disk() {
    let (mut map, outs) = replay(include_str!("fixtures/convert_amiga.log"), MapMode::Read);
    map.finish(true);
    assert!(map.geometry_known);
    let c = map.counts();
    assert_eq!((c.ok, c.bad, c.pending), (160, 0, 0));
    assert_eq!(map.found, Some((1760, 1760)));
    assert_eq!(map.get(79, 1).sector_map.len(), 11);
    assert!(map.get(79, 1).sector_map.iter().all(|s| *s == Some(true)));
    assert!(!outs.iter().any(Output::is_error));
}

#[test]
fn unreadable_ibm_tracks() {
    let (mut map, _) = replay(include_str!("fixtures/convert_ibm_bad.log"), MapMode::Read);
    assert_eq!(map.geometry.cyls, vec![0, 1, 2]);
    assert_eq!(map.geometry.heads, vec![0]);
    // gw convert does not retry: a partial track is bad as soon as the next one starts…
    assert_eq!(map.get(1, 0).state, TrackState::Bad);
    assert_eq!(map.get(2, 0).state, TrackState::Retrying);
    // …and the last one when gw exits.
    map.finish(true);
    assert_eq!(map.get(2, 0).state, TrackState::Bad);
    assert_eq!(map.get(1, 0).sectors, Some((0, 18)));
    assert_eq!(map.found, Some((0, 54)));
    assert_eq!(map.get(2, 0).sector_map.iter().filter(|s| **s == Some(false)).count(), 18);
}
