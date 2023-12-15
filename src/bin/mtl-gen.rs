use std::io::BufRead;
use std::io::Write;

use rand::{Rng, SeedableRng};
use sha1::Digest;

struct Hash {
    inner: [u8; 20],
}

impl From<[u8; 20]> for Hash {
    fn from(inner: [u8; 20]) -> Self {
        Self { inner }
    }
}

impl std::fmt::Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.inner.iter() {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

fn hash_content(content: &[u8]) -> std::io::Result<Hash> {
    let mut hasher = sha1::Sha1::new();
    hasher.write(content)?;
    let sha1 = hasher.finalize().into();
    Ok(Hash { inner: sha1 })
}

fn read_file<P: AsRef<std::path::Path>>(path: P) -> std::io::Result<Vec<String>> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);

    // read by lines
    let mut lines = Vec::new();
    for line in reader.lines() {
        lines.push(line?);
    }

    Ok(lines)
}

fn main() -> std::io::Result<()> {
    let dir = std::env::args().nth(1).unwrap();
    let nfile = std::env::args().nth(2).unwrap().parse::<usize>().unwrap();
    let seed = std::env::args()
        .nth(3)
        .unwrap_or("1234".to_string())
        .parse::<u64>()
        .unwrap();

    let dir = std::path::Path::new(&dir);
    let lines = read_file("/usr/share/dict/words")?;
    let len = lines.len();

    println!("seed: {}", seed);
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    for _i in 0..nfile {
        // generate random range
        let n1 = rng.gen_range(0..len);
        let n2 = rng.gen_range(n1..std::cmp::min(n1 + 10000, len));

        let lines = (&lines[n1..n2]).join("\n");
        let lines = lines.as_bytes();
        let hash = hash_content(lines)?;

        let hash = hash.to_string();

        let (prefix, rest) = hash.split_at(2);
        let path = dir.join(prefix);
        std::fs::create_dir_all(&path)?;

        let path = path.join(rest);
        let mut file = std::fs::File::create(path)?;
        file.write_all(&lines)?;
    }
    Ok(())
}
