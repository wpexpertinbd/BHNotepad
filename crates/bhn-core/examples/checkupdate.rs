// Exercises the real update check and download against the real GitHub release.
fn main() {
    let r = bhn_core::update::check();
    println!("running version : {}", r.current);
    match (&r.update, &r.error) {
        (_, Some(e)) => println!("ERROR           : {e}  (must NOT be shown as 'up to date')"),
        (None, None) => println!("result          : already the latest version"),
        (Some(u), None) => {
            println!("update found    : {}", u.version);
            println!("  asset         : {:?}", u.asset_name);
            if std::env::args().any(|a| a == "--download") {
                match bhn_core::update::download(u) {
                    Ok(path) => {
                        println!("  downloaded to : {}", path.display());
                        let parent = path.parent().unwrap();
                        println!("  in temp dir?  : {}", parent.starts_with(std::env::temp_dir()));
                        println!("  tracked path  : {:?}", bhn_core::update::last_download());
                        println!(
                            "  matches       : {}",
                            bhn_core::update::last_download().as_deref() == Some(path.as_path())
                        );
                        let meta = std::fs::metadata(&path).unwrap();
                        println!("  size on disk  : {} bytes", meta.len());
                        // Clean up after ourselves.
                        let _ = std::fs::remove_file(&path);
                        let _ = std::fs::remove_dir(parent);
                    }
                    Err(e) => println!("  download FAILED: {e}"),
                }
            }
        }
    }
}
