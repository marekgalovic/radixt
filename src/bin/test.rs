use radix_tree_rs::map::RadixMap;

fn main() {
    let mut map = RadixMap::new();

    map.insert(0_u32.to_be_bytes().as_slice(), 0);

    println!("{:?}", map);
}
