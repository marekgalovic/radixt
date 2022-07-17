use radix_tree::map::RadixMap;

fn main() {
    let mut map = RadixMap::new();

    for i in 0..1000000_u32 {
        map.insert(i.to_be_bytes().as_slice(), i);
    }

    println!("N: {}", map.len())
}
