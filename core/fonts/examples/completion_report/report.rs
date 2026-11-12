//! writing COMPLETION_REPORT.md from the measured rows.

use super::*;

pub(crate) fn write_report(rows: &[Row], font_count: usize) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Font completion: how seamless is added text?\n");
    let _ = writeln!(out, "Made by `cargo run --release -p letters-fonts --example completion_report -- core/fonts/COMPLETION_REPORT.md` on this computer, with {font_count} test fonts (the bundled open fonts in `vendor/fonts` and fonts installed in Windows, read in place as test subjects only).\n");
    let _ = writeln!(out, "Each test font is cut down to a subset holding only the letters of “{FIRST}”, the way a PDF embeds it. The pipeline is then asked for “{SECOND}” (scenario e: subset “{FIRST_ACCENTS}”, new text “{SECOND_ACCENTS}”, only its accented letters measured). Only the fonts a scenario allows can lend glyphs; the full font is used only afterwards, to measure.\n");
    let _ = writeln!(out, "- **Glyph distance**: mean (and largest) outline distance between each new glyph and the true one, both at their origin, in thousandths of the em (1 ‰ is 0.05 pt at 11 pt; 0.2 pixel at 300 dpi).");
    let _ = writeln!(
        out,
        "- **Advance**: mean advance-width error of the new glyphs, ‰ of the em."
    );
    let _ = writeln!(out, "- **Line SSIM / differing**: the whole new sentence drawn at 11 pt, 300 dpi with the true font and with the completed one (the subset's glyphs plus the new ones, each at its own advances, no kerning on either side), compared pixel for pixel. SSIM is averaged over the 8×8 windows that have ink; “differing” is the share of inked pixels whose coverage differs by more than 1/8. Advance errors add up along the line, so one wrong width shifts every letter after it: this is the strict 1:1 pixel test.");
    let _ = writeln!(out, "- **Shapes SSIM / differing**: the same, with the new glyphs set at the true advances, so only their shapes and placement within their own width count.");
    let _ = writeln!(out, "- **Seam**: the worse of the shapes (invisible: shapes SSIM ≥ 0.995 and ≤ 1% differing; very hard to see: ≥ 0.97 and ≤ 8%; visible on close reading: ≥ 0.93 and ≤ 18%; plainly visible otherwise) and the spacing (mean advance error ≤ 1 ‰, ≤ 4 ‰, ≤ 10 ‰, more).\n");
    let _ = writeln!(out, "| Scenario | Target | Donors allowed | How the new glyphs were made | Lent by | Fit (held-out, before) | Glyph distance ‰ mean / max | Advance ‰ | Line SSIM / differing | Shapes SSIM / differing | Seam | Time |");
    let _ = writeln!(out, "|---|---|---|---|---|---|---|---|---|---|---|---|");
    for row in rows {
        let m = &row.measured;
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {:.2} / {:.1} | {:.2} | {:.4} / {:.1}% | {:.4} / {:.1}% | {} | {:.1} s |",
            row.scenario,
            row.target,
            row.pool,
            strategy_mix(m),
            row.donors,
            row.fit,
            m.mean_distance * 1000.0,
            m.max_distance * 1000.0,
            m.advance_error * 1000.0,
            m.ssim,
            m.differing,
            m.shape_ssim,
            m.shape_differing,
            seam(m),
            row.seconds
        );
    }
    let _ = writeln!(out, "\n## Per scenario, in plain words\n");
    let scenarios = [
        ("a", "The same font is installed"),
        ("b", "Only another weight or version of the family"),
        ("c", "Only the metric-compatible open stand-in"),
        ("d", "Only unrelated fonts of the same kind"),
        ("e", "No donor: accents built from the subset"),
        ("f good", "Fake provider, good drawings"),
        ("f off-style", "Fake provider, off-style drawings"),
        ("f malformed", "Fake provider, malformed replies"),
    ];
    for (scenario, title) in scenarios {
        let these: Vec<&Row> = rows.iter().filter(|r| r.scenario == scenario).collect();
        if these.is_empty() {
            continue;
        }
        let seams: Vec<String> = these
            .iter()
            .map(|r| {
                let m = &r.measured;
                format!(
                    "{}: {} (shapes SSIM {:.3}, advances off by {:.1} ‰ on average)",
                    r.target,
                    seam(m),
                    m.shape_ssim,
                    m.advance_error * 1000.0
                )
            })
            .collect();
        let _ = writeln!(
            out,
            "- **{scenario}. {title}.** {}. First sentence shown to the person, for {}: “{}”",
            seams.join("; "),
            these[0].target,
            these[0].sentence
        );
    }
    let _ = writeln!(out, "\nThe fake provider (f) is a stand-in that replies with the true glyphs of the full font, changed: “good” wobbles every point by up to 2 ‰ of the em, “off-style” is 18% taller and 10% wider, “malformed” has unclosed contours and arcs. It shows the acceptance rules at work; it says nothing about how well any real provider draws.");
    out
}
