//! Time a live pointer drag of one object as the canvas does it:
//! `drag_timing <file.png> [layer] [index] [mask]` (`mask`: drag that object's mask)
//! (begin_live_edit, then per mouse step move_selection_live + render_cached, then commit).
use std::time::Instant;

use firecraft_engine::{DocState, ElementRef};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("usage: drag_timing <file.png> [layer]")?;
    let layer: usize = args.next().map_or(Ok(0), |s| s.parse())?;
    let index: usize = args.next().map_or(Ok(0), |s| s.parse())?;
    let mask = args.next().is_some_and(|a| a == "mask");
    let t = Instant::now();
    let doc = firecraft_io::open(&std::fs::read(&path)?, None)?;
    println!("open {:?}", t.elapsed());
    let mut d = DocState::new(doc);
    let mut cache = firecraft_doc::RenderCache::new();
    let t = Instant::now();
    d.doc.render_cached(d.doc.current_frame, &mut cache);
    println!("first render {:?} {:?}", t.elapsed(), cache.last);
    for i in 0..3 {
        let t = Instant::now();
        let hit = d.hit_test(300.0, 300.0);
        println!("hit_test {i}: {:?} {hit:?}", t.elapsed());
    }
    let r = ElementRef::new(layer, index);
    d.selection = vec![if mask { r.mask().ok_or("no mask")? } else { r }];
    let t = Instant::now();
    d.begin_live_edit("Move");
    println!("begin_live_edit {:?}", t.elapsed());
    for i in 0..5 {
        let t = Instant::now();
        d.move_selection_live(3.0, 2.0);
        let m = t.elapsed();
        let t = Instant::now();
        d.doc.render_cached(d.doc.current_frame, &mut cache);
        println!("step {i}: move {m:?} render {:?} {:?} {:?}", t.elapsed(), cache.last, cache.dirty);
    }
    let t = Instant::now();
    d.commit_live_edit();
    println!("commit {:?}", t.elapsed());
    let t = Instant::now();
    d.doc.render_cached(d.doc.current_frame, &mut cache);
    println!("render after commit {:?} {:?}", t.elapsed(), cache.last);
    let t = Instant::now();
    let hit = d.hit_test(300.0, 300.0);
    println!("hit_test after the move: {:?} {hit:?}", t.elapsed());
    Ok(())
}
