//! Cross-implementation correctness tests:
//! 1. ruzstd compress → C zstd decompress → compare
//! 2. C zstd compress → ruzstd decompress → compare
//! 3. Round-trip identity for various data patterns

use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;

fn gen_text(size: usize) -> Vec<u8> {
    let words: [&[u8]; 16] = [
        b"the ", b"of ", b"and ", b"to ", b"in ", b"that ",
        b"he ", b"was ", b"it ", b"his ", b"with ", b"is ",
        b"for ", b"as ", b"had ", b"be ",
    ];
    let mut rng = SmallRng::seed_from_u64(42);
    let mut buf = Vec::with_capacity(size);
    while buf.len() < size {
        buf.extend_from_slice(words[rng.gen_range(0..words.len())]);
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

fn gen_rle(size: usize) -> Vec<u8> {
    vec![0xAB; size]
}

fn gen_mixed(size: usize) -> Vec<u8> {
    // Alternating compressible and incompressible chunks
    let mut rng = SmallRng::seed_from_u64(99);
    let mut buf = Vec::with_capacity(size);
    while buf.len() < size {
        // 64KB compressible
        for _ in 0..65536.min(size - buf.len()) {
            buf.push(rng.gen_range(0..16));
        }
        // 64KB random
        for _ in 0..65536.min(size - buf.len()) {
            buf.push(rng.gen());
        }
    }
    buf.truncate(size);
    buf
}

fn gen_structured(size: usize) -> Vec<u8> {
    // Repeating structured pattern with offsets
    let mut buf = Vec::with_capacity(size);
    let mut i: u32 = 0;
    while buf.len() < size {
        buf.extend_from_slice(&i.to_le_bytes());
        buf.extend_from_slice(&[0u8; 12]);
        i += 1;
    }
    buf.truncate(size);
    buf
}

struct TestCase {
    name: &'static str,
    data: Vec<u8>,
}

fn main() {
    let sizes = [0, 1, 5, 255, 256, 1024, 65536, 131072, 1048576, 4194304];
    let mut failures = 0u32;
    let mut passes = 0u32;

    println!("=== CROSS-IMPLEMENTATION CORRECTNESS TESTS ===\n");

    for &size in &sizes {
        let cases = vec![
            TestCase { name: "text", data: gen_text(size) },
            TestCase { name: "random", data: gen_random(size) },
            TestCase { name: "rle", data: gen_rle(size) },
            TestCase { name: "mixed", data: gen_mixed(size) },
            TestCase { name: "structured", data: gen_structured(size) },
        ];

        for case in &cases {
            let data = &case.data;

            // Test 1: C compress → ruzstd decompress
            {
                let c_compressed = zstd::encode_all(&data[..], 1).expect("C compress L1");
                let mut dec = ruzstd::decoding::FrameDecoder::new();
                let mut out = vec![0u8; data.len()];
                match dec.decode_all(&c_compressed, &mut out) {
                    Ok(()) => {
                        if &out[..] == &data[..] {
                            passes += 1;
                        } else {
                            println!("FAIL: C→ruzstd {} {}B: output mismatch", case.name, size);
                            failures += 1;
                        }
                    }
                    Err(e) => {
                        println!("FAIL: C→ruzstd {} {}B: decode error: {:?}", case.name, size, e);
                        failures += 1;
                    }
                }
            }

            // Test 2: ruzstd compress (Fastest) → C decompress
            {
                let mut rs_compressed = Vec::new();
                ruzstd::encoding::compress(&data[..], &mut rs_compressed, ruzstd::encoding::CompressionLevel::Fastest);
                match zstd::decode_all(&rs_compressed[..]) {
                    Ok(decoded) => {
                        if decoded == *data {
                            passes += 1;
                        } else {
                            println!("FAIL: ruzstd→C {} {}B: output mismatch (got {}B, want {}B)",
                                case.name, size, decoded.len(), data.len());
                            failures += 1;
                        }
                    }
                    Err(e) => {
                        println!("FAIL: ruzstd→C {} {}B: C decode error: {}", case.name, size, e);
                        failures += 1;
                    }
                }
            }

            // Test 3: ruzstd compress (Uncompressed) → C decompress
            {
                let mut rs_compressed = Vec::new();
                ruzstd::encoding::compress(&data[..], &mut rs_compressed, ruzstd::encoding::CompressionLevel::Uncompressed);
                match zstd::decode_all(&rs_compressed[..]) {
                    Ok(decoded) => {
                        if decoded == *data {
                            passes += 1;
                        } else {
                            println!("FAIL: ruzstd(raw)→C {} {}B: output mismatch", case.name, size);
                            failures += 1;
                        }
                    }
                    Err(e) => {
                        println!("FAIL: ruzstd(raw)→C {} {}B: C decode error: {}", case.name, size, e);
                        failures += 1;
                    }
                }
            }

            // Test 4: C compress L3 → ruzstd decompress
            {
                let c_compressed = zstd::encode_all(&data[..], 3).expect("C compress L3");
                let mut dec = ruzstd::decoding::FrameDecoder::new();
                let mut out = vec![0u8; data.len()];
                match dec.decode_all(&c_compressed, &mut out) {
                    Ok(()) => {
                        if &out[..] == &data[..] {
                            passes += 1;
                        } else {
                            println!("FAIL: C(L3)→ruzstd {} {}B: output mismatch", case.name, size);
                            failures += 1;
                        }
                    }
                    Err(e) => {
                        println!("FAIL: C(L3)→ruzstd {} {}B: decode error: {:?}", case.name, size, e);
                        failures += 1;
                    }
                }
            }

            // Test 5: ruzstd compress → ruzstd decompress (self round-trip)
            {
                let mut rs_compressed = Vec::new();
                ruzstd::encoding::compress(&data[..], &mut rs_compressed, ruzstd::encoding::CompressionLevel::Fastest);
                let mut dec = ruzstd::decoding::FrameDecoder::new();
                let mut out = vec![0u8; data.len()];
                match dec.decode_all(&rs_compressed, &mut out) {
                    Ok(()) => {
                        if &out[..] == &data[..] {
                            passes += 1;
                        } else {
                            println!("FAIL: ruzstd→ruzstd {} {}B: output mismatch", case.name, size);
                            failures += 1;
                        }
                    }
                    Err(e) => {
                        println!("FAIL: ruzstd→ruzstd {} {}B: decode error: {:?}", case.name, size, e);
                        failures += 1;
                    }
                }
            }
        }
    }

    println!("\n=== RESULTS: {} passed, {} failed ===", passes, failures);
    if failures > 0 {
        std::process::exit(1);
    }
}
