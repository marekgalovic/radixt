use std::collections::{BTreeSet, HashSet};
use std::io::BufRead;

use radixt::RadixSet;

fn main() {
    let container = std::env::args().nth(1).unwrap();
    println!("Container: {}", container);

    match container.as_ref() {
        "radix" => {
            let mut set = RadixSet::new();
            each_line(|line| {
                set.insert(line);
            });
            println!("# LINES: {}", set.len());
        }
        "hash" => {
            let mut set = HashSet::new();
            each_line(|line| {
                set.insert(line);
            });
            println!("# LINES: {}", set.len());
        }
        "btree" => {
            let mut set = BTreeSet::new();
            each_line(|line| {
                set.insert(line);
            });
            println!("# LINES: {}", set.len());
        }
        "count" => {
            let mut count = 0;
            each_line(|_| {
                count += 1;
            });
            println!("# LINES: {}", count);
        }
        _ => unreachable!(),
    }
}

fn each_line<F>(mut f: F)
where
    F: FnMut(String),
{
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        f(line.unwrap());
    }
}
