//! **Also available** — the same film held by more than one source, as a list you can walk into.
//!
//! Deliverable E of the Shared Sources design (`docs/shared-servers.md` §6): when a second pinned
//! source also holds the item on screen, the detail page's actions row grows an *Also available*
//! pill with a trailing chevron, and it opens this panel — a `TableView` on the panel ground
//! (`widgets::panel_ground`), the same object the Library's Sort and Filter chips open one page over.
//!
//! **A `Style::Compact` surface on the container tree** since restructure phase 10 (§6.2): the
//! anchored menu the Library's chips already are. The container owns its PHASE and its appear
//! spring; the surface owns input while it is `Opening | Open`; and its ANCHOR — the drawn rect of
//! the pill that opened it — rides on its [`AltSourcesArg`] exactly as `LibraryMenuArg`'s does,
//! which is what retired a `static mut ANCHOR: PanelAnchor` holding a `Rect` (a geometry static is
//! never an allowlistable render cache, §15.2).
//!
//! Its rows use all FOUR of that cell's content places — the leading mark, the label over its
//! sub-line, the trailing read-out, and the right-aligned badge:
//!
//! | mark | label | sub-line | read-out | badge |
//! |---|---|---|---|---|
//! | ✓ | `Movies`    | `This account` | `1 hr 57 min` | `1080p` |
//! |   | `Film Club` | `friend`       | `1 hr 57 min` | `4K`    |
//!
//! # The four calls this module implements, and why each is not the obvious thing
//!
//! **A CONTROL in the actions row, not a section of the page.** An item on several servers is a
//! rare fact; a permanent band would spend a whole row on nothing on every other page in the
//! library. So the list is behind a button, and the button itself is drawn ONLY when a second
//! pinned source holds this item — the same "with one source, none of this is drawn" rule the
//! shelf heading's annotation implements as absence rather than as a branch that draws nothing
//! visible. **That gate is `metadata::alt_available`, asked by the PAGE about its own item**, and
//! it is deliberately not a question about this panel: a screen must never have to mount a surface
//! to find out how many columns its own footer has.
//!
//! **The tick marks the CHOSEN copy** — the page's own item until the viewer picks another —
//! and the badge is the RESOLUTION class — the vocabulary the hero's badges row already speaks
//! (`ui::fmt::resolution`, one spelling of "4K" in the product). A row is `[library] [owner ·
//! runtime] [class]`: the label answers *which library*, the sub-line *whose*, the badge *what
//! you would get*.
//!
//! **OK CHOOSES, it does not navigate.** Picking a row only moves the tick; the page stays, with
//! no fetch and no reload. The hero's Play (or Resume) starts the ticked copy straight into the
//! player, without opening its page. That is the whole of what the old navigate-on-OK did not
//! do: opening the copy's page meant a fresh fetch with a spinner on grey, which read as the
//! detail page reloading from under you on what was only ever meant as a choice. Like every
//! other menu here this surface only REPORTS the choice; it delivers
//! [`AppMsg::AltSourceChosen`] to the Detail instance named by its argument and the PAGE
//! remembers it — the *Versions* panel's shape, whose tick this one mirrors.
//!
//! The resume position stays per-copy: Play resolves the ticked copy's own viewOffset from its
//! own fetch, so choosing across servers never drags one file's position onto another's.
//!
//! **Ordering is the copy that PLAYS first**, then what a viewer would prefer: higher quality
//! first, and at equal quality yours before a friend's, because yours cannot go offline mid-film.
//! The design's own example pins the first clause against the second — a 1080p copy you are
//! standing on sorts above a friend's 4K one.
//!
//! Explicitly rejected, and not to be re-introduced: merging the copies invisibly and playing the
//! best one (it is what makes a film disappear when a friend goes offline, with no way to see why);
//! a permanent section on the page; switching the copy silently on OK.
//!
//! # Where the copies come from
//!
//! **The identity is the `guid` (`plex://movie/…`), never the `ratingKey`.** Every item-shaped
//! integer in Plex is server-local and starts at 1, so both servers in this household have a
//! `ratingKey` 4 and a section 1 (`docs/shared-servers.md` §1). Matching on one would confidently
//! offer a different film — which is why the STORE is ADDRESSED on the `(server, ratingKey)` PAIR:
//! a resolve outliving the page that asked for it is the normal case, not the exotic one. The
//! store, the cross-source resolve that fills it and the headless `/tmp/plxnative-shared` stand-in
//! all live in `plx_data::metadata`, beside the detail fetch whose landing kicks them; this module
//! ORDERS, MARKS and DRAWS and owns nothing else.
//!
//! **Rows are a SNAPSHOT, refreshed against a stamp.** `TableView` is built once and drawn from,
//! so a correction reaching the store — a re-described source's credit, a revoked share — has to
//! be carried the last step by rebuilding. The stamp is the row list itself, so a rebuild happens
//! exactly when the drawn content would differ, and the ORDER moves with the text (the credit is
//! the own-before-a-friend's tiebreak, so a corrected panel can legitimately swap two of its rows).
//! The cursor is KEPT across a rebuild rather than re-seated on the tick: the rows can re-order
//! beneath it, which is a smaller surprise than the cursor jumping mid-interaction.

use std::borrow::Cow;
use std::convert::Infallible;

use plx_data::metadata::AltCopy;
use plx_plex::plex::ServerId;
use crate::registry::{AppLike, AppMsg, PageMemory};
use plx_ui::frame::Budget;
use plx_machine::machine::{
    Canon, Cx, Delivery, Edge, Effects, EntryId, FocusKey, Fx, GroupId, Handled, InputKind,
    InstanceId, Key, LogicalState, Machine, MachineId, NavOp,
};
use plx_ui::screen::{
    Activate, At, AxisMask, Dir, DrawFrame, EdgeRule, ElemKind, FocusSource, Focusable, GroupKind,
    GroupSpec, Hover, HitSource, Placed, RenderStrategy, Screen, ScreenEvent, Scrim, Seat, Step,
    Stop,
};
use plx_ui::form::{Form, FormSection, FormTable, RowKey, RowKind};
use plx_ui::table::{Badge, Row, TableView};
use plx_ui::{theme, Rect};

/// The fields [`AltSourcesScreen::write`] canonicalises, for the recorder's shape pin (§5.4). The
/// selected ROW is in it deliberately: this panel's UP/DOWN changes nothing else in the app, so
/// without it a replay grades the panel opening and closing and nothing between.
pub const SHAPE: [&str; 2] = [
    "AltSourcesScreen{arg:AltSourcesArg{host:u32,sid:u32,rk:str,choice:opt<{sid:u32,rk:str}>,anchor:[u32;4]},rows:[{sid:u32,rk:str,label:str,detail:str,value:opt<str>,badge:opt<str>,checked:bool}],sel:i32,table:TableViewMotion}",
    TableView::MOTION_SHAPE,
];

/// One picked copy: which server's which item. The tick's whole identity — the page remembers it
/// (`DetailScreen::alt_choice`) and the hero's Play starts it straight into the player.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct AltChoice {
    pub sid: ServerId,
    pub rk: String,
}

/// What the container is asked to present.
///
/// `host` is the Detail instance the choice is reported back to; it rides on the argument rather
/// than being looked up, for `AltSourcesArg`'s reason — the entry outlives any one frame's idea of
/// which page is on top. `sid`/`rk` are the copy the page is STANDING ON: the pair the addressed
/// store is read with. `choice` is the copy the page has picked, if it picked another one: the
/// tick's other half.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AltSourcesArg {
    pub host: InstanceId,
    pub sid: ServerId,
    pub rk: String,
    pub choice: Option<AltChoice>,
    /// Bit-preserving rest rectangle of the pill that opened it; valid in canonical arguments
    /// without float equality.
    pub anchor: [u32; 4],
}

impl LogicalState for AltSourcesArg {
    fn write(&self, c: &mut Canon) {
        c.u32(self.host.0).u32(u32::from(self.sid.raw())).str(&self.rk);
        match &self.choice {
            Some(choice) => {
                c.bool(true).u32(u32::from(choice.sid.raw())).str(&choice.rk);
            }
            None => {
                c.bool(false).u32(u32::from(ServerId::UNSET.raw())).str("");
            }
        }
        for value in self.anchor {
            c.u32(value);
        }
    }
    fn probe(&self, out: &mut String) {
        out.push_str("alt_sources_arg");
    }
}

// ---- geometry --------------------------------------------------------------------------------

/// The pinned ~20px corner radius, the item menu's.
const PANEL_RAD: f32 = 20.0;
/// How far the panel rises into place, matching those same chip menus. The container's appear
/// spring drives it now (`DrawFrame::page_alpha` IS `Surface::motion.appear`), so the translate is
/// applied here rather than by `Popover::painter`; at rest it contributes nothing, which is what
/// makes a settled capture of this panel byte-identical across the conversion.
const RISE: f32 = plx_ui::popover::Popover::RISE;

/// Under the button when there is room, above it when there is not — never over it, so the control
/// that opened the panel stays readable beside its own list. Horizontally it hangs off the
/// button's left edge (the list and the label it came from share a margin), pulled inside the
/// screen's keep-out. Pure (anchor + measured height in, rect out), so the placement rules are
/// host-testable without mounting anything.
///
/// The rule itself is [`plx_ui::table::menu_panel_at`], shared with the *Versions* menu — the
/// screen layer may not name a sibling screen, so the geometry lives below both.
pub fn panel_at(a: Rect, content_w: f32, content_h: f32) -> Rect {
    plx_ui::table::menu_panel_at(a, content_w, content_h)
}

// ---- the model (pure) ------------------------------------------------------------------------

/// A copy's quality, as SCAN LINES — the ordering's second key.
///
/// It reads the same three fields `ui::fmt::resolution` badges the row with, in the same
/// precedence (the server's own class first, the stored frame size only as its fallback), so the
/// ladder the eye sees on the badges and the order the rows are in cannot disagree. Saturating,
/// because these are wire values through the lenient `de_i64` and a garbage 19-digit height must
/// not overflow into a top-of-list sort key.
fn scan_lines(c: &AltCopy) -> i64 {
    let r = c.res.trim().to_ascii_lowercase();
    match r.as_str() {
        "" => {}
        "8k" => return 4320,
        "4k" => return 2160,
        "sd" => return 480,
        _ if r.chars().all(|ch| ch.is_ascii_digit()) => return r.parse::<i64>().unwrap_or(0),
        _ => {}
    }
    if c.height > 0 {
        c.height
    } else {
        c.width.saturating_mul(9) / 16
    }
}

/// One drawn row, resolved: what it says and where OK goes. The panel builds its `TableView` from
/// these and nothing else, so the ordering, the tick and the destination map are ONE list.
#[derive(Clone, Debug, PartialEq)]
pub struct AltRow {
    /// the library, on that source
    pub label: String,
    /// whose copy it is: `"This account"` / `"friend"`
    pub detail: String,
    /// `detail` is this app's own "This account" word (`true`), not another person's name (`false`,
    /// user text) — which of the two the sub-line's text-fit check may hold to account.
    pub own_detail: bool,
    /// the runtime, for the row's trailing read-out — `None` for a copy the server sent no
    /// duration for, which then says nothing rather than "0 min"
    pub value: Option<String>,
    /// the resolution class, or `None` for a copy the server described no video for
    pub badge: Option<String>,
    /// the copy the page is on NOW — the picker's tick
    pub checked: bool,
    pub sid: ServerId,
    pub rk: String,
}

/// What the sub-line calls a source with no owner: the signed-in account's own server. A PERSON in
/// every case, which is the design's rule for every browsing surface — the machine name never
/// appears outside the Sources list.
fn own_account() -> &'static str { plx_platform::i18n::msg::browse_account_own() }

/// Build the panel's rows from `list`, given the copy the page is standing on (`here_sid` +
/// `here_rk`) and the copy it has picked, if it picked another one (`choice`). PURE — every
/// ordering and marking decision in this module is here, and the host suite drives it directly.
///
/// **Exactly one row is ticked**, by construction: the mark is placed on the FIRST copy matching
/// the choice, else the first matching the page's own pair — so a producer that listed the same
/// copy twice still yields one tick, never two. A list matching neither is ticked nowhere at all
/// — which is the honest answer while a landing is being reconciled, and never two.
///
/// The order is the design's: the page's own copy first, then higher quality, then yours before
/// a friend's, then by library name so the list cannot reshuffle between two frames that
/// resolved the same facts. The tick follows the choice, not the order: picking never re-sorts.
pub fn rows(
    list: &[AltCopy],
    here_sid: ServerId,
    here_rk: &str,
    choice: Option<(ServerId, &str)>,
) -> Vec<AltRow> {
    // `same_item`, not a hand-rolled `&&`: it is the one place the pair rule lives, and its own doc
    // promises every stored-item lookup routes through it. These two sites were the last that did
    // not — and they are the ones that decide which row wears the tick and which press does nothing.
    let tick_on = |c: &AltCopy| {
        choice.is_some_and(|(sid, rk)| plx_plex::plex::same_item((c.sid, &c.rk), (sid, rk)))
            || choice.is_none() && plx_plex::plex::same_item((c.sid, &c.rk), (here_sid, here_rk))
    };
    let here = list.iter().position(|c| tick_on(c));
    let here_page = list
        .iter()
        .position(|c| plx_plex::plex::same_item((c.sid, &c.rk), (here_sid, here_rk)));
    let mut idx: Vec<usize> = (0..list.len()).collect();
    idx.sort_by(|&a, &b| {
        let (ca, cb) = (&list[a], &list[b]);
        // `false` sorts before `true`, so each key is written as "the one that loses"
        (
            here_page != Some(a),
            std::cmp::Reverse(scan_lines(ca)),
            ca.owner.is_some(),
            &ca.library,
        )
            .cmp(&(
                here_page != Some(b),
                std::cmp::Reverse(scan_lines(cb)),
                cb.owner.is_some(),
                &cb.library,
            ))
    });
    let mut idx: Vec<usize> = (0..list.len()).collect();
    idx.sort_by(|&a, &b| {
        let (ca, cb) = (&list[a], &list[b]);
        // `false` sorts before `true`, so each key is written as "the one that loses"
        (
            here_page != Some(a),
            std::cmp::Reverse(scan_lines(ca)),
            ca.owner.is_some(),
            &ca.library,
        )
            .cmp(&(
                here_page != Some(b),
                std::cmp::Reverse(scan_lines(cb)),
                cb.owner.is_some(),
                &cb.library,
            ))
    });
    idx.into_iter()
        .map(|i| {
            let c = &list[i];
            let who = c.owner.as_deref().unwrap_or(own_account());
            AltRow {
                label: c.library.clone(),
                detail: who.to_string(),
                own_detail: c.owner.is_none(),
                // a copy the server sent no duration for leaves the read-out slot EMPTY rather
                // than claiming "0 min" — the same rule the sub-line's dangling separator followed
                // while the runtime was part of it
                value: (c.dur_ms > 0).then(|| plx_ui::fmt::dur_long(c.dur_ms)),
                badge: plx_ui::fmt::resolution(&c.res, c.width, c.height),
                checked: here == Some(i),
                sid: c.sid,
                rk: c.rk.clone(),
            }
        })
        .collect()
}

/// One row's drawing: its four content places. The tick is the CHOSEN copy, the label WHICH
/// LIBRARY (a server's name), the sub-line WHOSE (another person's name, or this app's "This
/// account"), the read-out HOW LONG and the badge WHAT YOU GET.
fn row_for(r: &AltRow) -> Row {
    let mut row = Row::new(r.label.clone()).server_label().detail(r.detail.clone()).checked(r.checked);
    if !r.own_detail {
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
    None,
    /// Choose `rk` on server `sid` — move the tick, stay on the page. The hero's Play starts the
    /// ticked copy straight into the player.
    Choose { sid: ServerId, rk: String },
}

/// What one row's OK means, decided once when the row is declared: every resolvable row chooses
/// its copy, so the "which press" question never indexes a list. A copy with no ratingKey names
/// nothing playable and stays inert.
pub fn action_for(row: &AltRow) -> Action {
    if row.rk.is_empty() {
        return Action::None;
    }
    Action::Choose {
        sid: row.sid,
        rk: row.rk.clone(),
    }
}

/// A row's identity: the copy it names (server + ratingKey) and, for a producer that listed the
/// same copy twice, which occurrence — a duplicate is a second row, never a second identity
/// collision. Selection across a rebuild follows this, not a position.
#[derive(Clone, Debug, PartialEq)]
pub struct AltId {
    sid: ServerId,
    rk: String,
    nth: usize,
}

impl AltId {
    /// The identity of the row at `pos` of `rows`.
    fn of(rows: &[AltRow], pos: usize) -> Self {
        let r = &rows[pos];
        let nth = rows[..pos].iter().filter(|o| o.sid == r.sid && o.rk == r.rk).count();
        AltId { sid: r.sid, rk: r.rk.clone(), nth }
    }
}

/// The panel's one section, declared from the resolved rows alone (pure, so the text-fit suite
/// drives it): every row carries its own choice ([`action_for`]). The dynamic rows' focus key
/// is their position in the list (docs/settings-form.md, "Dynamic rows"): the list is one
/// unheaded section whose length and order the store decides.
pub fn form_for(rows: &[AltRow]) -> Form<AltId, Action, Infallible> {
    let mut sec = FormSection::new("");
    for (pos, r) in rows.iter().enumerate() {
        let id = AltId::of(rows, pos);
        sec = sec.item_keyed(id, RowKey(pos as u32), RowKind::Choice, action_for(r), row_for(r));
    }
    Form::new().section(sec)
}

// ---- the surface -----------------------------------------------------------------------------

/// Where the selection lands when the table is rebuilt.
enum Sel {
    /// Opening: on the ticked copy — because that is where the eye already is, and one DOWN
    /// from it is the alternative.
    OnTheTick,
    /// Rebuilding under an OPEN panel: leave the cursor where the user put it.
    Keep,
}

pub struct AltSourcesScreen {
    entry: EntryId,
    arg: AltSourcesArg,
    /// The rows the `TableView` and `dests` were built from — the rebuild stamp (module doc).
    rows: Vec<AltRow>,
    /// The table and what each row does, replaced together ([`form_for`]).
    pub form: FormTable<AltId, Action, Infallible>,
}

impl AltSourcesScreen {
    pub fn new(entry: EntryId, arg: AltSourcesArg, meta: plx_data::metadata::MetadataView<'_>) -> Self {
        let mut screen = Self {
            entry,
            arg,
            rows: Vec::new(),
            form: FormTable::new(plx_ui::table_screen::BAND_BASE),
        };
        screen.rebuild(Sel::OnTheTick, meta);
        screen
    }

    /// The rows the store would produce right now, ticked against the page's PICKED copy if it
    /// picked another one, else the copy the page is standing on.
    ///
    /// The page's server, not `plex::current_server()`. Those were the same thing only while
    /// browsing a shared library also re-pointed `current`; once that stopped, opening a borrowed
    /// film left `current` on our own server, no copy matched the pair, and the panel drew **no
    /// tick at all** — owner-reported. The tick answers "which of these is chosen", and only
    /// the page knows; it says so on the argument.
    fn live_rows(&self, meta: plx_data::metadata::MetadataView<'_>) -> Vec<AltRow> {
        let choice = self.arg.choice.as_ref().map(|c| (c.sid, c.rk.as_str()));
        rows(
            meta.alt_copies(self.arg.sid, &self.arg.rk),
            self.arg.sid,
            &self.arg.rk,
            choice,
        )
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
        if matches!(sel_mode, Sel::OnTheTick) {
            self.rows = self.live_rows(meta);
        }
        let form = form_for(&self.rows);
        // rows carry a sub-line — the track menu's size class, not the account popover's
        self.form.table.compact = false;
        match sel_mode {
            // leave the cursor where the user put it — on the same copy, found by identity
            Sel::Keep => {
                let held = self.form.selected_id().cloned();
                self.form.set(form, held.as_ref());
            }
            // on the ticked copy, else the first row
            Sel::OnTheTick => {
                let here = self.rows.iter().position(|r| r.checked).map(|pos| AltId::of(&self.rows, pos));
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

    /// Commit row `elem` and close. The row only REPORTS which copy: the PAGE remembers the
    /// choice and the hero's Play starts it — nothing navigates here.
    ///
    /// Takes the elem directly rather than reading the table's cursor: the engine's OK arm reads it
    /// off the current focus key and a pointer click's `Activate` names the STOP that was clicked,
    /// which need not be the row the cursor was already resting on — the same distinction
    /// `AccountMenuScreen::activate` draws.
    fn commit<H: AppLike>(&mut self, elem: u32, fx: &mut Effects<'_, H>) {
        let action = match self.form.index_of_key(RowKey(elem)).and_then(|i| self.form.activate(i)) {
            Some(plx_ui::form::Activation::Action(action)) => action,
            _ => Action::None,
        };
        self.dismiss(fx);
        if let Action::Choose { sid, rk } = action {
            // The PAGE remembers. This surface names WHICH copy and nothing else — the
            // *Versions* panel's shape, and what keeps "what a press means on the Detail page"
            // in one place instead of two.
            fx.push(Fx::Deliver(
                MachineId::Instance(self.arg.host),
                Delivery::Screen(ScreenEvent::App(AppMsg::AltSourceChosen { sid, rk })),
            ));
        }
    }
}

impl<H: AppLike<Memory = PageMemory> + crate::registry::MetadataLike> Machine<H> for AltSourcesScreen {
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
                // exactly as `AccountMenuScreen::step` does — `FocusMoved` already keeps the two
                // in step, this is the belt to its suspenders for a focus change this screen was
                // not told about directly (a remembered-cursor seat on `Enter`, say).
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
            // UP/DOWN, OK and the pointer are no longer read here at all — the engine's own
            // direction/OK/hit-map machinery drives them through `Focusable` and the arms above,
            // exactly as it does for every other Engine screen (`AccountMenuScreen` is the
            // worked example this module follows). A click BESIDE every registered row stop is a
            // HIT-MAP MISS, and `Style::Compact`'s `on_miss` (`OnMiss::Dismiss`) is what closes
            // the panel — the same "click outside a popover dismisses it" rule this arm used to
            // implement by hand against raw pointer coordinates.
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

/// **A single-column `Focusable` over the table's own cursor** (phase 12, D2) — the same shape
/// `AccountMenuScreen::groups` uses for its own `TableView`: one `GroupKind::Column` of
/// one `Bare` element per row (OK/click activates on the down edge, never arms a hold —
/// there is nothing here to hold), `EdgeRule::Stop` on every side since this is a standalone
/// surface with no page to escape onto, and `place`/`neighbour` read straight off `TableView`'s
/// own row geometry (`row_frame`, `next_selectable`) rather than a second copy of it.
impl<H: AppLike<Memory = PageMemory>> Focusable<H> for AltSourcesScreen {
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

impl LogicalState for AltSourcesScreen {
    fn write(&self, c: &mut Canon) {
        self.arg.write(c);
        c.seq(self.rows.len());
        for r in &self.rows {
            c.u32(u32::from(r.sid.raw()))
                .str(&r.rk)
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
        out.push_str("alt");
    }
}

impl<H: AppLike<Memory = PageMemory> + crate::registry::MetadataLike> Screen<H> for AltSourcesScreen {
    fn name(&self) -> &'static str {
        "alt"
    }
    fn state(&self) -> &dyn LogicalState {
        self
    }
    fn crumb(&self, _cx: &Cx<'_, H>) -> Option<Cow<'_, str>> {
        None
    }
    fn prepare(&mut self, _b: &mut Budget, _cx: &Cx<'_, H>) {}
    /// The modal dim, asked for rather than drawn. Nothing is lifted back out of it: this picker
    /// hangs under the detail page's Source chip, and the chip is a control that says which copy
    /// is playing rather than the subject of the panel — unlike the card menu's tile or the
    /// profile menu's chip, there is nothing here whose dimming contradicts what the panel is
    /// about.
    ///
    /// **The ordering this replaces was load-bearing and is now the container's** (§16.3): the
    /// panel's ground once sampled the framebuffer (`Glass::CACHED`), so the dim had to be down
    /// before the backdrop was taken; the ground is the latched underlay field (a sheet is never glass; DS6 glass is for menus), latched at the
    /// head of the dims from the undimmed page. `ModalStack::draw_scrims` draws it at the end of the PAGE pass,
    /// which is strictly earlier than the surface pass this `draw` runs in, and multiplies by the
    /// appear spring and by `nav::page_alpha` — the second of which the in-`draw` version could
    /// not reach, `DrawFrame::page_alpha` being the surface's own spring alone.
    fn scrim(&self) -> Scrim {
        // The PANEL role (`theme::underlay::DIM_PANEL`): a picker in the middle of the frame. The
        // page recedes; it is not blanked, and the hero behind stays readable.
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
        plx_ui::profile::phase("dt.alt", || {
            plx_ui::widgets::panel_ground(p, r, PANEL_RAD, field, plx_ui::widgets::Ground::Sheet);
            self.form.table.draw(p, r, measure);
        });
        // The hit map's stops are registered against the SETTLED geometry (`self.frame()`, what
        // `Focusable::place` answers) rather than the transient slide `p` draws with — at rest
        // (`appear == 1`) the two coincide exactly (`RISE * (1.0 - appear)` is 0), and only during
        // the open/close fade would they differ; `ui/hit.rs`'s own module doc already carries the
        // wider version of this gap (no container feeds a fading surface's alpha into the map
        // yet), so this keeps the one known edge rather than inventing a second.
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
#[path = "alt_sources_tests.rs"]
mod tests;
