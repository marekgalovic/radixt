use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rand::Rng;

use radixt::RadixMap;

const SIZES: &[usize] = &[10000, 100000, 1000000];
const KEY_LENS: &[usize] = &[8, 32, 128];

fn generate_kv<R: Rng>(size: usize, rng: &mut R) -> Vec<(Vec<u8>, u64)> {
    let max_key_len = *KEY_LENS.iter().max().unwrap();
    (0..size)
        .map(|_| {
            let key = (0..max_key_len).map(|_| rng.gen::<u8>()).collect();
            let value = rng.gen::<u64>();
            (key, value)
        })
        .collect()
}

fn bench_insert(c: &mut Criterion) {
    let mut rng = rand::thread_rng();

    for &size in SIZES {
        let kv = generate_kv(size, &mut rng);

        for &key_len in KEY_LENS {
            let mut map = RadixMap::new();

            let mut kv_it = kv.iter().cycle();
            c.bench_function(&format!("insert_n={},key_len={}", size, key_len), |bench| {
                bench.iter(|| {
                    let (key, value) = kv_it.next().unwrap();
                    map.insert(&key[..key_len], *value);
                })
            });
        }
    }
}

fn bench_get(c: &mut Criterion) {
    let mut rng = rand::thread_rng();
    for &size in SIZES {
        let kv = generate_kv(size, &mut rng);

        for &key_len in KEY_LENS {
            let mut map = RadixMap::new();
            for (k, v) in kv.iter() {
                map.insert(&k[..key_len], *v);
            }

            c.bench_function(&format!("get_n={},key_len={}", size, key_len), |bench| {
                bench.iter(|| {
                    let (key, _) = &kv[rng.gen::<usize>() % kv.len()];
                    black_box(map.get(&key[..key_len]))
                })
            });
        }
    }
}

fn bench_remove(c: &mut Criterion) {
    let mut rng = rand::thread_rng();
    for &size in SIZES {
        let kv = generate_kv(size, &mut rng);

        for &key_len in KEY_LENS {
            let mut map = RadixMap::new();
            for (k, v) in kv.iter() {
                map.insert(&k[..key_len], *v);
            }

            c.bench_function(&format!("remove_n={},key_len={}", size, key_len), |bench| {
                bench.iter(|| {
                    let (key, _) = &kv[rng.gen::<usize>() % kv.len()];
                    black_box(map.remove(&key[..key_len]))
                })
            });
        }
    }
}

criterion_group!(
    name = benches;
    config = Criterion::default().significance_level(0.01).sample_size(1000);
    targets = bench_get, bench_insert, bench_remove
);
criterion_main!(benches);
