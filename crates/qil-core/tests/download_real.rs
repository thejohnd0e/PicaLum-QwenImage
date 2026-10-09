//! Network test against Hugging Face. Run with: cargo test -p qil-core --test download_real -- --ignored --nocapture

use qil_core::config::VAE;
use qil_core::download;

#[test]
#[ignore]
fn vae_downloads_resumes_and_verifies() {
    let dir = std::env::temp_dir().join("qil-dl-test");
    let _ = std::fs::remove_dir_all(&dir);
    let dest = dir.join(VAE.name);

    // Simulate an interrupted download: first ~50 MB into the .part file.
    let part = dest.with_extension("safetensors.part");
    std::fs::create_dir_all(&dir).unwrap();
    let head = reqwest::blocking::Client::new()
        .get(VAE.url())
        .header("Range", "bytes=0-52428799")
        .send()
        .unwrap()
        .bytes()
        .unwrap();
    std::fs::write(&part, &head).unwrap();
    assert_eq!(head.len(), 50 << 20);

    let mut last = 0;
    download::fetch(&VAE.url(), &dest, Some(VAE.size), &mut |p| {
        assert!(
            p.done >= 50 << 20,
            "resume must continue from the partial file"
        );
        last = p.done;
    })
    .unwrap();
    assert!(last > 50 << 20);
    assert_eq!(dest.metadata().unwrap().len(), VAE.size);
    let sum = download::sha256(&dest, &mut |_| {}).unwrap();
    assert_eq!(sum, VAE.sha256);
    let _ = std::fs::remove_dir_all(&dir);
}
