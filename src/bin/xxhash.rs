use std::io;
use std::io::Read;
use mtl::hash;

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut input = stdin.lock();

    let mut line = String::new();
    input.read_to_string(&mut line)?;

    println!("xxh64: {:x}", hash::xxh64_contents(&line));
    println!("xxh3 : {:x}", hash::xxh3_contents(&line));
    Ok(())
}