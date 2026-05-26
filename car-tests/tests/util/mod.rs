#![allow(dead_code)]

use std::{error::Error, path::PathBuf};

use test_support::fixture_group;

pub(crate) type TestResult<T = ()> = Result<T, Box<dyn Error>>;

pub(crate) fn smoke_fixture(name: &str) -> PathBuf {
    let group = fixture_group("smoke");
    let path = group.path(name);
    assert!(
        path.exists(),
        "missing {} fixture: {}",
        group.name,
        path.display()
    );
    path
}

pub(crate) fn full_fixture(name: &str) -> Option<PathBuf> {
    let group = fixture_group("full");
    if !group.is_enabled() {
        eprintln!(
            "skipping {} fixture '{name}': set CAR_TEST_FULL=1 to enable",
            group.name
        );
        return None;
    }

    let path = group.path(name);
    if !path.exists() {
        eprintln!(
            "skipping {} fixture '{name}': missing {}",
            group.name,
            path.display()
        );
        return None;
    }

    Some(path)
}

pub(crate) fn load_full_car(name: &str) -> Option<car::Car> {
    full_fixture(name).map(|path| car::Car::new(path).expect("load full fixture car"))
}

pub(crate) fn fetch_csiitem_with_encoding<'a>(
    car: &'a car::Car,
    facet_name: &str,
    encoding: car::Encoding,
) -> Option<&'a car::CSIItem> {
    car.facet_with_name(facet_name)
        .and_then(|facet| car.rendtions_with_facet(facet))
        .and_then(|items| items.iter().find(|item| item.header().encoding == encoding))
}
