//! The Chinese interface on a real machine: the full catalogs and Chinese file names, drawn with an
//! installed face rather than a font asset. Runs on every desktop platform —
//! [`theme::installed_font_definitions`] asks this machine for a Han face, searching Windows, macOS
//! and Linux.
//!
//! One installed face takes the general slot ([`theme::SYSTEM_FALLBACK`]): a Han face where the
//! interface is Chinese and the build embeds no Chinese face, the Arabic-script face otherwise.
//! Outside Chinese a second installed face ([`theme::SYSTEM_HAN_FALLBACK`]) carries the Chinese,
//! because that slot is busy. What this file adds over `fonts.rs` is the whole of both catalogs —
//! not just the Han characters — in all four UI families, and where the Chinese ink actually lands.

use std::collections::BTreeSet;

use egui::epaint::text::{Fonts, TextOptions};
use egui::{Color32, FontDefinitions, FontId};
use pdfcraft_ui_egui::theme;

fn families() -> [FontId; 4] {
    [FontId::proportional(13.0), FontId::monospace(13.0), theme::medium(13.0), theme::semibold(17.0)]
}

fn check_glyphs(defs: FontDefinitions, text: &str) {
    let mut fonts = Fonts::new(TextOptions::default(), defs);
    for id in families() {
        let missing: String = text.chars().filter(|c| !fonts.has_glyphs(&id, &c.to_string())).collect();
        assert!(missing.is_empty(), "{id:?} lacks {missing:?}");
    }
}

/// Every non-ASCII, non-space character a catalog writes to the screen.
fn translated_chars(catalog: &str) -> BTreeSet<char> {
    let mut chars = BTreeSet::new();
    for line in catalog.lines().filter(|line| !line.starts_with('#')) {
        if let Some(translation) = line.split('\t').nth(2) {
            chars.extend(translation.chars().filter(|c| !c.is_ascii() && !c.is_whitespace()));
        }
    }
    chars
}

/// True when the installed face is drawing Chinese for this interface language. False when there is
/// none — but a build that ships a Chinese interface is exactly the build that must not pass by
/// skipping, so the release jobs set `PDFCRAFT_TEST_REQUIRE_CJK=1` and the check fails instead.
fn han_face_or_skip(what: &str) -> bool {
    let defs = theme::installed_font_definitions(theme::CjkPreference::SimplifiedChinese);
    if pdfcraft_fonts::ui_chinese_fonts().is_empty() && defs.font_data.contains_key(theme::SYSTEM_FALLBACK) {
        return true;
    }
    assert!(
        std::env::var_os("PDFCRAFT_TEST_REQUIRE_CJK").is_none_or(|value| value != "1"),
        "no installed Chinese face: PDFCRAFT_TEST_REQUIRE_CJK=1 was set, so this build would show {what} broken"
    );
    eprintln!("skipping {what}: no installed Chinese face (or PDFCRAFT_SYSTEM_FONTS=0)");
    false
}

#[test]
fn chinese_catalogs_and_filenames_draw_with_the_installed_face() {
    // The installed faces are only ever added on top of the embedded ones.
    for prefer_hans in [true, false] {
        let defs = theme::font_definitions_for(prefer_hans);
        assert!(!defs.font_data.contains_key(theme::SYSTEM_FALLBACK));
        assert!(!defs.font_data.contains_key(theme::SYSTEM_HAN_FALLBACK));
    }
    if !han_face_or_skip("the Chinese catalogs") {
        return;
    }

    for cjk in [theme::CjkPreference::SimplifiedChinese, theme::CjkPreference::TraditionalChinese, theme::CjkPreference::Default] {
        let prefer_hans = cjk == theme::CjkPreference::SimplifiedChinese;
        let defs = theme::installed_font_definitions(cjk);
        let embedded = theme::font_definitions_for(prefer_hans);
        let japanese: Vec<String> = pdfcraft_fonts::ui_japanese_fonts().iter().map(|face| face.name()).collect();
        let chinese: Vec<String> = pdfcraft_fonts::ui_chinese_fonts().iter().map(|face| face.name()).collect();
        let installed = defs.font_data.contains_key(theme::SYSTEM_FALLBACK);
        for (family, stack) in &defs.families {
            if !installed {
                assert!(!stack.iter().any(|name| name == theme::SYSTEM_FALLBACK), "{family:?}: {stack:?}");
                continue;
            }
            let at = stack.iter().position(|name| name == theme::SYSTEM_FALLBACK).expect("checked above");
            assert_eq!(stack.iter().filter(|name| *name == theme::SYSTEM_FALLBACK).count(), 1);
            if prefer_hans || cjk == theme::CjkPreference::TraditionalChinese {
                // Chinese mode: the general slot holds the Han face, after any embedded Chinese
                // face and before every Japanese one, so one Chinese line is one face.
                assert!(!defs.font_data.contains_key(theme::SYSTEM_HAN_FALLBACK), "a Chinese interface uses the general slot for its Han face");
                assert!(stack.iter().enumerate().filter(|(_, name)| japanese.contains(name)).all(|(i, _)| at < i), "{family:?}: {stack:?}");
                assert!(stack.iter().enumerate().filter(|(_, name)| chinese.contains(name)).all(|(i, _)| i < at), "{family:?}: {stack:?}");
            } else {
                // Outside Chinese the general slot is the Arabic-script face and stays last; the
                // installed faces only ever end the family, after every embedded one.
                let base = embedded.families[family].len();
                assert_eq!(at, stack.len() - 1, "{family:?}: {stack:?}");
                let tail = stack.get(base..).expect("the installed faces extend every family");
                assert!(
                    tail.iter().all(|name| !embedded.font_data.contains_key(name)),
                    "{family:?}: an embedded face sits after the installed ones: {stack:?}"
                );
                if let Some(han) = stack.iter().position(|name| name == theme::SYSTEM_HAN_FALLBACK) {
                    assert!(han < at, "{family:?}: a Chinese file name needs the Han face before the Arabic one: {stack:?}");
                }
            }
        }
        check_glyphs(defs.clone(), "工程估价_欢迎编辑_报告.pdf");
        // In a Chinese interface the one general slot draws Chinese, so an Arabic file name draws
        // only when craft-fonts supplies an Arabic face, which every release build does.
        if cjk == theme::CjkPreference::Default || !pdfcraft_fonts::ui_arabic_fonts().is_empty() {
            check_glyphs(defs, "واحد اثنين");
        }
    }

    let mut total = 0;
    for catalog in [include_str!("../../src/i18n/zh-hans.tsv"), include_str!("../../src/i18n/zh-hant.tsv")] {
        let chars = translated_chars(catalog);
        total += chars.len();
        let chars: String = chars.iter().collect();
        check_glyphs(theme::installed_font_definitions(theme::CjkPreference::SimplifiedChinese), &chars);
    }
    println!("verified {total} unique non-ASCII translated glyphs (zh-hans + zh-hant) in four UI families; Chinese file names in both font orders");
}

/// The Chinese face must sit on the line, not above it.
///
/// egui places a glyph `face ascent + (primary row height - face row height) / 2` below the top of
/// its row, so a face whose row is much taller than Inter's — every Chinese face is — was drawn 3 pt
/// above the Latin beside it at 13 pt, and out of the top of the row box, where a fixed-height
/// widget clipped it. The installed face's `y_offset_factor` tweak cancels that; this measures what
/// the user sees, which is where the ink lands.
#[test]
fn chinese_text_shares_the_interface_baseline() {
    if !han_face_or_skip("Chinese baseline alignment") {
        return;
    }
    let defs = theme::installed_font_definitions(theme::CjkPreference::SimplifiedChinese);
    for size in [13.0, 17.0, 26.0] {
        let mut fonts = Fonts::new(TextOptions::default(), defs.clone());
        fonts.begin_pass(TextOptions::default());
        let mut view = fonts.with_pixels_per_point(2.0);
        let galley = view.layout("中A".to_owned(), FontId::proportional(size), Color32::BLACK, f32::INFINITY);
        let row = galley.rows.first().expect("one row");
        // Where the pixels actually land: the baseline, plus the glyph image's own box on it.
        let ink = |c: char| {
            row.row
                .glyphs
                .iter()
                .filter(|g| g.chr == c)
                .map(|g| (g.pos.y + g.uv_rect.offset.y, g.pos.y + g.uv_rect.offset.y + g.uv_rect.size.y))
                .next()
        };
        let Some((han_top, han_bottom)) = ink('中') else { panic!("中 left no ink at {size} pt: the installed face is not drawing") };
        let Some((latin_top, latin_bottom)) = ink('A') else { panic!("A left no ink at {size} pt") };
        let han_center = (han_top + han_bottom) / 2.0;
        let latin_center = (latin_top + latin_bottom) / 2.0;
        assert!(
            (han_center - latin_center).abs() <= 0.06 * size,
            "{size} pt: Chinese ink centred at {han_center:.2}, Latin at {latin_center:.2} — {} of the size apart",
            (han_center - latin_center).abs() / size
        );
        // A fixed-height widget clips at the row box, so the Chinese has to fit inside it.
        assert!(
            han_top >= -0.5 && han_bottom <= row.row.size.y + 0.5,
            "{size} pt: Chinese ink {han_top:.2}..{han_bottom:.2} spills the 0..{:.2} row box",
            row.row.size.y
        );
    }
}

#[test]
fn publication_opt_out_disables_the_installed_face() {
    if std::env::var_os("PDFCRAFT_SYSTEM_FONTS").is_none_or(|value| value != "0") {
        return;
    }
    for cjk in [theme::CjkPreference::SimplifiedChinese, theme::CjkPreference::Default] {
        let defs = theme::installed_font_definitions(cjk);
        for name in [theme::SYSTEM_FALLBACK, theme::SYSTEM_HAN_FALLBACK] {
            assert!(!defs.font_data.contains_key(name));
            assert!(defs.families.values().all(|stack| !stack.iter().any(|entry| entry == name)));
        }
    }
}
