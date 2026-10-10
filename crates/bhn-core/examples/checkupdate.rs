// Exercises the real update check against the real GitHub release.
fn main() {
    let r = bhn_core::update::check();
    println!("running version : {}", r.current);
    match (&r.update, &r.error) {
        (_, Some(e)) => println!("ERROR           : {e}  (must NOT be shown as 'up to date')"),
        (Some(u), None) => {
            println!("update found    : {}", u.version);
            println!(
                "  asset         : {:?} ({} bytes)",
                u.asset_name, u.asset_size
            );
            println!("  url           : {:?}", u.asset_url);
            println!("  page          : {}", u.page_url);
        }
        (None, None) => println!("result          : already the latest version"),
    }
}
