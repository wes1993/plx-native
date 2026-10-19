//! **Versions** — one title held as several `Media[]` versions, as a list you can play from.
//!
//! Plex collapses several files of one title into ONE item with several `Media[]` entries
//! (`docs/pms-api.md` §4: a 4K HDR and a 1080p cut of one episode, two 4K cuts of another at
//! different bitrates). The client used to read only `Media[0]`, so the other versions were
//! invisible. This panel is the choice: the hero's *Versions* pill opens it, and a FIRST Play on a
//! multi-version item opens it too rather than guessing (`DetailScreen::play_hero`).
//!
//! It is deliberately the SHAPE of the *Also available* panel (`screens::alt_sources`): an anchored
//! `Style::Compact` menu on the panel ground, one row per choice, a tick on the one in force, and a
//! press that only REPORTS. A screen may not name a sibling screen, so the geometry both share is
//! `plx_ui::table::menu_panel_at`.
//!
//! | mark | label | sub-line | read-out | badge |
//! |---|---|---|---|---|
//! | ✓ | `4K`   | `HEVC · MKV` | `24.5 Mbps` | `HDR`  |
//! |   | `1080p` | `H.264 · MP4` | `8.0 Mbps` | `32 GB` |
//!
//! **The label is the resolution class**, which is what a viewer chooses between — the server's own
//! `videoResolution` (or its stored frame size as the fallback), through the SAME `ui::fmt::resolution`
//! the hero's media chip and the *Also available* badge use, so one file cannot be spelled two ways.
//!
//! **The rows are in SERVER order**, which is the order `Detail::versions` holds and therefore the
//! order the index means: a re-sort would make "version 2" name a different row than the one
//! `PlayIntent::Item::version` addresses. The tick marks the page's current choice.
//!
//! **A press only REPORTS** (`AppMsg::VersionChosen`): starting a playback needs the playback
//! session's `&mut` and the `MainThread` token, neither of which a screen may name (§2.1), so the
//! panel names WHICH version and the Detail page plays it — `AltSourcesScreen`'s shape exactly.

use std::borrow::Cow;
use std::convert::Infallible;

use plx_data::metadata::{friendly_codec, MediaVersion};
use plx_plex::plex::ServerId;
use crate::registry::{AppLike, AppMsg, PageMemory};
use plx_machine::machine::{
    Canon, Cx, Delivery, Edge, Effects, EntryId, FocusKey, Fx, GroupId, Handled, InputKind, InstanceId,
    Key, LogicalState, Machine, MachineId, NavOp,
};
use plx_ui::frame::Budget;
use plx_ui::form::{Form, FormSection, FormTable, RowKey, RowKind};
use plx_ui::screen::{
    Activate, At, AxisMask, Dir, DrawFrame, EdgeRule, ElemKind, FocusSource, Focusable, GroupKind,
    GroupSpec, Hover, HitSource, Placed, RenderStrategy, Screen, ScreenEvent, Scrim, Seat, Step, Stop,
};
use plx_ui::table::{Badge, Row, TableView};
use plx_ui::{theme, Rect};

/// The fields [`VersionPickerScreen::write`] canonicalises, for the recorder's shape pin (§5.4).
/// The selected ROW is in it deliberately: this panel's UP/DOWN changes nothing else in the app, so
/// without it a replay grades the panel opening and closing and nothing between.
pub const SHAPE: [&str; 2] = [
    "VersionPickerScreen{arg:VersionPickerArg{host:u32,sid:u32,rk:str,current:u32,anchor:[u32;4]},rows:[{index:u32,label:str,detail:str,value:opt<str>,badge:opt<str>,checked:bool}],sel:i32,table:TableViewMotion}",
    TableView::MOTION_SHAPE,
];

/// What the container is asked to present.
///
/// `host` is the Detail instance the choice is reported back to; it rides on the argument rather
/// than being looked up, for `AltSourcesArg`'s reason — the entry outlives any one frame's idea of
/// which page is on top. `sid`/`rk` address the item whose versions are listed, and `current` is the
/// version the page is on NOW (the tick), carried rather than looked up so a landing under an open
/// panel cannot move it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VersionPickerArg {
    pub host: InstanceId,
    pub sid: ServerId,
    pub rk: String,
    pub current: usize,
    /// Bit-preserving rest rectangle of the pill that opened it; valid in canonical arguments
    /// without float equality.
    pub anchor: [u32; 4],
}

impl LogicalState for VersionPickerArg {
    fn write(&self, c: &mut Canon) {
        c.u32(self.host.0).u32(u32::from(self.sid.raw())).str(&self.rk).u32(self.current as u32);
        for value in self.anchor {
            c.u32(value);
        }
    }
    fn probe(&self, out: &mut String) {
        out.push_str("versions_arg");
    }
}

// ---- geometry --------------------------------------------------------------------------------

/// The pinned ~20px corner radius, the item menu's and *Also available*'s.
const PANEL_RAD: f32 = 20.0;
/// How far the panel rises into place, matching those same chip menus. The container's appear
/// spring drives it now (`DrawFrame::page_alpha` IS `Surface::motion.appear`), so the translate is
/// applied here rather than by `Popover::painter`; at rest it contributes nothing.
const RISE: f32 = plx_ui::popover::Popover::RISE;

/// Where the panel hangs: [`plx_ui::table::menu_panel_at`], shared with *Also available* — the one
/// rule for an anchored menu under a hero pill.
pub fn panel_at(anchor: Rect, content_w: f32, content_h: f32) -> Rect {
    plx_ui::table::menu_panel_at(anchor, content_w, content_h)
}

// ---- the model (pure) ------------------------------------------------------------------------

/// One drawn row, resolved: what it says and which version it names. The panel builds its
/// `TableView` from these and nothing else, so the marking and the index map are ONE list.
#[derive(Clone, Debug, PartialEq)]
pub struct VersionRow {
    /// The version's resolution class — the row's NAME, because that is what a viewer chooses
    /// between ("4K" vs "1080p").
    pub label: String,
    /// The technical sub-line: video codec · container.
    pub detail: String,
    /// The whole-stream bitrate, or `None` for a version the server did not measure.
    pub value: Option<String>,
    /// `HDR` / `DV` for a high-dynamic-range version, else the file size when the server sent one.
    pub badge: Option<String>,
    /// The version the page is on NOW — the picker's tick.
    pub checked: bool,
    /// This version's index in the item's `Media[]` — what a press reports.
    pub index: usize,
}

/// Build the panel's rows from `versions`, given the version the page is on (`current`). PURE —
/// every decision in this module is here, and the host suite drives it directly.
///
/// **Exactly one row is ticked**, by construction: the mark is placed on the index `current` when
/// the list holds it. A list that does not is ticked nowhere at all — the honest answer while a
/// landing is being reconciled, and never two.
///
/// The order is the server's, and it is load-bearing rather than cosmetic: `index` is what the
/// press reports and what `PlayIntent::Item::version` addresses, so re-sorting the rows would make
/// "version 2" name a different file than the one the resolve reads.
pub fn rows(versions: &[MediaVersion], current: usize) -> Vec<VersionRow> {
    versions
        .iter()
        .enumerate()
        .map(|(index, v)| VersionRow {
            label: version_label(v, index),
            detail: version_detail(v),
            value: (v.bitrate > 0).then(|| plx_ui::fmt::bitrate(v.bitrate)),
            badge: version_badge(v),
            checked: index == current,
            index,
        })
        .collect()
}

/// A version's name: its resolution class, else the container, else its position. Never empty —
/// a row with no label is a row a viewer cannot choose between.
fn version_label(v: &MediaVersion, index: usize) -> String {
    if let Some(res) = plx_ui::fmt::resolution(&v.video_resolution, v.width, v.height) {
        return res;
    }
    if !v.container.is_empty() {
        return v.container.to_uppercase();
    }
    format!("{}", index + 1)
}

/// A version's sub-line: the video codec's display name and the container, `HEVC · MKV`. The codec
/// map is the ONE `plx_data::metadata::friendly_codec` the track menu and Info card also use.
fn version_detail(v: &MediaVersion) -> String {
    let codec = friendly_codec(&v.vcodec);
    let container = v.container.to_uppercase();
    match (codec.is_empty(), container.is_empty()) {
        (false, false) => format!("{codec} \u{b7} {container}"),
        (false, true) => codec,
        (true, false) => container,
        (true, true) => String::new(),
    }
}

/// A version's badge: the HDR format it carries, else its file size. HDR first because it is the
/// fact that changes what a viewer sees; "HDR"/"DV" are the technical abbreviations the product
/// already prints untranslated in its own copy.
fn version_badge(v: &MediaVersion) -> Option<String> {
    if v.dovi.present {
        return Some("DV".to_string());
    }
    if v.hdr {
        return Some("HDR".to_string());
    }
    (v.size > 0).then(|| plx_ui::fmt::bytes(v.size, (1, 0, 0)))
}

/// One row's drawing: its four content places. The tick is WHICH version is in force, the label
/// WHICH RESOLUTION, the sub-line the codec and container, the read-out the bitrate and the badge
/// the HDR format or the size.
fn row_for(r: &VersionRow) -> Row {
    let mut row = Row::new(r.label.clone()).server_label().detail(r.detail.clone()).checked(r.checked);
    if !r.detail.is_empty() {
        row = row.server_detail();
    }
    if let Some(v) = &r.value {
        row = row.value(v.clone());
    }
    if let Some(b) = &r.badge {
        row = row.badge(Badge::Text(b.clone()));
    }
    row
}

/// What OK on the highlighted row does.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Play the version at this index in the item's `Media[]`.
    Choose(usize),
}

/// A row's identity: the version index it names and, for a producer that listed the same index
/// twice, which occurrence — a duplicate is a second row, never a second identity collision.
#[derive(Clone, Debug, PartialEq)]
pub struct VersionId {
    index: usize,
    nth: usize,
}

impl VersionId {
    /// The identity of the row at `pos` of `rows`.
    fn of(rows: &[VersionRow], pos: usize) -> Self {
        let r = &rows[pos];
        let nth = rows[..pos].iter().filter(|o| o.index == r.index).count();
        VersionId { index: r.index, nth }
    }
}

/// The panel's one section, declared from the resolved rows alone (pure, so the text-fit suite
/// drives it): every row carries its own destination. The dynamic rows' focus key is their
/// position in the list (docs/settings-form.md, "Dynamic rows").
pub fn form_for(rows: &[VersionRow]) -> Form<VersionId, Action, Infallible> {
    let mut sec = FormSection::new("");
    for (pos, r) in rows.iter().enumerate() {
        let id = VersionId::of(rows, pos);
        sec = sec.item_keyed(id, RowKey(pos as u32), RowKind::Choice, Action::Choose(r.index), row_for(r));
    }
    Form::new().section(sec)
}

// ---- the surface -----------------------------------------------------------------------------

/// Where the selection lands when the table is rebuilt.
enum Sel {
    /// Opening: on the version you are on — the row the tick is against.
    OnTheVersionYouAreOn,
    /// Rebuilding under an OPEN panel: leave the cursor where the user put it.
    Keep,
}

pub struct VersionPickerScreen {
    entry: EntryId,
    arg: VersionPickerArg,
    /// The rows the `TableView` was built from — the rebuild stamp (module doc).
    rows: Vec<VersionRow>,
    /// The table and what each row does, replaced together ([`form_for`]).
    pub form: FormTable<VersionId, Action, Infallible>,
}

impl VersionPickerScreen {
    pub fn new(entry: EntryId, arg: VersionPickerArg, meta: plx_data::metadata::MetadataView<'_>) -> Self {
        let mut screen = Self {
            entry,
            arg,
            rows: Vec::new(),
            form: FormTable::new(plx_ui::table_screen::BAND_BASE),
        };
        screen.rebuild(Sel::OnTheVersionYouAreOn, meta);
        screen
    }

    /// The rows the store would produce right now, ticked against **the version the open PAGE is
    /// on** (carried on the argument), for the item the page is standing on.
    fn live_rows(&self, meta: plx_data::metadata::MetadataView<'_>) -> Vec<VersionRow> {
        let versions = meta
            .current()
            .filter(|d| plx_plex::plex::same_item((d.sid, &d.rk), (self.arg.sid, &self.arg.rk)))
            .map(|d| d.play_versions())
            .unwrap_or(&[]);
        rows(versions, self.arg.current)
    }

    /// Materialise the store into the table, when and only when the drawn content would differ.
    fn refresh(&mut self, meta: plx_data::metadata::MetadataView<'_>) -> bool {
        let next = self.live_rows(meta);
        if next == self.rows {
            return false;
        }
        self.rows = next;
        self.rebuild(Sel::Keep, meta);
        true
    }

    fn rebuild(&mut self, sel_mode: Sel, meta: plx_data::metadata::MetadataView<'_>) {
        if matches!(sel_mode, Sel::OnTheVersionYouAreOn) {
            self.rows = self.live_rows(meta);
        }
        let form = form_for(&self.rows);
        // rows carry a sub-line — the track menu's size class, not the account popover's
        self.form.table.compact = false;
        match sel_mode {
            // leave the cursor where the user put it — on the same version, found by identity
            Sel::Keep => {
                let held = self.form.selected_id().cloned();
                self.form.set(form, held.as_ref());
            }
            // on the version you are on (the tick), else the first row
            Sel::OnTheVersionYouAreOn => {
                let here = self.rows.iter().position(|r| r.checked).map(|pos| VersionId::of(&self.rows, pos));
                self.form.set(form, here.as_ref());
            }
        }
    }

    pub fn frame(&self, measure: &dyn plx_machine::machine::Measure) -> Rect {
        let [x, y, w, h] = self.arg.anchor.map(f32::from_bits);
        panel_at(Rect::new(x, y, w, h), self.form.table.measured_width(measure), self.form.table.measured_height())
    }

    fn dismiss<H: AppLike>(&self, fx: &mut Effects<'_, H>) {
        fx.push(Fx::Nav(NavOp::Dismiss(self.entry)));
    }

    /// Commit row `elem` and close. The row reports its version index; the PAGE plays it.
    ///
    /// Takes the elem directly rather than reading the table's cursor: the engine's OK arm reads it
    /// off the current focus key and a pointer click's `Activate` names the STOP that was clicked,
    /// which need not be the row the cursor was already resting on — the same distinction
    /// `AltSourcesScreen::commit` draws.
    fn commit<H: AppLike>(&mut self, elem: u32, fx: &mut Effects<'_, H>) {
        let action = match self.form.index_of_key(RowKey(elem)).and_then(|i| self.form.activate(i)) {
            Some(plx_ui::form::Activation::Action(action)) => action,
            _ => return,
        };
        self.dismiss(fx);
        // The PAGE plays. This surface names WHICH version and nothing else — the
        // `AltSourcesScreen` shape, and what keeps "what a press means on the Detail page" in one
        // place instead of two.
        let Action::Choose(index) = action;
        fx.push(Fx::Deliver(
            MachineId::Instance(self.arg.host),
            Delivery::Screen(ScreenEvent::App(AppMsg::VersionChosen { index })),
        ));
    }
}

impl<H: AppLike<Memory = PageMemory> + crate::registry::MetadataLike> Machine<H> for VersionPickerScreen {
    type Ev = ScreenEvent<H>;
    fn step(&mut self, ev: &Self::Ev, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) -> Handled {
        match ev {
            ScreenEvent::Mount | ScreenEvent::Enter(_) => {
                self.refresh(H::metadata(cx));
                Handled::Yes
            }
            ScreenEvent::StoreChanged(ord, _) => {
                if *ord == plx_data::stores::StoreId::Metadata.ord() && self.refresh(H::metadata(cx)) {
                    fx.invalidate(plx_machine::present::Provenance::Landing(fx.from()));
                }
                Handled::Yes
            }
            ScreenEvent::Tick(tick) => {
                // Re-derive the drawn selection from the engine's own tracked focus every tick,
                // exactly as `AltSourcesScreen::step` does.
                self.form.table.sel = cx
                    .focus
                    .current
                    .filter(|key| key.entry == self.entry)
                    .and_then(|key| self.form.index_of_key(RowKey(key.elem)))
                    .map_or(self.form.table.sel, |i| i as i32);
                self.form.table.update(tick.dt(), self.frame(cx.measure).h);
                Handled::Yes
            }
            ScreenEvent::FocusMoved { to, .. } => {
                if let Some(i) = self.form.index_of_key(RowKey(to.elem)) {
                    self.form.table.sel = i as i32;
                }
                fx.invalidate(plx_machine::present::Provenance::Input);
                Handled::Yes
            }
            // The engine's OK arm, for a `Bare` element, delivers `Activate` directly rather than
            // arming a hold — see `Focusable::groups` below. A pointer click on a registered row
            // stop reaches the same arm through the hit map's own `Activate::Immediate` policy.
            ScreenEvent::Activate(elem) => {
                self.commit(*elem, fx);
                Handled::Yes
            }
            ScreenEvent::PressCommit(_) => {
                if let Some(key) = cx.focus.current {
                    self.commit(key.elem, fx);
                }
                Handled::Yes
            }
            // A click BESIDE every registered row stop is a HIT-MAP MISS, and `Style::Compact`'s
            // `on_miss` (`OnMiss::Dismiss`) closes the panel.
            ScreenEvent::Input(input) => match input.kind {
                InputKind::Key {
                    key: Key::Back,
                    edge: Edge::Down,
                    ..
                } => {
                    self.dismiss(fx);
                    Handled::Yes
                }
                _ => Handled::No,
            },
            _ => Handled::No,
        }
    }
}

/// **A single-column `Focusable` over the table's own cursor** — the same shape
/// `AltSourcesScreen::groups` uses for its own `TableView`: one `GroupKind::Column` of one `Bare`
/// element per row (OK/click activates on the down edge, never arms a hold), `EdgeRule::Stop` on
/// every side since this is a standalone surface with no page to escape onto, and
/// `place`/`neighbour` read straight off `TableView`'s own row geometry.
impl<H: AppLike<Memory = PageMemory>> Focusable<H> for VersionPickerScreen {
    fn groups(&self, cx: &Cx<'_, H>, out: &mut Vec<GroupSpec>) {
        out.push(GroupSpec {
            id: GroupId(0),
            kind: GroupKind::Column,
            seat: Seat::Remembered,
            reachable: AxisMask::BOTH,
            edge: [EdgeRule::Stop; 4],
            extent: self.frame(cx.measure),
            len: self.form.focusable_len(),
            elem: ElemKind::Bare,
        });
    }
    fn group_of(&self, elem: &u32, _cx: &Cx<'_, H>) -> Option<GroupId> {
        self.form.index_of_key(RowKey(*elem)).map(|_| GroupId(0))
    }
    fn neighbour(&self, key: FocusKey<u32>, dir: Dir, _cx: &Cx<'_, H>) -> Step<u32> {
        let delta = match dir {
            Dir::Up => -1,
            Dir::Down => 1,
            _ => return Step::Edge,
        };
        match self.form.step_key(RowKey(key.elem), delta) {
            Some(next) => Step::Move(FocusKey {
                entry: self.entry,
                elem: next.0,
            }),
            None => Step::Edge,
        }
    }
    fn place(&self, elem: &u32, cx: &Cx<'_, H>, _at: At) -> Option<Placed> {
        let index = self.form.index_of_key(RowKey(*elem))?;
        let rect = self.form.table.row_frame(self.frame(cx.measure), index as i32)?;
        Some(Placed {
            rect,
            rest_rect: rect,
            clip: self.frame(cx.measure),
            index: Some(index as u32),
        })
    }
    fn reconcile(&self, want: FocusKey<u32>, cx: &Cx<'_, H>) -> FocusKey<u32> {
        if self.group_of(&want.elem, cx).is_some() {
            want
        } else {
            FocusKey {
                entry: self.entry,
                elem: self.form.opening_key().map_or(0, |k| k.0),
            }
        }
    }
    fn seat(&self, _g: GroupId, _from: Placed, _cx: &Cx<'_, H>) -> FocusKey<u32> {
        FocusKey {
            entry: self.entry,
            elem: self.form.selected_key().or_else(|| self.form.opening_key()).map_or(0, |k| k.0),
        }
    }
}

impl LogicalState for VersionPickerScreen {
    fn write(&self, c: &mut Canon) {
        self.arg.write(c);
        c.seq(self.rows.len());
        for r in &self.rows {
            c.u32(r.index as u32)
                .str(&r.label)
                .str(&r.detail)
                .option(r.value.as_deref(), |c, v| {
                    c.str(v);
                })
                .option(r.badge.as_deref(), |c, b| {
                    c.str(b);
                })
                .bool(r.checked);
        }
        c.u32(self.form.table.sel as u32);
        self.form.table.write_motion(c);
    }
    fn probe(&self, out: &mut String) {
        out.push_str("versions");
    }
}

impl<H: AppLike<Memory = PageMemory> + crate::registry::MetadataLike> Screen<H> for VersionPickerScreen {
    fn name(&self) -> &'static str {
        "versions"
    }
    fn state(&self) -> &dyn LogicalState {
        self
    }
    fn crumb(&self, _cx: &Cx<'_, H>) -> Option<Cow<'_, str>> {
        None
    }
    fn prepare(&mut self, _b: &mut Budget, _cx: &Cx<'_, H>) {}
    /// The modal dim, asked for rather than drawn — the PANEL role, exactly as *Also available*
    /// asks it: the page recedes and the hero behind stays readable, because the pill that opened
    /// this list is a control on that page rather than the subject of the panel.
    fn scrim(&self) -> Scrim {
        Scrim::dim(theme::underlay::DIM_PANEL)
    }
    fn draw(&mut self, f: &mut DrawFrame<'_, '_, H>) {
        let appear = f.page_alpha;
        let r = self.frame(f.measure);
        let p = f.painter.alpha(appear).translate(0.0, RISE * (1.0 - appear));
        let measure = f.measure;
        // Named for `/tmp/plxnative-cpuprof` beside the page's own phases, so a slow frame while
        // this panel is up can be read as the PANEL or as the host under it.
        let field = f.underlay;
        plx_ui::profile::phase("dt.versions", || {
            plx_ui::widgets::panel_ground(p, r, PANEL_RAD, field, plx_ui::widgets::Ground::Sheet);
            self.form.table.draw(p, r, measure);
        });
        // The hit map's stops are registered against the SETTLED geometry (`self.frame()`, what
        // `Focusable::place` answers) rather than the transient slide `p` draws with — at rest
        // (`appear == 1`) the two coincide exactly.
        let hit_p = f.painter.alpha(appear);
        for index in 0..self.form.table.n_rows() as usize {
            let Some(elem) = self.form.key_at(index).map(|k| k.0) else {
                continue;
            };
            if let Some(placed) = <Self as Focusable<H>>::place(self, &elem, f.cx, At::Drawn) {
                f.stop(
                    hit_p,
                    Stop {
                        key: FocusKey {
                            entry: self.entry,
                            elem,
                        },
                        rect: placed.rect,
                        rest_rect: placed.rest_rect,
                        clip: placed.clip,
                        hover: Hover::Focus,
                        activate: Activate::Immediate,
                    },
                );
            }
        }
    }
    fn render(&self) -> RenderStrategy {
        RenderStrategy::Page
    }
    fn focus_source(&self) -> FocusSource {
        FocusSource::Engine
    }
    fn hit_source(&self) -> HitSource {
        HitSource::Engine
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

#[cfg(test)]
#[path = "versions_tests.rs"]
mod tests;
