[![Crates](https://img.shields.io/crates/v/radixt.svg)](https://crates.io/crates/radixt)
[![Docs](https://docs.rs/radixt/badge.svg)](https://docs.rs/radixt)
[![CI](https://github.com/marekgalovic/radix_tree_rs/actions/workflows/ci.yml/badge.svg)](https://github.com/marekgalovic/radixt/actions)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

# Radix Tree RS
A fast, memory-efficient radix tree implementation in Rust.

## Examples
```rust
use radixt::RadixMap;

let mut map = RadixMap::new();

map.insert("bar", 1);
map.insert("baz", 2);
map.insert("foo", 2);

assert_eq!(map.len(), 3);

assert_eq!(map.get("bar"), Some(&1));
assert_eq!(map.get("baz"), Some(&2));
assert_eq!(map.get("foo"), Some(&3));
```

## Benchmarks
|              | **enwiki-latest-all-titles** |                  | **googlebooks-eng-all-5gram** |                  |
|--------------|------------------------------|------------------|-------------------------------|------------------|
|              | **Time (s)**                 | **Max RSS (MB)** | **Time (s)**                  | **Max RSS (MB)** |
| **RadixSet** |                         5.74 |           558.65 |                          3.31 |           283.25 |
| **HashSet**  |                         6.37 |          2062.06 |                          3.62 |          1205.06 |
| **BTreeSet** |                         6.05 |          1276.07 |                          2.76 |           867.42 |

The results above were obtained using `examples/insert_lines.rs` with data downloaded using:
```bash
# enwiki-latest-all-titles
$ curl -s https://dumps.wikimedia.org/enwiki/latest/enwiki-latest-all-titles-in-ns0.gz | gzip -d > enwiki-latest-all-titles-in-ns0
# googlebooks-eng-all-5gram
$ curl -s http://storage.googleapis.com/books/ngrams/books/googlebooks-eng-all-5gram-20120701-0.gz | gzip -d > googlebooks-eng-all-5gram-20120701-0
```