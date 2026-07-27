use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;
use std::time::Instant;

fn gen_compressible(size: usize) -> Vec<u8> {
    let words: [&[u8]; 32] = [
        b"the ", b"of ", b"and ", b"to ", b"in ", b"that ",
        b"he ", b"was ", b"it ", b"his ", b"with ", b"is ", b"for ",
        b"as ", b"had ", b"be ", b"not ", b"on ", b"but ", b"at ",
        b"by ", b"an ", b"are ", b"from ", b"or ", b"which ", b"this ",
        b"also ", b"been ", b"has ", b"were ", b"they ",
    ];
    let mut rng = SmallRng::seed_from_u64(42);
    let mut buf = Vec::with_capacity(size);
    while buf.len() < size {
        let w = words[rng.gen_range(0..words.len())];
        buf.extend_from_slice(w);
        if rng.gen_bool(0.08) { buf.push(b'\n'); }
    }
    buf.truncate(size);
    buf
}

fn gen_medium(size: usize) -> Vec<u8> {
    let mut rng = SmallRng::seed_from_u64(99);
    let mut buf = Vec::with_capacity(size);
    while buf.len() < size {
        let tag: u32 = rng.gen_range(0..64);
        let len: u32 = rng.gen_range(8..256);
        buf.extend_from_slice(&tag.to_le_bytes());
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]);
        for _ in 0..len {
            if rng.gen_bool(0.6) { buf.push(rng.gen_range(0..16)); }
            else { buf.push(rng.gen()); }
        }
    }
    buf.truncate(size);
    buf
}

fn gen_random(size: usize) -> Vec<u8> {
    let mut rng = SmallRng::seed_from_u64(7);
    let mut buf = vec![0u8; size];
    rng.fill(&mut buf[..]);
    buf
}

struct BenchResult {
    name: String,
    data_size: usize,
    compressed_size: usize,
    compress_mb_s: f64,
    decompress_mb_s: f64,
    ratio: f64,
}

fn bench_pair(name: &str, data: &[u8], level: i32, iterations: u32) -> (BenchResult, BenchResult) {
    let c_compressed = zstd::encode_all(data, level).expect("C compress");
    let mut best_c_comp = f64::MAX;
    let mut best_c_decomp = f64::MAX;
    for _ in 0..iterations {
        let t = Instant::now();
        let _ = zstd::encode_all(data, level).unwrap();
        best_c_comp = best_c_comp.min(t.elapsed().as_secs_f64());
        let t = Instant::now();
        let _ = zstd::decode_all(&c_compressed[..]).unwrap();
        best_c_decomp = best_c_decomp.min(t.elapsed().as_secs_f64());
    }

    let mut rs_compressed = Vec::new();
    ruzstd::encoding::compress(data, &mut rs_compressed, ruzstd::encoding::CompressionLevel::Fastest)
        .expect("Rust compress");
    let mut best_rs_comp = f64::MAX;
    let mut best_rs_decomp = f64::MAX;
    for _ in 0..iterations {
        let t = Instant::now();
        let mut out = Vec::new();
        ruzstd::encoding::compress(data, &mut out, ruzstd::encoding::CompressionLevel::Fastest).unwrap();
        best_rs_comp = best_rs_comp.min(t.elapsed().as_secs_f64());
        let t = Instant::now();
        let mut dec = ruzstd::decoding::FrameDecoder::new();
        let mut out = Vec::with_capacity(data.len());
        dec.decode_all(&c_compressed, &mut out).unwrap();
        best_rs_decomp = best_rs_decomp.min(t.elapsed().as_secs_f64());
    }

    let mb = data.len() as f64 / (1024.0 * 1024.0);
    let c_res = BenchResult {
        name: format!("{} (C zstd L{})", name, level),
        data_size: data.len(),
        compressed_size: c_compressed.len(),
        compress_mb_s: mb / best_c_comp,
        decompress_mb_s: mb / best_c_decomp,
        ratio: data.len() as f64 / c_compressed.len() as f64,
    };
    let rs_res = BenchResult {
        name: format!("{} (ruzstd Fastest)", name),
        data_size: data.len(),
        compressed_size: rs_compressed.len(),
        compress_mb_s: mb / best_rs_comp,
        decompress_mb_s: mb / best_rs_decomp,
        ratio: data.len() as f64 / rs_compressed.len() as f64,
    };
    (c_res, rs_res)
}

fn print_result(r: &BenchResult) {
    println!(
        "  {:<36} ratio {:>6.2}  comp {:>8.1} MB/s  decomp {:>8.1} MB/s  ({} -> {} bytes)",
        r.name, r.ratio, r.compress_mb_s, r.decompress_mb_s, r.data_size, r.compressed_size
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let size: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(16 * 1024 * 1024);
    let iterations: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);

    println!("=== zstd C vs ruzstd (pure Rust) benchmark ===");
    println!("Data size: {} MB, iterations: {} (best-of)\n", size / 1024 / 1024, iterations);

    let datasets: Vec<(&str, Vec<u8>)> = vec![
        ("text-compressible", gen_compressible(size)),
        ("binary-medium", gen_medium(size)),
        ("random-incompressible", gen_random(size)),
    ];

    let levels = [1, 3];

    for (dname, data) in &datasets {
        println!("-- {} --", dname);
        for &level in &levels {
            let (c, rs) = bench_pair(dname, data, level, iterations);
            print_result(&c);
            print_result(&rs);
            println!(
                "  -> decomp slowdown: {:.2}x   comp slowdown: {:.2}x   ratio loss: {:.1}%\n",
                c.decompress_mb_s / rs.decompress_mb_s,
                c.compress_mb_s / rs.compress_mb_s,
                (1.0 - rs.ratio / c.ratio) * 100.0,
            );
        }
    }
}
