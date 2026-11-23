//! Exercise the system clipboard: sysclip_probe get OUT | files | set FILE SECONDS [DIB]
//! (with DIB, a `CF_DIB` file's bytes go beside the PNG as FireCraft's Copy puts them)
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("get") => {
            let data = firecraft_sysclip::get_image().ok_or("no picture on the clipboard")?;
            std::fs::write(a.get(2).ok_or("OUT")?, &data)?;
            println!("got {} bytes", data.len());
        }
        Some("files") => {
            for f in firecraft_sysclip::copied_files() {
                println!("{}", f.display());
            }
        }
        Some("set") => {
            let data = std::fs::read(a.get(2).ok_or("FILE")?)?;
            let secs: u64 = a.get(3).map_or(Ok(20), |s| s.parse())?;
            let len = data.len();
            let set = match a.get(4) {
                Some(dib) => firecraft_sysclip::set_png_and_dib(data, std::fs::read(dib)?),
                None => firecraft_sysclip::set_png(data),
            };
            println!("set {len} bytes: {set} (owns: {})", firecraft_sysclip::owns());
            std::thread::sleep(std::time::Duration::from_secs(secs));
            println!("still owner: {}", firecraft_sysclip::owns());
        }
        _ => return Err("usage: sysclip_probe get OUT | files | set FILE SECONDS [DIB]".into()),
    }
    Ok(())
}
