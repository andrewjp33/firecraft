//! Exercise the system clipboard: sysclip_probe get OUT | sysclip_probe set FILE SECONDS
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("get") => {
            let data = firecraft_sysclip::get_png().ok_or("no PNG on the clipboard")?;
            std::fs::write(a.get(2).ok_or("OUT")?, &data)?;
            println!("got {} bytes", data.len());
        }
        Some("set") => {
            let data = std::fs::read(a.get(2).ok_or("FILE")?)?;
            let secs: u64 = a.get(3).map_or(Ok(20), |s| s.parse())?;
            println!(
                "set {} bytes: {} (owns: {})",
                data.len(),
                firecraft_sysclip::set_png(data),
                firecraft_sysclip::owns()
            );
            std::thread::sleep(std::time::Duration::from_secs(secs));
            println!("still owner: {}", firecraft_sysclip::owns());
        }
        _ => return Err("usage: sysclip_probe get OUT | set FILE SECONDS".into()),
    }
    Ok(())
}
