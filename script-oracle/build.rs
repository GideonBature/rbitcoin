use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo = manifest.join("..");
    let submodule = repo.join("third_party/bitcoin");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("core");
    let src = out.join("src");
    if !src.join("script/interpreter.cpp").is_file() {
        std::fs::create_dir_all(&out).unwrap();
        if !archive(&submodule, &out) {
            let init = Command::new(repo.join("scripts/core-functional/init-submodule.sh"))
                .status()
                .expect("run init-submodule.sh");
            assert!(init.success(), "init-submodule.sh failed");
            let add = Command::new("git")
                .args(["-C"])
                .arg(&submodule)
                .args(["sparse-checkout", "add", "src"])
                .status()
                .expect("sparse-checkout");
            assert!(add.success(), "sparse-checkout add src failed");
            let checkout = Command::new("git")
                .args(["-C"])
                .arg(&submodule)
                .args(["checkout", "HEAD", "--", "src"])
                .status()
                .expect("checkout src");
            assert!(checkout.success(), "checkout src failed");
            assert!(archive(&submodule, &out), "git archive of Core src failed");
        }
    }
    assert!(
        src.join("script/interpreter.cpp").is_file(),
        "missing {}",
        src.join("script/interpreter.cpp").display()
    );

    cc::Build::new()
        .cpp(true)
        .std("c++20")
        .include(&src)
        .include(src.join("secp256k1/include"))
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-implicit-fallthrough")
        .file(manifest.join("verify.cpp"))
        .file(src.join("script/interpreter.cpp"))
        .file(src.join("script/script.cpp"))
        .file(src.join("script/script_error.cpp"))
        .file(src.join("primitives/transaction.cpp"))
        .file(src.join("pubkey.cpp"))
        .file(src.join("hash.cpp"))
        .file(src.join("uint256.cpp"))
        .file(src.join("arith_uint256.cpp"))
        .file(src.join("crypto/ripemd160.cpp"))
        .file(src.join("crypto/sha1.cpp"))
        .file(src.join("crypto/sha256.cpp"))
        .file(src.join("crypto/sha512.cpp"))
        .file(src.join("crypto/hmac_sha512.cpp"))
        .file(src.join("crypto/hex_base.cpp"))
        .file(src.join("util/strencodings.cpp"))
        .file(src.join("streams.cpp"))
        .file(src.join("support/cleanse.cpp"))
        .compile("coreverify");

    cc::Build::new()
        .include(src.join("secp256k1/include"))
        .include(src.join("secp256k1/src"))
        .define("ENABLE_MODULE_SCHNORRSIG", "1")
        .define("ENABLE_MODULE_EXTRAKEYS", "1")
        .define("ENABLE_MODULE_ELLSWIFT", "1")
        .define("ENABLE_MODULE_RECOVERY", "1")
        .define("ECMULT_WINDOW_SIZE", "15")
        .define("ECMULT_GEN_PREC_BITS", "4")
        .define("USE_FIELD_5X52", "1")
        .define("USE_SCALAR_4X64", "1")
        .define("HAVE___INT128", "1")
        .flag_if_supported("-Wno-unused-function")
        .file(src.join("secp256k1/src/secp256k1.c"))
        .file(src.join("secp256k1/src/precomputed_ecmult.c"))
        .file(src.join("secp256k1/src/precomputed_ecmult_gen.c"))
        .compile("secp256k1oracle");

    println!("cargo:rerun-if-changed=verify.cpp");
    println!("cargo:rerun-if-changed=build.rs");
}

fn archive(submodule: &Path, out: &Path) -> bool {
    let archived = Command::new("git")
        .args(["-C"])
        .arg(submodule)
        .args(["archive", "HEAD", "src"])
        .output()
        .expect("git archive");
    if !archived.status.success() || archived.stdout.is_empty() {
        return false;
    }
    let mut tar = Command::new("tar")
        .args(["-C"])
        .arg(out)
        .arg("-x")
        .stdin(Stdio::piped())
        .spawn()
        .expect("tar");
    tar.stdin
        .take()
        .unwrap()
        .write_all(&archived.stdout)
        .unwrap();
    tar.wait().expect("tar wait").success()
}
