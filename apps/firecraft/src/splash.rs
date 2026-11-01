//! Startup splash, drawn entirely in software (no GPU), shown while the GPU renderer starts.
//!
//! GPU start-up can take several seconds (e.g. in virtual machines), so `main` re-launches the
//! executable as `firecraft --splash` before creating the real window. The splash process exits
//! when its stdin closes (the main window has drawn), when the main process dies, or after a
//! timeout. It stays up at least [`MIN_SHOW`] (above the main window) so a fast start on a real
//! GPU doesn't flash it. Disable with `FIRECRAFT_NO_SPLASH=1`.

use std::io::Read;
use std::num::NonZeroU32;
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::time::Duration;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId, WindowLevel};

const W: u32 = 460;
const H: u32 = 280;
/// The splash never outlives this, even if nothing tells it to close.
const TIMEOUT: Duration = Duration::from_secs(45);
/// Shortest time the splash is shown, so it doesn't flicker on fast machines.
const MIN_SHOW: Duration = Duration::from_secs(2);
/// Longest the app waits for the splash to paint before starting anyway.
const READY_TIMEOUT: Duration = Duration::from_millis(1500);

/// Start the splash process (best effort: any failure just means no splash).
pub fn spawn() -> Option<Child> {
    if std::env::var_os("FIRECRAFT_NO_SPLASH").is_some() {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    // Debug builds show the splash's errors in the terminal; release builds keep it quiet.
    let stderr = if cfg!(debug_assertions) { Stdio::inherit() } else { Stdio::null() };
    let mut child =
        Command::new(exe).arg("--splash").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(stderr).spawn().ok()?;
    // Wait until the splash has been painted before the caller starts the GPU: initialising some
    // GPU drivers (notably in VMs) stalls the display server for seconds, and a splash that hasn't
    // painted yet would stay invisible until it's over.
    if let Some(mut out) = child.stdout.take() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut b = [0u8; 1];
            let _ = tx.send(out.read(&mut b).is_ok_and(|n| n > 0));
        });
        if rx.recv_timeout(READY_TIMEOUT).unwrap_or(false) {
            // Give the compositor a moment to put the frame on screen.
            std::thread::sleep(Duration::from_millis(150));
        }
    }
    Some(child)
}

/// Ask the splash to close. It exits by itself once it has been up for [`MIN_SHOW`]; a background
/// thread reaps it so the caller never blocks.
pub fn close(mut child: Child) {
    drop(child.stdin.take());
    std::thread::spawn(move || {
        // Guard against a splash that ignores its closed stdin.
        for _ in 0..50 {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
        let _ = child.wait();
    });
}

/// Entry point of `firecraft --splash`.
pub fn run() {
    let started = std::time::Instant::now();
    // Exit when the parent closes our stdin (or dies), but not before MIN_SHOW; or at the timeout.
    std::thread::spawn(move || {
        let mut buf = [0u8; 64];
        let mut stdin = std::io::stdin();
        while matches!(stdin.read(&mut buf), Ok(n) if n > 0) {}
        if std::env::var_os("FIRECRAFT_SPLASH_TRACE").is_some() {
            eprintln!("[splash] stdin closed after {:?}", started.elapsed());
        }
        std::thread::sleep(MIN_SHOW.saturating_sub(started.elapsed()));
        std::process::exit(0);
    });
    std::thread::spawn(|| {
        std::thread::sleep(TIMEOUT);
        std::process::exit(0);
    });
    let Ok(event_loop) = EventLoop::new() else { return };
    let mut app = Splash { window: None, surface: None, pixels: render(), announced: false };
    let _ = event_loop.run_app(&mut app);
}

type Surface = softbuffer::Surface<Rc<Window>, Rc<Window>>;

struct Splash {
    window: Option<Rc<Window>>,
    surface: Option<Surface>,
    pixels: Vec<u32>,
    /// Sent the "painted" byte to the app.
    announced: bool,
}

impl ApplicationHandler for Splash {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let mut attrs = Window::default_attributes()
            .with_title("FireCraft is starting")
            .with_decorations(false)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_inner_size(PhysicalSize::new(W, H));
        if let Some(m) = el.primary_monitor() {
            let (s, p) = (m.size(), m.position());
            attrs = attrs.with_position(PhysicalPosition::new(
                p.x + (s.width as i32 - W as i32) / 2,
                p.y + (s.height as i32 - H as i32) / 2,
            ));
        }
        let Ok(window) = el.create_window(attrs) else { return el.exit() };
        let window = Rc::new(window);
        let Ok(context) = softbuffer::Context::new(window.clone()) else { return el.exit() };
        let Ok(surface) = softbuffer::Surface::new(&context, window.clone()) else { return el.exit() };
        window.request_redraw();
        self.window = Some(window);
        self.surface = Some(surface);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::RedrawRequested => {
                let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else { return };
                let size = window.inner_size();
                let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else { return };
                if surface.resize(w, h).is_err() {
                    return;
                }
                let Ok(mut buffer) = surface.buffer_mut() else { return };
                for (y, row) in buffer.chunks_mut(size.width as usize).enumerate() {
                    for (x, px) in row.iter_mut().enumerate() {
                        let src = if (x as u32) < W && (y as u32) < H { self.pixels.get(y * W as usize + x) } else { None };
                        *px = src.copied().unwrap_or(0);
                    }
                }
                if buffer.present().is_ok() && !self.announced {
                    // Tell the app we're on screen (it waits for this before starting the GPU).
                    self.announced = true;
                    let mut out = std::io::stdout();
                    let _ = std::io::Write::write_all(&mut out, b"r");
                    let _ = std::io::Write::flush(&mut out);
                }
            }
            _ => {}
        }
    }
}

fn rgb(r: f32, g: f32, b: f32) -> u32 {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    (c(r) << 16) | (c(g) << 8) | c(b)
}

fn unpack(p: u32) -> [f32; 3] {
    [((p >> 16) & 255) as f32 / 255.0, ((p >> 8) & 255) as f32 / 255.0, (p & 255) as f32 / 255.0]
}

/// Blend colour `c` onto `buf` at (x, y) with coverage `a`.
fn plot(buf: &mut [u32], x: i32, y: i32, c: [f32; 3], a: f32) {
    if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 || a <= 0.0 {
        return;
    }
    let Some(p) = buf.get_mut((y as u32 * W + x as u32) as usize) else { return };
    let d = unpack(*p);
    let a = a.min(1.0);
    *p = rgb(d[0] + (c[0] - d[0]) * a, d[1] + (c[1] - d[1]) * a, d[2] + (c[2] - d[2]) * a);
}

fn text(buf: &mut [u32], font: &FontRef, s: &str, size: f32, x: f32, baseline: f32, c: [f32; 3]) {
    let scaled = font.as_scaled(PxScale::from(size));
    let mut pen = x;
    let mut prev = None;
    for ch in s.chars() {
        let id = scaled.glyph_id(ch);
        if let Some(p) = prev {
            pen += scaled.kern(p, id);
        }
        let glyph = id.with_scale_and_position(PxScale::from(size), ab_glyph::point(pen, baseline));
        pen += scaled.h_advance(id);
        prev = Some(id);
        if let Some(outline) = font.outline_glyph(glyph) {
            let b = outline.px_bounds();
            outline.draw(|gx, gy, cov| plot(buf, b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32, c, cov));
        }
    }
}

/// The splash image (0RGB pixels, `W` x `H`).
fn render() -> Vec<u32> {
    let mut buf = vec![0u32; (W * H) as usize];
    // Night-sky gradient.
    for y in 0..H {
        for x in 0..W {
            let t = y as f32 / H as f32;
            let v = x as f32 / W as f32 * 0.04;
            buf[(y * W + x) as usize] = rgb(0.05 + 0.10 * t + v, 0.05 + 0.04 * t, 0.16 + 0.10 * t);
        }
    }
    // Firework bursts: rays of fading sparks.
    let bursts: [(f32, f32, f32, [f32; 3]); 3] =
        [(345.0, 95.0, 70.0, [1.0, 0.62, 0.15]), (405.0, 175.0, 38.0, [1.0, 0.85, 0.35]), (290.0, 185.0, 30.0, [0.95, 0.35, 0.45])];
    for (cx, cy, r, c) in bursts {
        for k in 0..28 {
            let a = k as f32 / 28.0 * std::f32::consts::TAU;
            for step in 0..(r as i32) {
                let d = step as f32;
                let fade = 1.0 - d / r;
                if step % 3 == 0 {
                    let (x, y) = (cx + a.cos() * d, cy + a.sin() * d + d * d / (r * 6.0));
                    for (ox, oy, w) in [(0, 0, 1.0), (1, 0, 0.4), (0, 1, 0.4), (-1, 0, 0.4), (0, -1, 0.4)] {
                        plot(&mut buf, x as i32 + ox, y as i32 + oy, c, fade * w);
                    }
                }
            }
        }
    }
    if let Ok(font) = FontRef::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT) {
        text(&mut buf, &font, "FireCraft", 52.0, 28.0, 150.0, [1.0, 1.0, 1.0]);
        text(&mut buf, &font, &format!("Version {}", env!("CARGO_PKG_VERSION")), 15.0, 32.0, 178.0, [0.8, 0.8, 0.9]);
        text(&mut buf, &font, "Starting up...", 13.0, 32.0, 236.0, [1.0, 0.75, 0.4]);
        text(&mut buf, &font, "Open source web graphics editor  |  MIT OR Apache-2.0", 11.0, 32.0, 262.0, [0.6, 0.6, 0.72]);
    }
    // Thin frame.
    for x in 0..W {
        plot(&mut buf, x as i32, 0, [0.4, 0.4, 0.5], 1.0);
        plot(&mut buf, x as i32, H as i32 - 1, [0.4, 0.4, 0.5], 1.0);
    }
    for y in 0..H {
        plot(&mut buf, 0, y as i32, [0.4, 0.4, 0.5], 1.0);
        plot(&mut buf, W as i32 - 1, y as i32, [0.4, 0.4, 0.5], 1.0);
    }
    buf
}

#[cfg(test)]
mod tests {
    #[test]
    fn renders_without_panicking() {
        let px = super::render();
        assert_eq!(px.len(), (super::W * super::H) as usize);
        assert!(px.iter().any(|&p| p == 0xFF_FF_FF), "title text drawn");
    }
}
