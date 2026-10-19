//! The *Versions* panel's own suite: the pure row model (order, the tick, the labels and badges)
//! and the anchored geometry it shares with *Also available*. The surface itself is the same
//! `TableView`-in-a-`Compact`-surface machinery `alt_sources_tests.rs` already covers.

use super::*;
use plx_data::metadata::{Detail, Episode, MediaVersion};

fn mv(res: &str, vcodec: &str, bitrate: i64) -> MediaVersion {
    MediaVersion {
        part: "/library/parts/1/file.mkv".into(),
        video_resolution: res.into(),
        vcodec: vcodec.into(),
        acodec: "eac3".into(),
        bitrate,
        container: "mkv".into(),
        ..Default::default()
    }
}

fn labels(rs: &[VersionRow]) -> Vec<&str> {
    rs.iter().map(|r| r.label.as_str()).collect()
}

/// **The rows are in SERVER order and the tick is on `current`** — the order is load-bearing,
/// because `index` is what a press reports and what `PlayIntent::Item::version` addresses, so a
/// re-sort would make "version 2" name a different file than the one the resolve reads.
#[test]
fn rows_keep_server_order_and_tick_the_current_version() {
    let versions = vec![
        mv("4k", "hevc", 24_500),
        mv("1080", "h264", 8_000),
        mv("720", "h264", 3_000),
    ];
    let rs = rows(&versions, 1);
    assert_eq!(labels(&rs), ["4K", "1080p", "720p"]);
    assert_eq!(
        rs.iter().map(|r| r.index).collect::<Vec<_>>(),
        [0, 1, 2],
        "the row's index IS its Media[] position"
    );
    assert_eq!(
        rs.iter().map(|r| r.checked).collect::<Vec<_>>(),
        [false, true, false],
        "exactly the current version is ticked"
    );
}

/// A `current` the list does not hold ticks NOWHERE — the honest answer while a landing is being
/// reconciled, and never two rows at once.
#[test]
fn the_tick_is_nowhere_when_current_is_out_of_range() {
    let rs = rows(&[mv("4k", "hevc", 24_500), mv("1080", "h264", 8_000)], 9);
    assert!(rs.iter().all(|r| !r.checked));
}

/// The label is the resolution class; a version the server described no resolution for falls back
/// to its container rather than to an empty row a viewer cannot choose between.
#[test]
fn a_version_with_no_resolution_falls_back_to_its_container() {
    let mut v = mv("", "", 0);
    v.container = "mp4".into();
    assert_eq!(rows(&[v], 0)[0].label, "MP4");

    let bare = MediaVersion { container: String::new(), ..MediaVersion::default() };
    assert_eq!(rows(&[bare], 0)[0].label, "1", "…and finally to its position");
}

/// The sub-line is the codec's display name and the container — the ONE `friendly_codec` map the
/// track menu and Info card also use.
#[test]
fn the_sub_line_names_the_codec_and_container() {
    let rs = rows(&[mv("1080", "h264", 8_000)], 0);
    assert_eq!(
        rs[0].detail, "H264 \u{b7} MKV",
        "the ONE `friendly_codec` map the track menu and Info card also read"
    );
    assert_eq!(rs[0].value.as_deref(), Some("8.0 Mbps"), "the whole-stream bitrate");
}

/// The badge prefers the HDR format (the fact that changes what a viewer sees) and falls back to
/// the file size; an unmeasured version with no HDR carries no badge at all.
#[test]
fn the_badge_prefers_the_hdr_format_over_the_size() {
    let mut dv = mv("4k", "hevc", 24_500);
    dv.dovi = plx_data::metadata::Dovi { present: true, profile: 8, ..plx_data::metadata::Dovi::NONE };
    dv.size = 34_000_000_000;
    assert_eq!(rows(&[dv], 0)[0].badge.as_deref(), Some("DV"));

    let mut hdr = mv("4k", "hevc", 24_500);
    hdr.hdr = true;
    hdr.size = 34_000_000_000;
    assert_eq!(rows(&[hdr], 0)[0].badge.as_deref(), Some("HDR"));

    let mut plain = mv("1080", "h264", 8_000);
    plain.size = 1_500_000_000;
    assert_eq!(rows(&[plain], 0)[0].badge.as_deref(), Some("1.4 GB"));

    assert_eq!(rows(&[mv("1080", "h264", 8_000)], 0)[0].badge, None);
}

/// Every row reports its OWN version index — the panel's whole job is to name which `Media[]` entry
/// the page should play.
#[test]
fn every_row_reports_its_own_version_index() {
    let rs = rows(&[mv("4k", "hevc", 24_500), mv("1080", "h264", 8_000)], 0);
    let mut table = FormTable::new(plx_ui::table_screen::BAND_BASE);
    table.set(form_for(&rs), None);
    let picked: Vec<usize> = (0..table.focusable_len())
        .filter_map(|i| match table.activate(i) {
            Some(plx_ui::form::Activation::Action(Action::Choose(index))) => Some(index),
            _ => None,
        })
        .collect();
    assert_eq!(picked, [0, 1]);
}

/// **The panel hangs UNDER its anchor, and ABOVE it when there is no room below** — never over the
/// pill that opened it. The geometry is `plx_ui::table::menu_panel_at`, shared with *Also available*.
#[test]
fn the_panel_hangs_under_its_anchor_and_above_when_there_is_no_room() {
    let anchor = Rect::new(120.0, 200.0, 240.0, 60.0);
    let below = panel_at(anchor, 400.0, 300.0);
    assert!(below.y > anchor.y + anchor.h, "under the pill when there is room: {below:?}");
    assert_eq!(below.x, anchor.x, "hung off the anchor's left edge");

    let low = Rect::new(120.0, plx_ui::consts::SCR_H - 100.0, 240.0, 60.0);
    let above = panel_at(low, 400.0, 300.0);
    assert!(above.y + above.h <= low.y, "above the pill when there is not: {above:?}");
}

/// One `TableView` section for `list`, built through the panel's REAL `form_for` — the shape a
/// fit test must grade, never a hand-assembled row.
fn section_for(list: &[VersionRow]) -> plx_ui::table::Section {
    let mut t = FormTable::<VersionId, Action, std::convert::Infallible>::new(plx_ui::table_screen::BAND_BASE);
    t.set(form_for(list), None);
    t.table.sections.remove(0)
}

/// **The panel's own fit gate**: the longest realistic version content must fit the menu's cap and
/// its hugged width in every shipped language. The row text is server-derived (`server_label`/
/// `server_detail`), but the badge and the read-out are not, so this grades what a viewer sees.
#[test]
fn every_version_row_fits_the_menu_in_every_shipped_language() {
    use plx_platform::i18n::{language_on_this_thread_for_test, SHIPPED};
    let mut list = Vec::new();
    for (i, res) in ["4k", "1080", "720", "576", "sd", ""].into_iter().enumerate() {
        let mut v = mv(res, if i % 2 == 0 { "hevc" } else { "h264" }, 26_100);
        v.size = 34_600_000_000;
        v.dovi = if i == 0 {
            plx_data::metadata::Dovi { present: true, profile: 8, ..plx_data::metadata::Dovi::NONE }
        } else {
            plx_data::metadata::Dovi::NONE
        };
        list.push(v);
    }
    let built = rows(&list, 0);
    let mut out = Vec::new();
    for language in SHIPPED {
        let _guard = language_on_this_thread_for_test(language);
        let mut table = plx_ui::table::TableView::new();
        table.compact = false;
        table.set_sections(vec![section_for(&built)], 0, false);
        out.extend(table.menu_cap_failure(&plx_base::fontcov::advances::ShippedMeasure, language.tag()));
        out.extend(table.app_fit_failures(plx_ui::table::MENU_MAX_W, language.tag()));
        out.extend(table.app_fit_failures_hugged(language.tag()));
    }
    plx_ui::table::assert_no_fit_failures(&out);
}

/// **`play_versions` is the leaf the hero would START**: a movie's own `Media[]`, and on a show the
/// hero episode's — a show container carries no `Media[]`, so reading `versions` there would report
/// "no choice" on a page that plainly has one.
#[test]
fn play_versions_follows_the_hero_episode_on_a_show() {
    let episode = Episode {
        rk: "e1".into(),
        resume_ms: 60_000,
        versions: vec![mv("4k", "hevc", 24_500), mv("1080", "h264", 8_000)],
        ..Default::default()
    };
    let show = Detail {
        is_show: true,
        seasons: vec![plx_data::metadata::Season {
            rk: "s1".into(),
            index: 1,
            title: "Season 1".into(),
            leaf_count: 3,
            viewed_leaf_count: 1,
        }],
        on_deck: Some(episode),
        ..Default::default()
    };
    assert!(show.versions.is_empty(), "a show container carries no Media[] of its own");
    assert_eq!(show.play_versions().len(), 2, "…but its hero episode does");

    let movie = Detail { versions: vec![mv("4k", "hevc", 24_500)], ..Default::default() };
    assert_eq!(movie.play_versions().len(), 1, "a leaf answers for itself");
}
