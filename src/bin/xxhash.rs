use mtl::hash;
use std::io;
use std::io::Read;

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut input = stdin.lock();

    let mut line = String::new();
    input.read_to_string(&mut line)?;

    println!("xxh64: {:016x}", hash::xxh64_contents(&line));
    println!("xxh3 : {:016x}", hash::xxh3_contents(&line));
    Ok(())
}
