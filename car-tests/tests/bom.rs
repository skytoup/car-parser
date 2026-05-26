mod util;

use std::{
    fs::File,
    io::{Seek, SeekFrom},
};

use bom::raw::{StoreHeader, TreeHeader, TreePaths};
use deku::prelude::*;

use util::{TestResult, smoke_fixture};

#[test]
fn parse_tree() -> TestResult {
    let mut file = File::options()
        .read(true)
        .open(smoke_fixture("Assets.car"))?;
    let (_, store_header) = StoreHeader::from_reader((&mut file, 0))?;

    let tree_index = store_header
        .index_with_name("APPEARANCEKEYS".as_bytes())
        .unwrap();

    file.seek(SeekFrom::Start(tree_index.offset as u64))
        .unwrap();
    let (_, tree_header) = TreeHeader::from_reader((&mut file, 0))?;
    let tree_path_index = store_header
        .index_store
        .indexs
        .get(tree_header.index as usize)
        .unwrap();

    file.seek(SeekFrom::Start(tree_path_index.offset as u64))
        .unwrap();
    let (_, tree_paths) = TreePaths::from_reader((&mut file, 0)).unwrap();

    assert_eq!(1, tree_paths.is_leaf);

    println!("{:?}", tree_paths.indices);

    Ok(())
}

#[test]
fn parse_store() -> TestResult {
    let ver = 1_u32;
    let indexes_len = 17776_usize;
    let var_names = vec![
        "CARHEADER",
        "RENDITIONS",
        "FACETKEYS",
        "APPEARANCEKEYS",
        "KEYFORMAT",
        "EXTENDED_METADATA",
        "BITMAPKEYS",
    ];
    let mut file = File::options()
        .read(true)
        .open(smoke_fixture("Assets.car"))?;

    let (_, store_header) = StoreHeader::from_reader((&mut file, 0))?;

    assert_eq!(ver, store_header.version);
    assert_eq!(indexes_len, store_header.index_store.indexs.len());
    for var_name in var_names {
        store_header
            .index_with_name(var_name.as_bytes())
            .expect("Can not find var name in store header!");
    }

    Ok(())
}
