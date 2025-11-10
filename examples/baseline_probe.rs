use fontdue::{Font, FontSettings};

fn dump(font: &Font, size: f32, ch: char, baseline: f32) {
    let (metrics, _) = font.rasterize(ch, size);
    println!(
        "glyph '{}': xmin={}, ymin={}, width={}, height={}, advance_width={}, advance_height={}",
        ch, metrics.xmin, metrics.ymin, metrics.width, metrics.height, metrics.advance_width, metrics.advance_height
    );
    let top = baseline - metrics.ymin as f32;
    let bottom = baseline - (metrics.ymin as f32 + metrics.height as f32);
    println!("    top_above_baseline={:.3}, bottom_above_baseline={:.3}", top, bottom);
}

fn main() {
    let font_bytes = std::fs::read("assets/NORMAL.ttf").expect("failed to load NORMAL.ttf");
    let font = Font::from_bytes(font_bytes, FontSettings::default()).expect("invalid font");
    let size = 12.0;

    if let Some(metrics) = font.horizontal_line_metrics(size) {
        println!(
            "line metrics: ascent={:.3}, descent={:.3}, line_gap={:.3}, new_line={:.3}",
            metrics.ascent, metrics.descent, metrics.line_gap, metrics.new_line_size
        );
    }

    let baseline = font.horizontal_line_metrics(size).map(|m| m.ascent).unwrap_or(size);
    println!("baseline(ascent) = {:.3}", baseline);
    dump(&font, size, 'h', baseline);
    dump(&font, size, 'g', baseline);
}
