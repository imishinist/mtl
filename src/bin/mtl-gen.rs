use std::io::BufRead;
use std::io::Write;

use rand::Rng;

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

    let dir = std::path::Path::new(&dir);
    let lines = read_file("/usr/share/dict/words")?;
    // let len = lines.len() as usize;
    let len = 1000;

    let mut rng = rand::thread_rng();
    for _i in 0..nfile {
        let mut n1 = rng.gen_range(0..len);
        let mut n2 = rng.gen_range(0..len);
        if n2 < n1 {
            std::mem::swap(&mut n1, &mut n2);
        }

        let uuid = uuid::Uuid::new_v4().to_string();
        let (prefix, rest) = uuid.split_at(2);

        let path = dir.join(prefix);
        std::fs::create_dir_all(&path)?;
        let path = path.join(rest);

        let mut file = std::fs::File::create(path)?;
        file.write_all(&lines[n1..n2].join("\n").as_bytes())?;
    }
    Ok(())
}

/*
*
#!/bin/bash

dir=$(mktemp -d)
cd $dir

echo $dir

dictfile=/usr/share/dict/words
nfile=10000

for i in $(seq 1 $nfile); do
  uuid=$(uuidgen)
  prefix=$(echo $uuid | cut -c 1-2)
  rest=$(echo $uuid | cut -c 3-)

  mkdir -p $prefix
  shuf -n $RANDOM $dictfile > $prefix/$rest
done

*/
