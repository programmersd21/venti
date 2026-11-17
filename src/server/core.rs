use crossbeam_channel::{Receiver, Sender};
use ratatui::Terminal;
use ratatui::layout::{Position, Rect};
use std::collections::{HashMap, HashSet};
use std::os::unix::net::UnixStream;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::config::schema::{BorderStyle, SplitAxisSetting, WorkspaceBarPosition};
use crate::config::{Config, Theme};
use crate::input::{Command, InputMode, Key, KeyBindings, KeyParser, ParsedInput};
use crate::layout::{Direction, PaneId, SplitAxis, WorkspaceId, choose_aspect_split};
use crate::proto::{MouseButtonKind, MouseEventMsg, ServerMsg, write_msg};

use crate::render::{
    BUILTIN_THEMES, ClientBackend, PaletteState, PaneView, SettingsMenuState, ThemeMenuState,
    filtered_commands, render_help, render_hint_bar, render_palette, render_pane_border,
    render_settings_menu, render_theme_menu, render_toast, render_vim_cmd_bar,
    render_vim_cmd_popup, render_welcome, render_workspace_bar, render_workspace_sidebar,
};
use crate::server::session::Session;

/// internal event stream sent to server core loop.
pub enum CoreEvent {
    ClientInput {
        client_id: usize,
        bytes: Vec<u8>,
    },
    ClientMouse {
        client_id: usize,
        msg: MouseEventMsg,
    },
    ClientPaste {
        client_id: usize,
        bytes: Vec<u8>,
    },
    ClientCommand {
        client_id: usize,
        command: String,
    },
    ClientDetach {
        client_id: usize,
    },
    ClientResize {
        client_id: usize,
        cols: u16,
        rows: u16,
    },
    NewClient {
        client_id: usize,
        stream: std::os::unix::net::UnixStream,
        cols: u16,
        rows: u16,
        term: String,
        cell_pixel: Option<(u16, u16)>,
    },
    ConfigChanged,
}

/// attached client connection state.
pub struct AttachedClient {
    pub id: usize,
    pub stream: UnixStream,
    pub cols: u16,
    pub rows: u16,
    pub terminal: Terminal<ClientBackend>,
    pub needs_full: bool,
    /// graphics this client has already received, as (pane, command seq).
    /// tracked per client: a client that attaches later, or that missed a
    /// frame, must still get the image.
    pub emitted_gfx: std::collections::HashSet<(PaneId, u64)>,
    /// graphics this client has seen hidden (deleted on the host).
    pub hidden_gfx: std::collections::HashSet<(PaneId, u64)>,
}

/// drag state for mouse window manipulation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MouseDragState {
    None,
    MoveFloating {
        pane_id: PaneId,
        offset_x: i32,
        offset_y: i32,
    },
    SplitDrag {
        path: Vec<crate::layout::SplitDirection>,
        axis: crate::layout::SplitAxis,
        start_col: u16,
        start_row: u16,
    },
    ResizeFloatingCorner {
        pane_id: PaneId,
        corner: FloatingCorner,
        origin_rect: Rect,
    },
    /// dragging a tiled pane title to swap with another pane.
    SwapTitleDrag {
        pane_id: PaneId,
        start_col: u16,
        start_row: u16,
        target_id: Option<PaneId>,
    },
}

/// corner of a floating pane being resized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatingCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// core multiplexer server engine.
pub struct ServerCore {
    pub session: Session,
    pub config: Config,
    pub config_path: std::path::PathBuf,
    pub theme: Theme,
    pub help_scroll: usize,
    pub mouse_drag: MouseDragState,
    /// pane awaiting a second close to confirm, when it runs a busy child.
    pub pending_close: Option<PaneId>,
    pub bindings: KeyBindings,
    pub mode: InputMode,
    pub palette: PaletteState,
    pub theme_menu: ThemeMenuState,
    pub context_menu: crate::render::context_menu::ContextMenuState,
    pub settings_menu: SettingsMenuState,
    pub show_welcome: bool,
    pub show_help: bool,
    pub toast: Option<(String, Instant)>,
    pub clients: HashMap<usize, AttachedClient>,
    pub key_parser: KeyParser,
    pending_escape_deadline: Option<Instant>,
    pub event_rx: Receiver<CoreEvent>,
    pub event_tx: Sender<CoreEvent>,
    pub running: Arc<AtomicBool>,
    ui_dirty: bool,
    /// whether the initial pane must wait for a real client's dimensions.
    initial_pane_deferred: bool,
    force_full: bool,
    /// anchored graphics moved; request cheap re-placement from the client
    /// instead of a full redraw.
    reemit_gfx: bool,
    /// set when a sixel/iterm2 image that was on screen got hidden (modal,
    /// scroll out of view, alternate screen swap); the next visible frame
    /// needs an ed-two repaint to wipe the stale pixels before re-placing
    sixel_restore_pending: bool,
    /// per-pane foreground app running state, refreshed every 250 ms;
    /// busy -> idle transitions trigger graphics cleanup for the pane
    pane_app_running: HashMap<PaneId, bool>,
    /// ids (seq) of graphics already transmitted at least once. used to pick
    /// pixel payloads (first send) vs cheap delete/place re-emission.
    emitted_gfx: HashSet<(PaneId, u64)>,
    hidden_gfx: HashSet<(PaneId, u64)>,
    /// delete commands to hand to the next client update (queued when stored
    /// graphics are dropped, so visible images don't ghost onto the host).
    pending_gfx_deletes: Vec<crate::proto::GfxCmd>,
    /// monotonic seq for queued delete commands (keeps them past the client's
    /// seq filter even when connected with other pending graphics).
    gfx_delete_seq: u64,
    gfx_anchors: HashMap<PaneId, (usize, usize)>,
    last_sizes: HashMap<PaneId, (u16, u16)>,
    clock_text: String,
    last_clock_check: Instant,
    last_title_check: Instant,
    /// per-pane "has running foreground work" flag, refreshed every 250 ms
    pane_busy: HashMap<PaneId, bool>,
    theme_before_menu: Option<Theme>,
    /// tab scroll offset for workspace bar when tabs overflow width
    tab_scroll_offset: usize,
    /// whether the vertical sidebar is collapsed to an icon rail (runtime toggle,
    /// initialised from ui.sidebar_collapsed)
    pub sidebar_collapsed: bool,
    /// buffer for vim-style `:command` prompt
    vim_cmd_buf: String,
    /// currently selected completion in vim command popup
    vim_cmd_selected: usize,
    /// pane layout change animations
    pane_animations: crate::render::anim::PaneAnimationManager,
    /// when set, the cat from `:cat` is visiting the focused pane (spawn time).
    cat_started: Option<Instant>,
    /// last time the clock tick was evaluated for cat animation re-renders.
    cat_tick: Instant,
    /// number of times the cat has been booped this visit.
    cat_boops: usize,
    /// when the cat was last booped (summons the heart).
    cat_boop_at: Option<Instant>,
}

impl ServerCore {
    pub fn new(
        session_name: &str,
        config: Config,
        theme: Theme,
        running: Arc<AtomicBool>,
        restore: Option<crate::server::persist::SessionSnapshot>,
    ) -> (Self, Sender<CoreEvent>) {
        Self::new_inner(session_name, config, theme, running, restore, false)
    }

    /// builds the daemon core without starting its first shell at a guessed size.
    pub fn new_for_server(
        session_name: &str,
        config: Config,
        theme: Theme,
        running: Arc<AtomicBool>,
        restore: Option<crate::server::persist::SessionSnapshot>,
    ) -> (Self, Sender<CoreEvent>) {
        Self::new_inner(session_name, config, theme, running, restore, true)
    }

    fn new_inner(
        session_name: &str,
        config: Config,
        theme: Theme,
        running: Arc<AtomicBool>,
        restore: Option<crate::server::persist::SessionSnapshot>,
        defer_initial_pane: bool,
    ) -> (Self, Sender<CoreEvent>) {
        // bound queued client payloads: a protocol frame can be up to 1 MiB.
        // socket reader threads then provide backpressure instead of retaining
        // an unbounded stream of frames while the core is busy rendering.
        let (event_tx, event_rx) = crossbeam_channel::bounded(64);
        let mut bindings = KeyBindings::new(config.profile.mode, &config.input.passthrough);
        bindings.set_disabled(&config.input.disabled_keys);
        if let Err(e) = bindings.apply_custom_binds(&config.binds) {
            tracing::warn!("invalid custom keybindings ignored: {e}");
        }

        let mut session = Session::new(session_name);
        session.monocle = config.layout.mode == crate::config::schema::LayoutMode::Monocle;
        let restored = match restore {
            Some(snap) => session.restore_snapshot_with_limits(
                &snap,
                80,
                24,
                config.general.scrollback,
                &config.general.shell,
                crate::server::session::RestoreLimits {
                    max_panes: config.general.max_panes,
                    max_workspaces: config.general.max_workspaces,
                },
            ),
            None => 0,
        };
        session.set_workspace_names(&config.general.workspace_names);
        for id in 1..=config.general.default_workspaces {
            session
                .workspace_names
                .entry(id)
                .or_insert_with(|| format!("ws{id}"));
        }

        let show_welcome = config.ui.show_welcome;
        let sidebar_collapsed = config.ui.sidebar_collapsed;

        let mut core = Self {
            session,
            config,
            config_path: crate::util::paths::config_path(),
            theme,
            help_scroll: 0,
            mouse_drag: MouseDragState::None,
            pending_close: None,
            bindings,
            mode: InputMode::Normal,
            palette: PaletteState::default(),
            theme_menu: ThemeMenuState::default(),
            context_menu: crate::render::context_menu::ContextMenuState::default(),
            settings_menu: SettingsMenuState::default(),
            show_welcome,
            show_help: false,
            toast: None,
            clients: HashMap::new(),
            key_parser: KeyParser::new(),
            pending_escape_deadline: None,
            event_rx,
            event_tx: event_tx.clone(),
            running,
            ui_dirty: true,
            initial_pane_deferred: defer_initial_pane && restored == 0,
            force_full: true,
            reemit_gfx: false,
            sixel_restore_pending: false,
            pane_app_running: HashMap::new(),
            emitted_gfx: HashSet::new(),
            hidden_gfx: HashSet::new(),
            pending_gfx_deletes: Vec::new(),
            gfx_delete_seq: 0,
            gfx_anchors: HashMap::new(),
            last_sizes: HashMap::new(),
            clock_text: String::new(),
            last_clock_check: Instant::now(),
            last_title_check: Instant::now(),
            pane_busy: HashMap::new(),
            theme_before_menu: None,
            tab_scroll_offset: 0,
            sidebar_collapsed,
            vim_cmd_buf: String::new(),
            vim_cmd_selected: 0,
            pane_animations: crate::render::anim::PaneAnimationManager::default(),
            cat_started: None,
            cat_tick: Instant::now(),
            cat_boops: 0,
            cat_boop_at: None,
        };

        if restored == 0 && !defer_initial_pane {
            core.spawn_pane(None, false);
        }

        (core, event_tx)
    }

    /// runs the core single-threaded event loop.
    pub fn run(&mut self) {
        let frame_interval = Duration::from_millis(self.config.ui.frame_interval_ms.clamp(4, 100));

        while self.running.load(Ordering::Relaxed) {
            match self.event_rx.recv_timeout(frame_interval) {
                Ok(event) => {
                    self.handle_event(event);
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            }

            self.check_exited_panes();

            self.check_alt_exited_panes();

            self.refresh_pane_titles();

            self.check_escape_timeout();

            if self.needs_render() {
                self.render_all();
            }
        }

        self.detach_all(crate::proto::ExitReason::Killed);
    }

    /// hides the kitty placements of a pane on the host without destroying
    /// them: the image data stays, the placement is re-created on the next
    /// repaint. used when a foreground app exits - the image is anchored to
    /// the grid and must survive the app that drew it.
    fn hide_pane_graphics(&mut self, pane_id: PaneId) {
        let Some(pane) = self.session.panes.get_mut(&pane_id) else {
            return;
        };
        let commands: Vec<crate::pane::graphics::GraphicCommand> = pane
            .graphics
            .lock()
            .map(|store| store.active_commands().to_vec())
            .unwrap_or_default();
        for command in commands {
            if command.format != crate::pane::GraphicFormat::Kitty {
                continue;
            }
            let Some(metadata) = command.kitty_metadata.as_ref() else {
                continue;
            };
            if matches!(metadata.a.as_deref(), Some("d") | Some("q")) {
                continue;
            }
            let Some(image_id) = metadata.i else {
                continue;
            };
            self.queue_kitty_delete(image_id, metadata.p);
            // mark the placement as hidden so the next repaint re-places it:
            // the host placement is gone, the image data is not.
            self.hidden_gfx.insert((pane_id, command.seq));
            for client in self.clients.values_mut() {
                client.hidden_gfx.insert((pane_id, command.seq));
            }
        }
    }

    /// queues a kitty delete-placement command for the next client frame.
    /// the image data stays on the host, only the placement is removed.
    fn queue_kitty_delete(&mut self, image_id: u32, placement: Option<u32>) {
        let control = crate::pane::graphics::KittyControlData {
            a: Some("d".to_string()),
            d: Some("i".to_string()),
            i: Some(image_id),
            p: placement,
            q: Some(2),
            ..Default::default()
        };
        let data = crate::pane::graphics::kitty_apc(&control);
        self.gfx_delete_seq = self.gfx_delete_seq.wrapping_add(1).max(1);
        self.pending_gfx_deletes.push(crate::proto::GfxCmd {
            row: 0,
            col: 0,
            format: crate::proto::GfxFormat::Kitty,
            seq: self.gfx_delete_seq,
            data,
        });
    }

    /// tears down graphics for a pane being removed, including virtual image
    /// placeholders whose text cells will not survive the redraw.
    fn drop_pane_graphics_hard(&mut self, pane_id: PaneId) {
        let mut ids: Vec<u32> = self
            .session
            .panes
            .get(&pane_id)
            .and_then(|pane| {
                pane.graphics.lock().ok().map(|store| {
                    store
                        .active_commands()
                        .iter()
                        .filter_map(|cmd| {
                            if cmd.format == crate::pane::GraphicFormat::Kitty {
                                cmd.kitty_metadata.as_ref()?.i
                            } else {
                                None
                            }
                        })
                        .collect()
                })
            })
            .unwrap_or_default();
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            self.queue_kitty_delete(id, None);
        }
        if let Some(pane) = self.session.panes.get(&pane_id)
            && let Ok(mut store) = pane.graphics.lock()
        {
            store.clear();
        }
        self.emitted_gfx.retain(|&(pid, _)| pid != pane_id);
        self.hidden_gfx.retain(|&(pid, _)| pid != pane_id);
    }

    fn note_layout_change(&mut self) {
        // a plain repaint never touches image pixels; hide/reveal handling
        // lives in render_all and per-pane geometry changes drop placements
        // through their own paths (resize / scroll / pane exit).
        self.force_full = true;
        self.ui_dirty = true;
    }

    /// captures the current pane geometry for animation replay.
    fn capture_layout_rects(&self) -> Vec<(PaneId, Rect)> {
        let (cols, rows) = self.effective_size();
        let (_, _, _, content) = self.chrome_areas(Rect::new(0, 0, cols, rows));
        self.session
            .layout(content, self.gap_inner(), self.gap_outer())
    }

    /// replays a pane-rect delta: any pane that changed geometry since
    /// `before` gets an animation from its old position to the new one.
    fn animate_layout_delta(&mut self, before: Vec<(PaneId, Rect)>) {
        let after = self.capture_layout_rects();
        let duration_ms = self.config.animation.layout_duration_ms;
        let easing = self.config.animation.layout_easing;
        let enabled = self.config.animation.enabled;
        for (id, from_rect) in before {
            if let Some((_, to_rect)) = after.iter().find(|(pid, _)| *pid == id)
                && from_rect != *to_rect
            {
                self.pane_animations.animate_pane(
                    id,
                    from_rect,
                    *to_rect,
                    duration_ms,
                    easing,
                    enabled,
                );
            }
        }
    }

    /// shifts stored graphic commands after pane content scrolls, and detects
    /// scrollback navigation. returns true when any pane holding graphics was
    /// shifted; callers should force a full redraw in that case.
    fn shift_graphics_for_scroll(&mut self) -> bool {
        let pane_ids: Vec<PaneId> = self.session.panes.keys().copied().collect();
        let mut moved = false;
        for id in pane_ids {
            let Some(pane) = self.session.panes.get(&id) else {
                continue;
            };
            let cur_base = pane
                .parser
                .lock()
                .map(|mut p| crate::pane::search::scrollback_len(&mut p))
                .unwrap_or(0);
            let cur_off = pane.scroll_offset;
            let prev = self
                .gfx_anchors
                .get(&id)
                .copied()
                .unwrap_or((cur_base, cur_off));
            // anchors track the vt100 grid; scroll offset is applied at
            // paint time (row + scroll_offset), so only content growth moves
            // them here. shifting for scroll-back double-counted the offset
            // and the image was never visible again afterwards.
            let grew = cur_base.saturating_sub(prev.0) as i32;
            let shift = grew;
            if shift != 0
                && let Ok(mut store) = pane.graphics.lock()
            {
                let before = store.active_commands().len();
                store.scroll_rows(shift);
                // a placement anchored further above the grid than the
                // retained scrollback is gone for good: free it and tell the
                // host to drop the pixels (zellij frees the image once its
                // last placement leaves the history).
                let limit = -(self.config.general.scrollback as i32);
                let freed = store.drop_scrolled_out(limit);
                for command in freed {
                    let Some(metadata) = command.kitty_metadata.as_ref() else {
                        continue;
                    };
                    if command.format != crate::pane::GraphicFormat::Kitty {
                        continue;
                    }
                    let Some(image_id) = metadata.i else {
                        continue;
                    };
                    let control = crate::pane::graphics::KittyControlData {
                        a: Some("d".to_string()),
                        d: Some("I".to_string()),
                        i: Some(image_id),
                        q: Some(2),
                        ..Default::default()
                    };
                    let data = crate::pane::graphics::kitty_apc(&control);
                    self.gfx_delete_seq = self.gfx_delete_seq.wrapping_add(1).max(1);
                    self.pending_gfx_deletes.push(crate::proto::GfxCmd {
                        row: 0,
                        col: 0,
                        format: crate::proto::GfxFormat::Kitty,
                        seq: self.gfx_delete_seq,
                        data,
                    });
                    self.emitted_gfx.remove(&(id, command.seq));
                    self.hidden_gfx.remove(&(id, command.seq));
                    for client in self.clients.values_mut() {
                        client.emitted_gfx.remove(&(id, command.seq));
                        client.hidden_gfx.remove(&(id, command.seq));
                    }
                }
                if before > 0 || !store.active_commands().is_empty() {
                    moved = true;
                }
            }
            self.gfx_anchors.insert(id, (cur_base, cur_off));
        }
        self.gfx_anchors
            .retain(|id, _| self.session.panes.contains_key(id));
        moved
    }

    fn needs_render(&mut self) -> bool {
        if self.clients.is_empty() {
            return false;
        }
        if self.ui_dirty || self.clients.values().any(|c| c.needs_full) {
            return true;
        }
        if self
            .session
            .panes
            .values()
            .any(|p| p.dirty.load(Ordering::Acquire) || p.gfx_dirty.load(Ordering::Acquire))
        {
            return true;
        }
        // re-render once when a transient toast expires so it is cleared.
        if let Some((_, created)) = &self.toast
            && created.elapsed() > Duration::from_secs(3)
        {
            return true;
        }
        // keep rendering while pane layout animations are active
        if self.pane_animations.has_active() {
            return true;
        }
        // the cat animates at ~7 fps while it is visiting
        if self.cat_started.is_some() && self.cat_tick.elapsed() >= Duration::from_millis(140) {
            self.cat_tick = Instant::now();
            return true;
        }
        // the clock only changes once per second at most; check periodically so
        // an otherwise idle session still updates its status bar.
        if self.config.ui.clock && self.last_clock_check.elapsed() >= Duration::from_secs(1) {
            self.last_clock_check = Instant::now();
            let now = crate::render::clock_text(&self.config.ui.clock_format);
            if now != self.clock_text {
                return true;
            }
        }
        false
    }

    /// returns true when any pane in the workspace is running a foreground child.
    fn workspace_busy(&self, ws: usize) -> bool {
        self.session
            .workspace_pane_ids(ws)
            .into_iter()
            .any(|id| self.pane_busy.get(&id).copied().unwrap_or(false))
    }

    fn refresh_pane_titles(&mut self) {
        if self.last_title_check.elapsed() < Duration::from_millis(250) {
            return;
        }
        self.last_title_check = Instant::now();
        let mut changed = false;
        let show_hostname = self.config.ui.show_hostname;
        let show_pwd = self.config.ui.show_pwd;

        let host = if show_hostname {
            static HOSTNAME: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
            HOSTNAME
                .get_or_init(|| {
                    nix::unistd::gethostname()
                        .ok()
                        .and_then(|h| h.into_string().ok())
                })
                .as_deref()
        } else {
            None
        };

        self.pane_busy
            .retain(|id, _| self.session.panes.contains_key(id));
        self.pane_app_running
            .retain(|id, _| self.session.panes.contains_key(id));
        let mut app_exited: Vec<PaneId> = Vec::new();
        for pane in self.session.panes.values() {
            let busy = pane
                .child_pid
                .map(crate::util::process::has_foreground_app)
                .unwrap_or(false);
            let was = self.pane_app_running.insert(pane.id, busy).unwrap_or(false);
            if was && !busy {
                app_exited.push(pane.id);
            }
        }
        for pane in self.session.panes.values_mut() {
            let busy = pane
                .child_pid
                .map(crate::util::process::has_busy_children)
                .unwrap_or(false);
            self.pane_busy.insert(pane.id, busy);
        }
        // a foreground app just quit (busy -> idle). its kitty placements are
        // hidden on the host, not destroyed: the image data stays in the
        // store and the placement is re-created on the next repaint, so a
        // logo drawn before `cat`/`icat`/any command survived the command
        // finishing. deleting the store here wiped every image in the pane
        // each time a command exited.
        if !app_exited.is_empty() {
            for id in &app_exited {
                self.hide_pane_graphics(*id);
            }
            self.note_layout_change();
        }

        for pane in self.session.panes.values_mut() {
            if let Some(pid) = pane.child_pid
                && let Some(name) = crate::util::process::foreground_process_name(pid)
            {
                let mut title = name.clone();

                if let Some(host) = host {
                    title = format!("{}@{}", title, host);
                }

                if show_pwd && let Some(pwd) = crate::util::process::process_cwd(pid) {
                    title = format!("{} {}", title, pwd);
                }

                if self.config.ui.show_time_in_title {
                    let now = time::OffsetDateTime::now_local()
                        .unwrap_or_else(|_| time::OffsetDateTime::now_utc());
                    let time_str = if self.config.ui.clock_format == "12h" {
                        let hour12 = now.hour() % 12;
                        format!(
                            "{:02}:{:02}{}",
                            if hour12 == 0 { 12 } else { hour12 },
                            now.minute(),
                            if now.hour() >= 12 { "pm" } else { "am" }
                        )
                    } else {
                        format!("{:02}:{:02}", now.hour(), now.minute())
                    };
                    title = format!("{} {}", title, time_str);
                }

                if title != pane.title {
                    pane.title = title;
                    changed = true;
                }
            }
        }
        if changed {
            self.ui_dirty = true;
        }
    }

    /// hard-drop images anchored in any pane whose alt-screen application
    /// just exited (the vt100 returns to the main grid). this covers TUIs
    /// (kew, vim, btop) that skip their own kitty cleanup on quit.
    fn check_alt_exited_panes(&mut self) {
        let ids: Vec<PaneId> = self
            .session
            .panes
            .iter()
            .filter(|(_, pane)| pane.alt_exited.swap(false, Ordering::Acquire))
            .map(|(id, _)| *id)
            .collect();
        if ids.is_empty() {
            return;
        }
        self.drop_alternate_screen_graphics(&ids);
        // also queue a soft reset: sixel/iterm2 images lay outside
        // the cell grid model, and the ED2+full repaint that follows is the
        // erase we can trigger from inside without the app's cooperation.
        for client in self.clients.values_mut() {
            client.terminal.backend_mut().set_pending_reset();
        }
        self.force_full = true;
        self.ui_dirty = true;
    }

    /// hides the kitty placements an alt-screen application left behind.
    ///
    /// the placements are NOT removed from the pane's store: the host keeps
    /// the image data and the next full repaint re-places the placement, so a
    /// logo drawn before the TUI ran comes back when the TUI exits. only the
    /// host-side placement is deleted, the pixels stay.
    fn drop_alternate_screen_graphics(&mut self, pane_ids: &[PaneId]) {
        for id in pane_ids {
            let Some(pane) = self.session.panes.get_mut(id) else {
                continue;
            };
            let removed = pane
                .graphics
                .lock()
                .map(|store| store.alternate_screen_placements())
                .unwrap_or_default();
            for command in removed {
                let Some(metadata) = command.kitty_metadata.as_ref() else {
                    continue;
                };
                if matches!(metadata.a.as_deref(), Some("d") | Some("q")) {
                    continue;
                }
                let Some(image_id) = metadata.i else {
                    continue;
                };
                self.queue_kitty_delete(image_id, metadata.p);
            }
        }
    }

    fn check_exited_panes(&mut self) {
        let exited_ids: Vec<PaneId> = self
            .session
            .panes
            .iter()
            .filter_map(|(&id, pane)| {
                if pane.child_exited.load(Ordering::Acquire) {
                    Some(id)
                } else {
                    None
                }
            })
            .collect();

        for id in exited_ids {
            // delete the dying pane's graphics before it is removed, so its
            // images (including U=1 placeholders) don't linger on the host.
            self.drop_pane_graphics_hard(id);
            self.session.close_pane(id);
            self.force_full = true;
            self.ui_dirty = true;
        }

        // if all panes in the session have exited, terminate cleanly
        if self.session.panes.is_empty() && !self.initial_pane_deferred {
            self.running.store(false, Ordering::Relaxed);
        }
    }

    /// feeds an event into the core loop (public for integration tests).
    pub fn handle_event(&mut self, event: CoreEvent) {
        match event {
            CoreEvent::NewClient {
                client_id,
                stream,
                cols,
                rows,
                term,
                cell_pixel,
            } => {
                crate::util::outer_term::set_cell_pixel_size(cell_pixel);
                self.session.outer_graphics = crate::util::outer_term::from_client_term(&term);
                // upgrade every existing pane's probe so a pane spawned before
                // the client attached gets the attached terminal's real
                // graphics capabilities instead of "unknown" forever
                let code = crate::pane::query::graphics_code(self.session.outer_graphics);
                for pane in self.session.panes.values() {
                    pane.graphics_probe
                        .store(code, std::sync::atomic::Ordering::Relaxed);
                }
                self.session.outer_term = term;
                let backend = ClientBackend::new(cols, rows);
                // a client we cannot build a backend for is dropped, not registered;
                // registering would pin a dead client forever.
                if let Ok(terminal) = Terminal::new(backend) {
                    let mut client = AttachedClient {
                        id: client_id,
                        stream,
                        cols,
                        rows,
                        terminal,
                        needs_full: true,
                        emitted_gfx: std::collections::HashSet::new(),
                        hidden_gfx: std::collections::HashSet::new(),
                    };
                    // backpressure policy: a client that cannot drain a full update
                    // within 2 seconds is considered broken and is dropped.
                    if let Err(e) = client
                        .stream
                        .set_write_timeout(Some(Duration::from_secs(2)))
                    {
                        tracing::warn!(
                            "client {client_id}: failed to set write timeout ({e}); a wedged client may never be detected"
                        );
                    }
                    let hello = ServerMsg::HelloAck {
                        session: self.session.name.clone(),
                        graphics: crate::pane::query::graphics_code(self.session.outer_graphics),
                    };
                    if write_msg(&mut client.stream, &hello).is_err() {
                        tracing::error!(
                            "failed to send helloack to client {client_id}; dropping it"
                        );
                    } else {
                        self.clients.insert(client_id, client);
                        if self.initial_pane_deferred {
                            self.initial_pane_deferred = false;
                            if self
                                .session
                                .workspace_pane_ids(self.session.active_workspace)
                                .is_empty()
                            {
                                self.spawn_pane(None, false);
                            }
                        }
                        self.ui_dirty = true;
                    }
                } else {
                    tracing::error!("failed to initialize client {client_id}; dropping it");
                }
            }
            CoreEvent::ClientDetach { client_id } => {
                if self.clients.remove(&client_id).is_some() && !self.clients.is_empty() {
                    // effective geometry may grow when the smallest client leaves.
                    self.force_full = true;
                }
                self.ui_dirty = true;
            }
            CoreEvent::ClientResize {
                client_id,
                cols,
                rows,
            } => {
                if let Some(client) = self.clients.get_mut(&client_id) {
                    client.cols = cols.max(1);
                    client.rows = rows.max(1);
                    client
                        .terminal
                        .backend_mut()
                        .set_dimensions(cols.max(1), rows.max(1));
                }
                // effective geometry is the minimum across clients; a resize
                // can reflow every client's viewport.
                self.force_full = true;
                self.ui_dirty = true;
            }
            CoreEvent::ClientInput {
                client_id: _,
                bytes,
            } => {
                let inputs = self.key_parser.parse_bytes(&bytes);
                for input in inputs {
                    self.dispatch_input(input);
                }
                self.pending_escape_deadline = self
                    .key_parser
                    .has_pending_escape()
                    .then(|| Instant::now() + Duration::from_millis(35));
            }
            CoreEvent::ClientPaste {
                client_id: _,
                bytes,
            } => {
                self.write_paste_to_focused(&bytes);
            }
            CoreEvent::ClientCommand {
                client_id: _,
                command,
            } => {
                if let Ok(cmd) = Command::from_str(&command) {
                    self.execute_command(cmd);
                } else {
                    self.toast = Some((format!("unknown command: {command}"), Instant::now()));
                    self.ui_dirty = true;
                }
            }
            CoreEvent::ClientMouse { client_id: _, msg } => {
                self.handle_mouse(msg);
            }
            CoreEvent::ConfigChanged => self.reload_config(),
        }
    }

    fn check_escape_timeout(&mut self) {
        if self
            .pending_escape_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.pending_escape_deadline = None;
            if let Some(input) = self.key_parser.flush_pending_escape() {
                self.dispatch_input(input);
            }
        }
    }

    fn dispatch_input(&mut self, input: ParsedInput) {
        match input {
            ParsedInput::Key(key) => self.handle_key(key),
            ParsedInput::Mouse(msg) => self.handle_mouse(msg),
            ParsedInput::Paste(bytes) => self.write_paste_to_focused(&bytes),
            // sequences the outer terminal emits that venti cannot make sense of
            // must not be injected into the pane; the shell would render them
            // as garbage on the content grid.
            ParsedInput::Raw(bytes) => {
                tracing::debug!("dropping unparseable outer-terminal bytes: {bytes:?}");
            }
            ParsedInput::FocusGained | ParsedInput::FocusLost => {}
        }
    }

    fn handle_key(&mut self, key: Key) {
        if self.show_welcome {
            self.show_welcome = false;
            self.ui_dirty = true;

            match key.code {
                crate::input::KeyCode::Char('1') => {
                    self.execute_command(Command::NewPane);
                    return;
                }
                crate::input::KeyCode::Char('2') => {
                    self.execute_command(Command::SplitDown);
                    return;
                }
                crate::input::KeyCode::Char('3') => {
                    // quick tour - show help screen
                    self.show_help = true;
                    self.force_full = true;
                    return;
                }
                crate::input::KeyCode::Char('4') => {
                    self.open_config_in_editor();
                    return;
                }
                crate::input::KeyCode::Char('5') => {
                    self.execute_command(Command::ThemeMenu);
                    return;
                }
                _ => {
                    // any other key just dismisses welcome
                    return;
                }
            }
        }
        if self.show_help {
            self.ui_dirty = true;
            let max_scroll = crate::render::help::HELP_ITEMS.saturating_sub(1);
            match key.code {
                crate::input::KeyCode::Up | crate::input::KeyCode::Char('k') => {
                    self.help_scroll = self.help_scroll.saturating_sub(1);
                }
                crate::input::KeyCode::Down | crate::input::KeyCode::Char('j') => {
                    if self.help_scroll < max_scroll {
                        self.help_scroll = self.help_scroll.saturating_add(1);
                    }
                }
                crate::input::KeyCode::PageUp => {
                    self.help_scroll = self.help_scroll.saturating_sub(5);
                }
                crate::input::KeyCode::PageDown => {
                    self.help_scroll = (self.help_scroll + 5).min(max_scroll);
                }
                crate::input::KeyCode::Home => {
                    self.help_scroll = 0;
                }
                crate::input::KeyCode::End => {
                    self.help_scroll = max_scroll;
                }
                crate::input::KeyCode::Esc
                | crate::input::KeyCode::Char('q')
                | crate::input::KeyCode::Enter
                | crate::input::KeyCode::Space => {
                    self.show_help = false;
                    self.help_scroll = 0;
                    self.force_full = true;
                }
                _ => {
                    self.show_help = false;
                    self.help_scroll = 0;
                    self.force_full = true;
                }
            }
            return;
        }
        match self.mode {
            InputMode::Palette => self.handle_palette_key(key),
            InputMode::ThemeMenu => self.handle_theme_menu_key(key),
            InputMode::ContextMenu => self.handle_context_menu_key(key),
            InputMode::SettingsMenu => self.handle_settings_menu_key(key),
            InputMode::Copy => self.handle_copy_key(key),
            InputMode::Search => self.handle_search_key(key),
            InputMode::Resize => {
                self.ui_dirty = true;
                match key.code {
                    crate::input::KeyCode::Esc | crate::input::KeyCode::Enter => {
                        self.mode = InputMode::Normal;
                    }
                    crate::input::KeyCode::Char('h') | crate::input::KeyCode::Left => {
                        self.resize(Direction::Left);
                    }
                    crate::input::KeyCode::Down => {
                        self.resize(Direction::Down);
                    }
                    // j/k follow vim: j resizes down, k resizes up
                    crate::input::KeyCode::Char('j') => {
                        self.resize(Direction::Down);
                    }
                    crate::input::KeyCode::Char('k') => {
                        self.resize(Direction::Up);
                    }
                    crate::input::KeyCode::Up => {
                        self.resize(Direction::Up);
                    }
                    crate::input::KeyCode::Char('l') | crate::input::KeyCode::Right => {
                        self.resize(Direction::Right);
                    }
                    _ => {}
                }
            }
            InputMode::VimCmd => {
                self.handle_vim_cmd_key(key);
            }
            _ => {
                let pane_focused = self.session.focused_pane.is_some();
                if let Some(cmd) = self.bindings.find(InputMode::Normal, &key).cloned() {
                    self.execute_command(cmd);
                } else if key.code == crate::input::KeyCode::Char(':')
                    && !key.modifiers.ctrl
                    && !key.modifiers.alt
                    && !key.modifiers.shift
                {
                    self.vim_cmd_buf.clear();
                    self.mode = InputMode::VimCmd;
                    self.ui_dirty = true;
                } else if pane_focused || !self.session.panes.is_empty() {
                    self.write_to_focused(&key.to_bytes());
                }
            }
        }
    }

    fn handle_palette_key(&mut self, key: Key) {
        use crate::input::KeyCode;
        self.ui_dirty = true;
        match key.code {
            KeyCode::Esc => {
                self.mode = InputMode::Normal;
                self.force_full = true;
            }
            KeyCode::Enter => {
                // if the typed text parses as a command (with args, e.g.
                // "workspace_goto 3"), execute the typed text directly;
                // otherwise execute the selected suggestion.
                let query = self.palette.query.trim().to_string();
                let typed = Command::from_str(&query).ok();
                let matches = filtered_commands(&self.palette.query);
                let selected = matches
                    .get(self.palette.selected_idx)
                    .and_then(|s| Command::from_str(s).ok());
                let chosen = typed.or(selected);
                self.mode = InputMode::Normal;
                self.force_full = true;
                if let Some(cmd) = chosen {
                    self.execute_command(cmd);
                } else if !query.is_empty() {
                    self.toast = Some((format!("unknown command: {query}"), Instant::now()));
                    self.ui_dirty = true;
                }
            }
            KeyCode::Up => {
                self.palette.selected_idx = self.palette.selected_idx.saturating_sub(1);
            }
            KeyCode::Down => {
                let len = filtered_commands(&self.palette.query).len();
                if len > 0 {
                    self.palette.selected_idx =
                        (self.palette.selected_idx + 1).min(len.saturating_sub(1));
                }
            }
            KeyCode::Backspace => {
                self.palette.query.pop();
                self.palette.selected_idx = 0;
            }
            KeyCode::Char(c) => {
                self.palette.query.push(c);
                self.palette.selected_idx = 0;
            }
            KeyCode::Space => {
                self.palette.query.push(' ');
                self.palette.selected_idx = 0;
            }
            _ => {}
        }
    }

    /// tab context menu: right-clicking a workspace tab opens this menu.
    /// entries act on the tab that was clicked (not the focused one).
    fn handle_context_menu_key(&mut self, key: Key) {
        use crate::input::KeyCode;
        use crate::render::context_menu::{action_for_row, row_for_accel, step_row};
        self.ui_dirty = true;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.context_menu.selected_idx = step_row(self.context_menu.selected_idx, -1);
                self.force_full = true;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.context_menu.selected_idx = step_row(self.context_menu.selected_idx, 1);
                self.force_full = true;
            }
            KeyCode::Enter | KeyCode::Space => {
                let row = self.context_menu.selected_idx;
                if let Some(action) = action_for_row(row) {
                    let target = self.context_menu.target_ws;
                    self.mode = InputMode::Normal;
                    self.run_context_menu_action(action, target);
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = InputMode::Normal;
                self.force_full = true;
            }
            KeyCode::Char(accel) if key.modifiers == crate::input::Modifiers::NONE => {
                if let Some(row) = row_for_accel(accel) {
                    self.context_menu.selected_idx = row;
                    if let Some(action) = action_for_row(row) {
                        let target = self.context_menu.target_ws;
                        self.mode = InputMode::Normal;
                        self.run_context_menu_action(action, target);
                    }
                }
            }
            _ => {}
        }
    }

    /// executes a context-menu action against workspace `target`.
    fn run_context_menu_action(
        &mut self,
        action: crate::render::context_menu::ContextMenuAction,
        target: usize,
    ) {
        use crate::render::context_menu::ContextMenuAction;
        match action {
            ContextMenuAction::RenameWorkspace => {
                self.goto_workspace(target);
                self.vim_cmd_buf.clear();
                self.vim_cmd_buf.push_str(":rename_workspace ");
                self.vim_cmd_selected = 0;
                self.mode = InputMode::VimCmd;
            }
            ContextMenuAction::NewWorkspace => {
                let ws = self.new_workspace_tab();
                self.toast = Some((format!("workspace {ws} created"), Instant::now()));
            }
            ContextMenuAction::CloseWorkspace => {
                self.close_workspace(target);
            }
        }
        self.force_full = true;
        self.ui_dirty = true;
    }

    /// shared guts for creating a workspace + first pane (used by the + button
    /// on the tab bar/sidebar and by the context menu).
    /// the workspace `delta` steps away from the active one, wrapping around.
    fn step_workspace(&self, delta: isize) -> Option<usize> {
        let ids = self.session.workspace_ids();
        let len = ids.len();
        let pos = ids
            .iter()
            .position(|w| *w == self.session.active_workspace)?;
        let next = (pos as isize + delta).rem_euclid(len as isize) as usize;
        Some(ids[next])
    }

    fn new_workspace_tab(&mut self) -> usize {
        let max_ws = self.config.general.max_workspaces.max(1);
        if self.session.workspace_ids().len() >= max_ws {
            return self.session.active_workspace;
        }
        let ws = self.session.new_workspace();
        self.spawn_pane(None, false);
        ws
    }

    fn handle_theme_menu_key(&mut self, key: Key) {
        use crate::input::KeyCode;
        self.ui_dirty = true;
        match key.code {
            KeyCode::Esc => {
                if let Some(previous) = self.theme_before_menu.take() {
                    self.theme = previous;
                }
                self.mode = InputMode::Normal;
                self.force_full = true;
            }
            KeyCode::Up => {
                let next = self.theme_menu.selected_idx.saturating_sub(1);
                self.theme_menu.selected_idx = next;
                self.preview_theme_selection();
                self.force_full = true;
            }
            KeyCode::Down => {
                let next =
                    (self.theme_menu.selected_idx + 1).min(BUILTIN_THEMES.len().saturating_sub(1));
                self.theme_menu.selected_idx = next;
                self.preview_theme_selection();
                self.force_full = true;
            }
            KeyCode::Enter => {
                if let Some(name) = BUILTIN_THEMES.get(self.theme_menu.selected_idx)
                    && let Some(theme) = Theme::builtin(name)
                {
                    self.theme = theme;
                    let previous = self.config.clone();
                    self.config.theme.name = (*name).to_string();
                    self.theme_before_menu = None;
                    self.mode = InputMode::Normal;
                    self.save_config(previous);
                    self.toast = Some((format!("theme: {name}"), Instant::now()));
                    self.ui_dirty = true;
                    self.force_full = true;
                }
            }
            _ => {}
        }
    }

    fn handle_settings_menu_key(&mut self, key: Key) {
        use crate::input::KeyCode;
        use crate::render::settings_menu::{Setting, SettingEdit, SettingTab};
        self.ui_dirty = true;

        if let Some(ref mut edit) = self.settings_menu.editing {
            match key.code {
                KeyCode::Esc => {
                    self.settings_menu.editing = None;
                }
                KeyCode::Enter => {
                    let setting = edit.setting;
                    let value_str = edit.buffer.trim().to_string();
                    let previous = self.config.clone();
                    let result = setting.apply(&mut self.config, &value_str);
                    match result {
                        Ok(()) => {
                            self.save_config(previous);
                            self.settings_menu.editing = None;
                            self.toast =
                                Some((format!("{} updated", setting.name()), Instant::now()));
                            self.force_full = true;
                        }
                        Err(msg) => {
                            self.toast = Some((format!("error: {msg}"), Instant::now()));
                        }
                    }
                }
                KeyCode::Left | KeyCode::Right if edit.is_numeric => {
                    // arrow key increment/decrement for numeric settings
                    let step = if key.modifiers.shift { 10 } else { 1 };
                    let delta = if key.code == KeyCode::Right {
                        step
                    } else {
                        -step
                    };

                    if let Ok(mut val) = edit.buffer.trim().parse::<i64>() {
                        val = val.saturating_add(delta).max(0);
                        edit.buffer = val.to_string();
                    }
                }
                KeyCode::Backspace => {
                    edit.buffer.pop();
                }
                KeyCode::Char(c) => {
                    edit.buffer.push(c);
                }
                _ => {}
            }
        } else {
            let filtered: Vec<Setting> = self.settings_menu.visible_settings();

            let max_idx = filtered.len().saturating_sub(1);

            match key.code {
                KeyCode::Esc => {
                    if !self.settings_menu.filter.is_empty() {
                        self.settings_menu.filter.clear();
                        self.settings_menu.selected_idx = 0;
                        self.settings_menu.scroll_offset = 0;
                    } else {
                        self.mode = InputMode::Normal;
                        self.force_full = true;
                    }
                }
                // tab switching with Tab key or number keys
                KeyCode::Tab => {
                    let next_tab = if key.modifiers.shift {
                        // shift+Tab goes backwards
                        match self.settings_menu.active_tab {
                            SettingTab::Quick => SettingTab::Advanced,
                            SettingTab::Visuals => SettingTab::Quick,
                            SettingTab::Layout => SettingTab::Visuals,
                            SettingTab::Advanced => SettingTab::Layout,
                        }
                    } else {
                        // tab goes forwards
                        match self.settings_menu.active_tab {
                            SettingTab::Quick => SettingTab::Visuals,
                            SettingTab::Visuals => SettingTab::Layout,
                            SettingTab::Layout => SettingTab::Advanced,
                            SettingTab::Advanced => SettingTab::Quick,
                        }
                    };
                    self.settings_menu.active_tab = next_tab;
                    self.settings_menu.selected_idx = 0;
                    self.settings_menu.scroll_offset = 0;
                }
                // alt+1-4 jump to a tab; plain digits go to the search filter.
                // same convention as alt+1-9 (goto workspace) in normal mode.
                KeyCode::Char(c @ '1'..='4') if key.modifiers.alt => {
                    self.settings_menu.active_tab = match c {
                        '1' => SettingTab::Quick,
                        '2' => SettingTab::Visuals,
                        '3' => SettingTab::Layout,
                        _ => SettingTab::Advanced,
                    };
                    self.settings_menu.selected_idx = 0;
                    self.settings_menu.scroll_offset = 0;
                }
                KeyCode::Up => {
                    if self.settings_menu.selected_idx > 0 {
                        self.settings_menu.selected_idx -= 1;
                        if self.settings_menu.selected_idx < self.settings_menu.scroll_offset {
                            self.settings_menu.scroll_offset = self.settings_menu.selected_idx;
                        }
                    }
                }
                KeyCode::Down => {
                    if self.settings_menu.selected_idx < max_idx {
                        self.settings_menu.selected_idx += 1;
                        let max_visible = 18;
                        if self.settings_menu.selected_idx
                            >= self.settings_menu.scroll_offset + max_visible
                        {
                            self.settings_menu.scroll_offset = self
                                .settings_menu
                                .selected_idx
                                .saturating_sub(max_visible - 1);
                        }
                    }
                }
                KeyCode::Enter => {
                    // enter always enters edit mode (for text/numeric) or cycles enums
                    let selected_idx = self
                        .settings_menu
                        .selected_idx
                        .min(filtered.len().saturating_sub(1));
                    if let Some(setting) = filtered.get(selected_idx) {
                        if setting.is_bool() {
                            // for bools, Enter enters edit mode to pick true/false
                            let current = setting.current_value(&self.config);
                            let is_numeric = false;
                            self.settings_menu.editing =
                                Some(SettingEdit::new(*setting, current, is_numeric));
                        } else if setting.is_enum() {
                            // for enums, Enter enters edit mode to pick a value
                            let current = setting.current_value(&self.config);
                            let is_numeric = false;
                            self.settings_menu.editing =
                                Some(SettingEdit::new(*setting, current, is_numeric));
                        } else {
                            // for text/numeric, enter edit mode
                            let current = setting.current_value(&self.config);
                            let is_numeric = setting.supports_arrow_increment();
                            self.settings_menu.editing =
                                Some(SettingEdit::new(*setting, current, is_numeric));
                        }
                    }
                }
                KeyCode::Space => {
                    // space toggles bools or cycles enums
                    // ensure selected_idx is within bounds
                    let selected_idx = self
                        .settings_menu
                        .selected_idx
                        .min(filtered.len().saturating_sub(1));
                    if let Some(setting) = filtered.get(selected_idx) {
                        if setting.is_bool() {
                            let now_true = setting.current_value(&self.config) == "true";
                            let previous = self.config.clone();
                            let _ = setting
                                .apply(&mut self.config, if now_true { "false" } else { "true" });
                            self.save_config(previous);
                            self.toast =
                                Some((format!("{} toggled", setting.name()), Instant::now()));
                            self.force_full = true;
                        } else if setting.is_enum() {
                            let opts = setting.enum_options();
                            if !opts.is_empty() {
                                let cur = setting.current_value(&self.config);
                                let pos = opts.iter().position(|o| *o == cur).unwrap_or(0);
                                let next = opts[(pos + 1) % opts.len()];
                                let previous = self.config.clone();
                                match setting.apply(&mut self.config, next) {
                                    Ok(()) => {
                                        self.save_config(previous);
                                        self.toast = Some((
                                            format!("{} updated", setting.name()),
                                            Instant::now(),
                                        ));
                                        self.force_full = true;
                                    }
                                    Err(msg) => {
                                        self.toast =
                                            Some((format!("error: {msg}"), Instant::now()));
                                    }
                                }
                            }
                        }
                    }
                }
                KeyCode::Left | KeyCode::Right => {
                    // left/Right cycles through enum values
                    if let Some(setting) = filtered.get(self.settings_menu.selected_idx)
                        && setting.is_enum()
                    {
                        let opts = setting.enum_options();
                        if !opts.is_empty() {
                            let cur = setting.current_value(&self.config);
                            let pos = opts.iter().position(|o| *o == cur).unwrap_or(0);
                            let next_pos = if key.code == KeyCode::Right {
                                (pos + 1) % opts.len()
                            } else {
                                (pos + opts.len() - 1) % opts.len()
                            };
                            let next = opts[next_pos];
                            let previous = self.config.clone();
                            match setting.apply(&mut self.config, next) {
                                Ok(()) => {
                                    self.save_config(previous);
                                    self.toast = Some((
                                        format!("{} updated", setting.name()),
                                        Instant::now(),
                                    ));
                                    self.force_full = true;
                                }
                                Err(msg) => {
                                    self.toast = Some((format!("error: {msg}"), Instant::now()));
                                }
                            }
                        }
                    }
                }
                KeyCode::Char(c) if !key.modifiers.ctrl && !key.modifiers.alt && c != ' ' => {
                    // only add to filter, no quick toggle keys, and space is reserved for toggle
                    self.settings_menu.filter.push(c);
                    self.settings_menu.selected_idx = 0;
                    self.settings_menu.scroll_offset = 0;
                }
                KeyCode::Char('r') if key.modifiers.alt => {
                    // alt+r: reset selected setting to default
                    let selected_idx = self
                        .settings_menu
                        .selected_idx
                        .min(filtered.len().saturating_sub(1));
                    if let Some(setting) = filtered.get(selected_idx) {
                        // reset to default by loading default config and applying that value
                        let default_config = crate::config::Config::default();
                        let default_value = setting.current_value(&default_config);
                        let previous = self.config.clone();
                        let result = setting.apply(&mut self.config, &default_value);
                        match result {
                            Ok(()) => {
                                self.save_config(previous);
                                self.toast = Some((
                                    format!("{} reset to default", setting.name()),
                                    Instant::now(),
                                ));
                                self.force_full = true;
                            }
                            Err(msg) => {
                                self.toast = Some((format!("error: {msg}"), Instant::now()));
                            }
                        }
                    }
                }
                KeyCode::Backspace => {
                    self.settings_menu.filter.pop();
                    self.settings_menu.selected_idx = 0;
                    self.settings_menu.scroll_offset = 0;
                }
                _ => {}
            }
        }
    }

    fn preview_theme_selection(&mut self) {
        if let Some(name) = BUILTIN_THEMES.get(self.theme_menu.selected_idx)
            && let Some(theme) = Theme::builtin(name)
        {
            self.theme = theme;
        }
    }

    fn search_text_for_focused(&mut self) -> Option<String> {
        let id = self.session.focused_pane?;
        let pane = self.session.panes.get_mut(&id)?;
        let mut parser = pane.parser.lock().ok()?;
        Some(crate::pane::full_buffer_text(&mut parser))
    }

    fn scroll_to_current_match(&mut self, id: crate::layout::PaneId) {
        let Some(pane) = self.session.panes.get(&id) else {
            return;
        };
        let Some(search) = &pane.search else {
            return;
        };
        let Some(i) = search.current_match else {
            return;
        };
        let Some((pt, _)) = search.matches.get(i) else {
            return;
        };
        let scrollback_len = pane
            .parser
            .lock()
            .map(|mut p| crate::pane::search::scrollback_len(&mut p))
            .unwrap_or(0);
        let target = scrollback_len.saturating_sub(pt.line);
        if let Some(pane) = self.session.panes.get_mut(&id)
            && pane.scroll_offset != target
        {
            pane.scroll_offset = target;
            pane.dirty.store(true, Ordering::Release);
            self.ui_dirty = true;
        }
    }

    fn handle_search_key(&mut self, key: crate::input::Key) {
        self.ui_dirty = true;
        let Some(id) = self.session.focused_pane else {
            self.mode = InputMode::Normal;
            return;
        };

        // ensure a search state exists on the focused pane
        if self
            .session
            .panes
            .get(&id)
            .is_some_and(|p| p.search.is_none())
            && let Some(pane) = self.session.panes.get_mut(&id)
        {
            pane.search = Some(crate::pane::SearchState::new(String::new(), false));
        }

        match key.code {
            crate::input::KeyCode::Esc => {
                if let Some(pane) = self.session.panes.get_mut(&id) {
                    pane.search = None;
                }
                self.mode = InputMode::Normal;
            }
            crate::input::KeyCode::Backspace => {
                if let Some(text) = self.search_text_for_focused()
                    && let Some(pane) = self.session.panes.get_mut(&id)
                    && let Some(search) = &mut pane.search
                {
                    search.backspace(&text);
                }
                self.scroll_to_current_match(id);
            }
            crate::input::KeyCode::Enter => {
                if key.modifiers.shift {
                    if let Some(pane) = self.session.panes.get_mut(&id)
                        && let Some(search) = &mut pane.search
                    {
                        search.prev_match();
                    }
                } else {
                    if let Some(pane) = self.session.panes.get_mut(&id)
                        && let Some(search) = &mut pane.search
                    {
                        search.next_match();
                    }
                }
                self.scroll_to_current_match(id);
            }
            crate::input::KeyCode::Space => {
                if let Some(text) = self.search_text_for_focused() {
                    if let Some(pane) = self.session.panes.get_mut(&id)
                        && let Some(search) = &mut pane.search
                    {
                        search.push(' ', &text);
                    }
                    self.scroll_to_current_match(id);
                }
            }
            crate::input::KeyCode::Char(c)
                if !key.modifiers.ctrl
                    && !key.modifiers.alt
                    && (c.is_ascii_graphic() || c == ' ') =>
            {
                if let Some(text) = self.search_text_for_focused() {
                    if let Some(pane) = self.session.panes.get_mut(&id)
                        && let Some(search) = &mut pane.search
                    {
                        search.push(c, &text);
                    }
                    self.scroll_to_current_match(id);
                }
            }
            _ => {}
        }
    }

    fn handle_vim_cmd_key(&mut self, key: crate::input::Key) {
        use crate::input::KeyCode;
        match key.code {
            KeyCode::Esc => {
                self.vim_cmd_buf.clear();
                self.vim_cmd_selected = 0;
                self.mode = InputMode::Normal;
                self.ui_dirty = true;
            }
            KeyCode::Tab => {
                // complete the command name; keep any already-typed arguments
                let buf = self.vim_cmd_buf.trim_start_matches(':').to_string();
                let name_part: String = buf.chars().take_while(|c| !c.is_whitespace()).collect();
                let arg_part = buf[name_part.len()..].to_string();
                let matches = filtered_commands(&self.vim_cmd_buf);
                if !matches.is_empty() {
                    self.vim_cmd_buf = format!(":{}{}", matches[0], arg_part);
                    self.vim_cmd_selected = 0;
                }
                self.ui_dirty = true;
            }
            KeyCode::Up => {
                // navigate up in completion list
                let matches = filtered_commands(&self.vim_cmd_buf);
                if !matches.is_empty() {
                    self.vim_cmd_selected = self.vim_cmd_selected.saturating_sub(1);
                }
                self.ui_dirty = true;
            }
            KeyCode::Down => {
                // navigate down in completion list
                let matches = filtered_commands(&self.vim_cmd_buf);
                if !matches.is_empty() {
                    self.vim_cmd_selected = (self.vim_cmd_selected + 1).min(matches.len() - 1);
                }
                self.ui_dirty = true;
            }
            KeyCode::Enter => {
                let matches = filtered_commands(&self.vim_cmd_buf);
                let typed = self.vim_cmd_buf.trim().to_string();
                let chosen = if typed.is_empty() {
                    if !matches.is_empty() && self.vim_cmd_selected < matches.len() {
                        Some(format!(":{}", matches[self.vim_cmd_selected]))
                    } else {
                        None
                    }
                } else if typed.split_whitespace().count() > 1 {
                    // typed text with arguments (e.g. "workspace_goto 3 4")
                    // takes precedence over the fuzzy selection
                    Some(typed)
                } else if !matches.is_empty() && self.vim_cmd_selected < matches.len() {
                    Some(format!(":{}", matches[self.vim_cmd_selected]))
                } else {
                    Some(typed)
                };
                let cmd_raw = chosen.unwrap_or_default();

                self.vim_cmd_buf.clear();
                self.vim_cmd_selected = 0;
                self.mode = InputMode::Normal;
                self.ui_dirty = true;

                if !cmd_raw.is_empty() {
                    // command::from_str already resolves every known command and
                    // aliases ("tabclose", "q", "buffer 3", "tabn 2", etc.).
                    let cmd_name = cmd_raw.strip_prefix(':').unwrap_or(&cmd_raw).to_string();
                    match cmd_name.parse::<Command>() {
                        Ok(cmd) => self.execute_command(cmd),
                        Err(err) => {
                            self.toast = Some((format!("error: {err}"), Instant::now()));
                        }
                    }
                }
            }
            KeyCode::Char(c) => {
                self.vim_cmd_buf.push(c);
                self.vim_cmd_selected = 0; // Reset selection when typing
                self.ui_dirty = true;
            }
            KeyCode::Space => {
                // space is its own key code, not Char(' '); without this the
                // argument separator was dropped and every command with
                // arguments failed with "unknown command".
                self.vim_cmd_buf.push(' ');
                self.vim_cmd_selected = 0;
                self.ui_dirty = true;
            }
            KeyCode::Backspace => {
                if self.vim_cmd_buf.is_empty() {
                    self.vim_cmd_selected = 0;
                    self.mode = InputMode::Normal;
                } else {
                    self.vim_cmd_buf.pop();
                    self.vim_cmd_selected = 0; // Reset selection
                }
                self.ui_dirty = true;
            }
            _ => {}
        }
    }

    fn handle_copy_key(&mut self, key: crate::input::Key) {
        self.ui_dirty = true;
        let Some(id) = self.session.focused_pane else {
            self.mode = InputMode::Normal;
            return;
        };

        match key.code {
            crate::input::KeyCode::Esc | crate::input::KeyCode::Char('q') => {
                if let Some(pane) = self.session.panes.get_mut(&id) {
                    pane.selection = None;
                }
                self.mode = InputMode::Normal;
            }
            crate::input::KeyCode::Char('h') | crate::input::KeyCode::Left => {
                if let Some(pane) = self.session.panes.get_mut(&id)
                    && let Some(sel) = &mut pane.selection
                {
                    sel.end.col = sel.end.col.saturating_sub(1);
                }
            }
            crate::input::KeyCode::Char('l') | crate::input::KeyCode::Right => {
                if let Some(pane) = self.session.panes.get_mut(&id)
                    && let Some(sel) = &mut pane.selection
                {
                    sel.end.col = sel.end.col.saturating_add(1);
                }
            }
            crate::input::KeyCode::Char('k') | crate::input::KeyCode::Up => {
                if let Some(pane) = self.session.panes.get_mut(&id) {
                    if let Some(sel) = &mut pane.selection {
                        sel.end.line = sel.end.line.saturating_sub(1);
                    }
                    pane.scroll_offset = pane.scroll_offset.saturating_add(1);
                    pane.dirty.store(true, Ordering::Release);
                }
            }
            crate::input::KeyCode::Char('j') | crate::input::KeyCode::Down => {
                if let Some(pane) = self.session.panes.get_mut(&id) {
                    if let Some(sel) = &mut pane.selection {
                        sel.end.line = sel.end.line.saturating_add(1);
                    }
                    pane.scroll_offset = pane.scroll_offset.saturating_sub(1);
                    pane.dirty.store(true, Ordering::Release);
                }
            }
            crate::input::KeyCode::Char('v') => {
                use crate::pane::selection::{GridPoint, Selection, SelectionMode};
                if let Some(pane) = self.session.panes.get_mut(&id)
                    && pane.selection.is_none()
                {
                    let pt = GridPoint { line: 0, col: 0 };
                    pane.selection = Some(Selection::new(pt, pt, SelectionMode::Character));
                }
            }
            crate::input::KeyCode::Char('y') => {
                if self.yank_focused_selection() {
                    self.toast = Some(("yanked to clipboard".into(), Instant::now()));
                }
                if let Some(pane) = self.session.panes.get_mut(&id) {
                    pane.selection = None;
                }
                self.mode = InputMode::Normal;
            }
            _ => {}
        }
    }

    fn handle_mouse(&mut self, msg: MouseEventMsg) {
        if !self.config.input.mouse {
            return;
        }
        // welcome overlay: left-click on a menu item activates it; anywhere else dismisses
        if self.show_welcome
            && msg.button == MouseButtonKind::Left
            && msg.action == crate::proto::MouseActionKind::Down
        {
            let (cols, rows) = self.effective_size();
            let width = 48u16.min(cols.saturating_sub(4));
            let height = 16u16.min(rows.saturating_sub(2));
            let start_x = cols.saturating_sub(width) / 2;
            let start_y = rows.saturating_sub(height) / 2;
            let local_row = msg.row.saturating_sub(start_y + 5) as usize; // first menu row is 6 lines into the box
            if msg.col >= start_x && msg.col < start_x + width && local_row < 5 {
                let _ = self.execute_command_row(local_row);
                return;
            }
            // click outside welcome dismisses it
            self.show_welcome = false;
            self.ui_dirty = true;
            return;
        }
        // help overlay eats mouse first
        if self.show_help {
            match msg.button {
                MouseButtonKind::ScrollUp => {
                    self.help_scroll = self.help_scroll.saturating_sub(1);
                    self.ui_dirty = true;
                }
                MouseButtonKind::ScrollDown => {
                    let max_idx = crate::render::help::HELP_ITEMS.saturating_sub(1);
                    if self.help_scroll < max_idx {
                        self.help_scroll = self.help_scroll.saturating_add(1);
                    }
                    self.ui_dirty = true;
                }
                MouseButtonKind::Left if msg.action == crate::proto::MouseActionKind::Down => {
                    self.show_help = false;
                    self.help_scroll = 0;
                    self.ui_dirty = true;
                }
                _ => {}
            }
            return;
        }
        // tab context menu: hover selects, click runs, wheel navigates,
        // anywhere else dismisses
        if self.mode == InputMode::ContextMenu {
            use crate::proto::MouseActionKind;
            use crate::render::context_menu::{
                action_for_row, context_menu_geometry, context_menu_index_at, step_row,
            };
            let (cols, rows) = self.effective_size();
            let (modal_rect, inner) = context_menu_geometry(
                Rect::new(0, 0, cols, rows),
                self.context_menu.anchor_col,
                self.context_menu.anchor_row,
            );
            let inside = msg.col >= modal_rect.x
                && msg.col < modal_rect.x + modal_rect.width
                && msg.row >= modal_rect.y
                && msg.row < modal_rect.y + modal_rect.height;
            match msg.button {
                MouseButtonKind::Left if msg.action == MouseActionKind::Down => {
                    if !inside {
                        self.mode = InputMode::Normal;
                        self.ui_dirty = true;
                        self.force_full = true;
                        return;
                    }
                    if let Some(row) = context_menu_index_at(inner, msg.row) {
                        self.context_menu.selected_idx = row;
                        if let Some(action) = action_for_row(row) {
                            let target = self.context_menu.target_ws;
                            self.mode = InputMode::Normal;
                            self.run_context_menu_action(action, target);
                        }
                    }
                }
                MouseButtonKind::Left
                    if msg.action == MouseActionKind::Moved
                        || msg.action == MouseActionKind::Drag =>
                {
                    if let Some(row) = context_menu_index_at(inner, msg.row)
                        && self.context_menu.selected_idx != row
                    {
                        self.context_menu.selected_idx = row;
                        self.ui_dirty = true;
                    }
                }
                MouseButtonKind::ScrollUp => {
                    self.context_menu.selected_idx = step_row(self.context_menu.selected_idx, -1);
                    self.ui_dirty = true;
                }
                MouseButtonKind::ScrollDown => {
                    self.context_menu.selected_idx = step_row(self.context_menu.selected_idx, 1);
                    self.ui_dirty = true;
                }
                _ => {}
            }
            return;
        }
        // palette menu mouse handling
        if self.mode == InputMode::Palette {
            if msg.button == MouseButtonKind::Left
                && msg.action == crate::proto::MouseActionKind::Down
            {
                let (cols, rows) = self.effective_size();
                let width = crate::render::palette::PALETTE_WIDTH.min(cols.saturating_sub(4));
                let height = crate::render::palette::PALETTE_HEIGHT.min(rows.saturating_sub(4));
                let start_x = cols.saturating_sub(width) / 2;
                let start_y = 2.min(rows.saturating_sub(height));
                let inside = msg.col >= start_x
                    && msg.col < start_x + width
                    && msg.row >= start_y
                    && msg.row < start_y + height;
                if !inside {
                    self.mode = InputMode::Normal;
                    self.ui_dirty = true;
                    return;
                }
                let click_row = msg.row - start_y;
                if click_row >= 3 && click_row < height - 2 {
                    let matches = filtered_commands(&self.palette.query);
                    let max_display = height.saturating_sub(4) as usize;
                    let selected = self
                        .palette
                        .selected_idx
                        .min(matches.len().saturating_sub(1));
                    let offset = selected
                        .saturating_sub(max_display.saturating_sub(1))
                        .min(matches.len().saturating_sub(max_display));
                    let item_idx = offset + (click_row - 3) as usize;
                    if item_idx < matches.len() {
                        let query = self.palette.query.trim().to_string();
                        self.palette.selected_idx = item_idx;
                        self.mode = InputMode::Normal;
                        self.force_full = true;
                        let typed = Command::from_str(&query).ok();
                        let selected = matches
                            .get(item_idx)
                            .and_then(|s| Command::from_str(s).ok());
                        if let Some(cmd) = typed.or(selected) {
                            self.execute_command(cmd);
                        }
                    }
                }
            } else if msg.button == MouseButtonKind::ScrollUp {
                self.palette.selected_idx = self.palette.selected_idx.saturating_sub(1);
                self.ui_dirty = true;
            } else if msg.button == MouseButtonKind::ScrollDown {
                let len = filtered_commands(&self.palette.query).len();
                if len > 0 {
                    self.palette.selected_idx =
                        (self.palette.selected_idx + 1).min(len.saturating_sub(1));
                }
                self.ui_dirty = true;
            }
            return;
        }
        // theme menu mouse handling
        if self.mode == InputMode::ThemeMenu {
            let (cols, rows) = self.effective_size();
            let (modal_rect, inner) =
                crate::render::theme_menu::theme_menu_geometry(Rect::new(0, 0, cols, rows));
            if msg.button == MouseButtonKind::Left
                && msg.action == crate::proto::MouseActionKind::Down
            {
                if !(msg.col >= modal_rect.x
                    && msg.col < modal_rect.x + modal_rect.width
                    && msg.row >= modal_rect.y
                    && msg.row < modal_rect.y + modal_rect.height)
                {
                    if let Some(previous) = self.theme_before_menu.take() {
                        self.theme = previous;
                    }
                    self.mode = InputMode::Normal;
                    self.ui_dirty = true;
                    return;
                }
                let rel_col = msg.col - inner.x;
                let rel_row = msg.row - inner.y;
                if let Some(idx) =
                    crate::render::theme_menu::theme_menu_index_at(inner, rel_col, rel_row)
                    && let Some(name) = BUILTIN_THEMES.get(idx)
                    && let Some(theme) = Theme::builtin(name)
                {
                    self.theme = theme;
                    let previous = self.config.clone();
                    self.config.theme.name = (*name).to_string();
                    self.save_config(previous);
                    self.toast = Some((format!("theme: {name}"), Instant::now()));
                    self.theme_before_menu = None;
                    self.mode = InputMode::Normal;
                    self.ui_dirty = true;
                }
            } else if msg.button == MouseButtonKind::ScrollUp {
                self.theme_menu.selected_idx = self.theme_menu.selected_idx.saturating_sub(1);
                self.preview_theme_selection();
                self.ui_dirty = true;
            } else if msg.button == MouseButtonKind::ScrollDown {
                self.theme_menu.selected_idx =
                    (self.theme_menu.selected_idx + 1).min(BUILTIN_THEMES.len().saturating_sub(1));
                self.preview_theme_selection();
                self.ui_dirty = true;
            }
            return;
        }
        // settings menu mouse handling
        if self.mode == InputMode::SettingsMenu {
            let (cols, rows) = self.effective_size();
            let (modal_rect, inner, max_display) =
                crate::render::settings_menu::settings_menu_geometry(Rect::new(0, 0, cols, rows));
            let tabs = crate::render::settings_menu::settings_tab_layout(inner);
            if msg.button == MouseButtonKind::Left
                && msg.action == crate::proto::MouseActionKind::Down
            {
                if !(msg.col >= modal_rect.x
                    && msg.col < modal_rect.x + modal_rect.width
                    && msg.row >= modal_rect.y
                    && msg.row < modal_rect.y + modal_rect.height)
                {
                    self.mode = InputMode::Normal;
                    self.ui_dirty = true;
                    return;
                }

                let click_col = msg.col - modal_rect.x;

                // tab row is the first inner row (after the border)
                if msg.row == inner.y {
                    for (tab, tab_x, tab_w) in tabs {
                        if msg.col >= tab_x && msg.col < tab_x + tab_w {
                            self.settings_menu.active_tab = tab;
                            self.settings_menu.selected_idx = 0;
                            self.settings_menu.scroll_offset = 0;
                            self.ui_dirty = true;
                            return;
                        }
                    }
                    return;
                }

                // click on a list row to select that setting
                let list_start = inner.y + 4;
                if msg.row >= list_start
                    && msg.row < inner.y + inner.height
                    && click_col >= 2
                    && click_col < modal_rect.width.saturating_sub(2)
                {
                    let filtered = self.settings_menu.visible_settings();
                    let item_row = (msg.row - list_start) as usize;
                    let offset = self.settings_menu.scroll_offset;
                    let item_idx = offset + item_row;

                    if item_row < max_display && item_idx < filtered.len() {
                        self.settings_menu.selected_idx = item_idx;
                        self.ui_dirty = true;
                    }
                }
            } else if msg.button == MouseButtonKind::ScrollUp {
                if self.settings_menu.selected_idx > 0 {
                    self.settings_menu.selected_idx -= 1;
                    if self.settings_menu.selected_idx < self.settings_menu.scroll_offset {
                        self.settings_menu.scroll_offset = self.settings_menu.selected_idx;
                    }
                }
                self.ui_dirty = true;
            } else if msg.button == MouseButtonKind::ScrollDown {
                let max_idx = self
                    .settings_menu
                    .visible_settings()
                    .len()
                    .saturating_sub(1);
                if self.settings_menu.selected_idx < max_idx {
                    self.settings_menu.selected_idx += 1;
                    if self.settings_menu.selected_idx
                        >= self.settings_menu.scroll_offset + max_display
                    {
                        self.settings_menu.scroll_offset = self
                            .settings_menu
                            .selected_idx
                            .saturating_sub(max_display - 1);
                    }
                }
                self.ui_dirty = true;
            }
            return;
        }

        let (cols, rows) = self.effective_size();
        let (ws_rect, sidebar_rect, _, content) = self.chrome_areas(Rect::new(0, 0, cols, rows));

        // forward to the app when the app enabled mouse reporting
        if self.mode == InputMode::Normal && self.forward_mouse_to_app(&msg, content) {
            return;
        }

        // booping the cat takes precedence over pane and app input
        if msg.button == MouseButtonKind::Left
            && msg.action == crate::proto::MouseActionKind::Down
            && self.cat_started.is_some()
        {
            let rects = self
                .session
                .layout(content, self.gap_inner(), self.gap_outer());
            let border = self.border_enabled(rects.len());
            let hit = rects
                .iter()
                .find(|(id, _)| Some(*id) == self.session.focused_pane)
                .and_then(|(_, rect)| crate::render::cat::cat_rect(inner_rect(*rect, border)))
                .is_some_and(|cat| {
                    msg.col >= cat.x
                        && msg.col < cat.x + cat.width
                        && msg.row >= cat.y
                        && msg.row < cat.y + cat.height
                });
            if hit {
                self.cat_boops += 1;
                self.cat_boop_at = Some(Instant::now());
                let reply = match self.cat_boops {
                    1 => "mrrp.",
                    2 => "mrrp!",
                    3 => "purr~",
                    4 => "*kneads you*",
                    _ => "MROW",
                };
                self.toast = Some((reply.to_string(), Instant::now()));
                self.ui_dirty = true;
                return;
            }
        }

        match msg.button {
            MouseButtonKind::ScrollUp => {
                if ws_rect.is_some_and(|r| msg.row == r.y)
                    || sidebar_rect.is_some_and(|r| msg.col >= r.x && msg.col < r.x + r.width)
                {
                    // scroll workspace tabs left / sidebar up
                    self.tab_scroll_offset = self.tab_scroll_offset.saturating_sub(1);
                    self.ui_dirty = true;
                } else {
                    self.scroll_focused(self.config.input.scroll_lines as isize);
                }
            }
            MouseButtonKind::ScrollDown => {
                if ws_rect.is_some_and(|r| msg.row == r.y)
                    || sidebar_rect.is_some_and(|r| msg.col >= r.x && msg.col < r.x + r.width)
                {
                    // scroll workspace tabs right / sidebar down
                    let max_offset = self.session.workspace_ids().len().saturating_sub(1);
                    self.tab_scroll_offset = (self.tab_scroll_offset + 1).min(max_offset);
                    self.ui_dirty = true;
                } else {
                    self.scroll_focused(-(self.config.input.scroll_lines as isize));
                }
            }
            MouseButtonKind::Middle if msg.action == crate::proto::MouseActionKind::Down => {
                if let Some(content) = crate::util::clipboard::paste_external() {
                    self.write_paste_to_focused(content.as_bytes());
                }
            }
            MouseButtonKind::Left if msg.action == crate::proto::MouseActionKind::Down => {
                if self.mode == InputMode::Copy {
                    let rects = self
                        .session
                        .layout(content, self.gap_inner(), self.gap_outer());
                    let border = self.border_enabled(rects.len());
                    if let Some((id, rect)) = rects
                        .iter()
                        .rev()
                        .find(|(_, r)| contains(r, msg.col, msg.row))
                        .copied()
                    {
                        let inner_view = inner_rect(rect, border);
                        let Some(pane) = self.session.panes.get_mut(&id) else {
                            return;
                        };
                        let col = msg.col.saturating_sub(inner_view.x) as usize;
                        let row = msg.row.saturating_sub(inner_view.y) as usize;
                        use crate::pane::selection::{GridPoint, Selection, SelectionMode};
                        let point = GridPoint { line: row, col };
                        if pane.selection.is_none() {
                            pane.selection =
                                Some(Selection::new(point, point, SelectionMode::Character));
                        } else if let Some(ref mut sel) = pane.selection {
                            sel.start = point;
                            sel.end = point;
                        }
                        self.ui_dirty = true;
                    }
                    return;
                }

                // click on workspace bar tabs
                if let Some(bar) = ws_rect
                    && msg.row == bar.y
                {
                    let workspaces: Vec<(usize, String, bool)> = self
                        .session
                        .workspace_ids()
                        .into_iter()
                        .map(|id| (id, self.session.workspace_name(id), self.workspace_busy(id)))
                        .collect();
                    let layout = crate::render::chrome::layout_workspace_bar(
                        bar,
                        &workspaces,
                        self.tab_scroll_offset,
                    );

                    // click on '<' scroll left (pinned at the bar's left edge)
                    if let Some(ax) = layout.left_arrow_x
                        && msg.col < ax + 2
                    {
                        self.tab_scroll_offset = self.tab_scroll_offset.saturating_sub(1);
                        self.ui_dirty = true;
                        return;
                    }

                    // click on '>' scroll right (pinned at the tab area's right edge)
                    if let Some(rx) = layout.right_arrow_x
                        && msg.col >= rx
                        && msg.col < rx + 2
                    {
                        let max_offset = workspaces.len().saturating_sub(1);
                        self.tab_scroll_offset = (self.tab_scroll_offset + 1).min(max_offset);
                        self.ui_dirty = true;
                        return;
                    }

                    for tab in &layout.tabs {
                        // click on tab label switches workspace
                        if msg.col >= tab.pill_x && msg.col < tab.pill_x + tab.pill_w {
                            self.goto_workspace(tab.ws);
                            return;
                        }

                        // click on close button closes workspace
                        if msg.col >= tab.close_x && msg.col < tab.close_x + tab.close_w {
                            self.close_workspace(tab.ws);
                            return;
                        }
                    }

                    // click on '+' button creates new workspace
                    if let Some(ax) = layout.add_x
                        && msg.col >= ax
                        && msg.col < ax + 2
                    {
                        if self.session.workspace_ids().len()
                            >= self.config.general.max_workspaces.max(1)
                        {
                            self.toast = Some((
                                format!(
                                    "max workspaces ({}) reached; close one first",
                                    self.config.general.max_workspaces.max(1)
                                ),
                                Instant::now(),
                            ));
                            self.ui_dirty = true;
                            return;
                        }
                        let ws = self.session.new_workspace();
                        self.spawn_pane(None, false);
                        self.toast = Some((format!("workspace {ws} created"), Instant::now()));
                        self.ui_dirty = true;
                        return;
                    }
                }

                // click on the vertical sidebar: tab switch, close, new workspace, scroll arrows
                if let Some(bar) = sidebar_rect
                    && msg.col >= bar.x
                    && msg.col < bar.x + bar.width
                {
                    let workspaces: Vec<(usize, String, bool)> = self
                        .session
                        .workspace_ids()
                        .into_iter()
                        .map(|id| (id, self.session.workspace_name(id), self.workspace_busy(id)))
                        .collect();
                    let layout = crate::render::chrome::layout_workspace_sidebar(
                        bar,
                        &workspaces,
                        self.tab_scroll_offset,
                        self.config.ui.sidebar_density,
                        self.sidebar_collapsed,
                        self.config.ui.sidebar_header,
                    );

                    if let Some(ay) = layout.up_arrow_y
                        && msg.row == ay
                    {
                        self.tab_scroll_offset = self.tab_scroll_offset.saturating_sub(1);
                        self.ui_dirty = true;
                        return;
                    }
                    if let Some(ay) = layout.down_arrow_y
                        && msg.row == ay
                    {
                        let max_offset = workspaces.len().saturating_sub(1);
                        self.tab_scroll_offset = (self.tab_scroll_offset + 1).min(max_offset);
                        self.ui_dirty = true;
                        return;
                    }

                    for tab in &layout.slots {
                        if msg.row >= tab.y && msg.row < tab.y + tab.row_h {
                            if msg.col >= tab.close_x && !self.sidebar_collapsed {
                                self.close_workspace(tab.ws);
                                return;
                            }
                            self.goto_workspace(tab.ws);
                            return;
                        }
                    }

                    if let Some(ay) = layout.add_y
                        && msg.row == ay
                    {
                        if self.session.workspace_ids().len()
                            >= self.config.general.max_workspaces.max(1)
                        {
                            self.toast = Some((
                                format!(
                                    "max workspaces ({}) reached; close one first",
                                    self.config.general.max_workspaces.max(1)
                                ),
                                Instant::now(),
                            ));
                            self.ui_dirty = true;
                            return;
                        }
                        let ws = self.session.new_workspace();
                        self.spawn_pane(None, false);
                        self.toast = Some((format!("workspace {ws} created"), Instant::now()));
                        self.ui_dirty = true;
                        return;
                    }

                    // click on header does nothing (session name is not interactive)
                    return;
                }

                // click on content panes to focus, start split drag, or start float drag
                let rects = self
                    .session
                    .layout(content, self.gap_inner(), self.gap_outer());

                let alt_held = msg.alt;

                let on_any_title_bar = rects
                    .iter()
                    .any(|(_, r)| contains(r, msg.col, msg.row) && msg.row == r.y);

                // alt+drag swaps panes; only with alt held
                let clicked = rects
                    .iter()
                    .rev()
                    .find(|(_, r)| contains(r, msg.col, msg.row))
                    .copied();

                if let Some((id, _rect)) = clicked {
                    let ws = self.session.active_workspace;
                    let is_floating = self
                        .session
                        .floating_panes
                        .get(&ws)
                        .is_some_and(|list| list.iter().any(|f| f.id == id));

                    // alt+drag to swap panes (ONLY with alt held)
                    if alt_held && !is_floating {
                        self.session.focus_pane(id);
                        self.mouse_drag = MouseDragState::SwapTitleDrag {
                            pane_id: id,
                            start_col: msg.col,
                            start_row: msg.row,
                            target_id: None,
                        };
                        self.ui_dirty = true;
                        return;
                    }
                }

                // split-border resize drag; disabled on title bars and with alt held
                if !on_any_title_bar
                    && !alt_held
                    && let Some(tree) = self.session.active_tree()
                    && let Some((axis, path)) =
                        tree.split_at(content, self.gap_inner(), msg.col, msg.row)
                {
                    self.mouse_drag = MouseDragState::SplitDrag {
                        path,
                        axis,
                        start_col: msg.col,
                        start_row: msg.row,
                    };
                    self.ui_dirty = true;
                    return;
                }

                // pane click: floating corners/move drag, or plain focus
                if let Some((id, rect)) = clicked {
                    self.session.focus_pane(id);
                    let ws = self.session.active_workspace;
                    let is_floating = self
                        .session
                        .floating_panes
                        .get(&ws)
                        .is_some_and(|list| list.iter().any(|f| f.id == id));
                    if is_floating {
                        // corners resize; the top row (outside the corner blocks) drags.
                        let on_top = msg.row == rect.y;
                        let on_bottom = msg.row == rect.y + rect.height.saturating_sub(1);
                        let on_left = msg.col == rect.x;
                        let on_right = msg.col == rect.x + rect.width.saturating_sub(1);
                        let in_corner = |on_top: bool, on_left: bool| {
                            let xs = if on_left {
                                msg.col.saturating_sub(rect.x)
                            } else {
                                (rect.x + rect.width - 1).saturating_sub(msg.col)
                            };
                            let ys = if on_top {
                                msg.row.saturating_sub(rect.y)
                            } else {
                                (rect.y + rect.height - 1).saturating_sub(msg.row)
                            };
                            xs <= 1 && ys <= 1
                        };
                        if on_top && on_left && in_corner(true, true) {
                            self.mouse_drag = MouseDragState::ResizeFloatingCorner {
                                pane_id: id,
                                corner: FloatingCorner::TopLeft,
                                origin_rect: rect,
                            };
                        } else if on_top && on_right && in_corner(true, false) {
                            self.mouse_drag = MouseDragState::ResizeFloatingCorner {
                                pane_id: id,
                                corner: FloatingCorner::TopRight,
                                origin_rect: rect,
                            };
                        } else if on_bottom && on_left && in_corner(false, true) {
                            self.mouse_drag = MouseDragState::ResizeFloatingCorner {
                                pane_id: id,
                                corner: FloatingCorner::BottomLeft,
                                origin_rect: rect,
                            };
                        } else if on_bottom && on_right && in_corner(false, false) {
                            self.mouse_drag = MouseDragState::ResizeFloatingCorner {
                                pane_id: id,
                                corner: FloatingCorner::BottomRight,
                                origin_rect: rect,
                            };
                        } else if on_top && !on_left && !on_right {
                            self.mouse_drag = MouseDragState::MoveFloating {
                                pane_id: id,
                                offset_x: msg.col as i32 - rect.x as i32,
                                offset_y: msg.row as i32 - rect.y as i32,
                            };
                        }
                    }
                    self.ui_dirty = true;
                    return;
                }

                // click outside every pane clears focus
                if self.session.focused_pane.take().is_some() {
                    self.ui_dirty = true;
                }
            }
            MouseButtonKind::Left if msg.action == crate::proto::MouseActionKind::Drag => {
                if self.mode == InputMode::Copy {
                    let rects = self
                        .session
                        .layout(content, self.gap_inner(), self.gap_outer());
                    let border = self.border_enabled(rects.len());
                    if let Some((id, rect)) = rects
                        .iter()
                        .rev()
                        .find(|(_, r)| contains(r, msg.col, msg.row))
                        .copied()
                    {
                        let inner_view = inner_rect(rect, border);
                        if let Some(pane) = self.session.panes.get_mut(&id) {
                            let col = msg.col.saturating_sub(inner_view.x) as usize;
                            let row = msg.row.saturating_sub(inner_view.y) as usize;
                            use crate::pane::selection::GridPoint;
                            let point = GridPoint { line: row, col };
                            if let Some(sel) = pane.selection.as_mut() {
                                sel.end = point;
                            } else {
                                use crate::pane::selection::Selection;
                                pane.selection = Some(Selection::new(
                                    point,
                                    point,
                                    crate::pane::selection::SelectionMode::Character,
                                ));
                            }
                            self.ui_dirty = true;
                        }
                    }
                    return;
                }
                match self.mouse_drag {
                    MouseDragState::MoveFloating {
                        pane_id,
                        offset_x,
                        offset_y,
                    } => {
                        let ws = self.session.active_workspace;
                        if let Some(list) = self.session.floating_panes.get_mut(&ws)
                            && let Some(f) = list.iter_mut().find(|f| f.id == pane_id)
                        {
                            let min_x = content.x as i32;
                            let max_x =
                                (content.x + content.width).saturating_sub(f.rect.width) as i32;
                            let min_y = content.y as i32;
                            let max_y =
                                (content.y + content.height).saturating_sub(f.rect.height) as i32;
                            let new_x = (msg.col as i32 - offset_x).clamp(min_x, max_x.max(min_x));
                            let new_y = (msg.row as i32 - offset_y).clamp(min_y, max_y.max(min_y));
                            f.rect.x = new_x.max(0) as u16;
                            f.rect.y = new_y.max(0) as u16;
                            self.ui_dirty = true;
                        }
                    }
                    MouseDragState::ResizeFloatingCorner {
                        pane_id,
                        corner,
                        origin_rect,
                    } => {
                        let ws = self.session.active_workspace;
                        if let Some(list) = self.session.floating_panes.get_mut(&ws)
                            && let Some(f) = list.iter_mut().find(|f| f.id == pane_id)
                        {
                            f.rect = resize_floating_rect(
                                content,
                                origin_rect,
                                corner,
                                msg.col,
                                msg.row,
                            );
                            self.ui_dirty = true;
                        }
                    }
                    MouseDragState::SplitDrag { ref path, axis, .. } => {
                        let gap_inner = self.config.layout.gap_inner;
                        if let Some(tree) = self.session.active_tree_mut() {
                            tree.drag_split(content, gap_inner, path, axis, msg.col, msg.row);
                            self.ui_dirty = true;
                        }
                    }
                    MouseDragState::SwapTitleDrag {
                        pane_id,
                        start_col,
                        start_row,
                        target_id,
                    } => {
                        let mut new_target = target_id;

                        let rects =
                            self.session
                                .layout(content, self.gap_inner(), self.gap_outer());
                        if let Some((hovered_id, _)) = rects
                            .iter()
                            .rev()
                            .find(|(_, r)| contains(r, msg.col, msg.row))
                            .copied()
                        {
                            if hovered_id != pane_id {
                                new_target = Some(hovered_id);
                                self.ui_dirty = true;
                            }
                        } else {
                            new_target = None;
                            self.ui_dirty = true;
                        }

                        self.mouse_drag = MouseDragState::SwapTitleDrag {
                            pane_id,
                            start_col,
                            start_row,
                            target_id: new_target,
                        };
                    }
                    MouseDragState::None => {}
                }
            }
            MouseButtonKind::Left if msg.action == crate::proto::MouseActionKind::Up => {
                if let MouseDragState::SwapTitleDrag {
                    pane_id, target_id, ..
                } = self.mouse_drag
                {
                    if let Some(target) = target_id {
                        let rects_before =
                            self.session
                                .layout(content, self.gap_inner(), self.gap_outer());
                        let rect_from_id = rects_before
                            .iter()
                            .find(|(id, _)| *id == pane_id)
                            .map(|(_, r)| *r);
                        let rect_from_target = rects_before
                            .iter()
                            .find(|(id, _)| *id == target)
                            .map(|(_, r)| *r);

                        // perform swap
                        self.session.swap_panes(pane_id, target);

                        let rects_after =
                            self.session
                                .layout(content, self.gap_inner(), self.gap_outer());
                        let rect_to_id = rects_after
                            .iter()
                            .find(|(id, _)| *id == pane_id)
                            .map(|(_, r)| *r);
                        let rect_to_target = rects_after
                            .iter()
                            .find(|(id, _)| *id == target)
                            .map(|(_, r)| *r);

                        // animate the swaps
                        let duration_ms = self.config.animation.layout_duration_ms;
                        let easing = self.config.animation.layout_easing;
                        let enabled = self.config.animation.enabled;

                        if let (Some(from), Some(to)) = (rect_from_id, rect_to_id) {
                            self.pane_animations.animate_pane(
                                pane_id,
                                from,
                                to,
                                duration_ms,
                                easing,
                                enabled,
                            );
                        }
                        if let (Some(from), Some(to)) = (rect_from_target, rect_to_target) {
                            self.pane_animations.animate_pane(
                                target,
                                from,
                                to,
                                duration_ms,
                                easing,
                                enabled,
                            );
                        }

                        self.note_layout_change();
                    }
                    self.mouse_drag = MouseDragState::None;
                } else if matches!(self.mouse_drag, MouseDragState::SplitDrag { .. }) {
                    // animation on split resize needs the pre-resize rects, which are
                    // already gone by this point; skip animation and just note the change.
                    self.mouse_drag = MouseDragState::None;
                    self.note_layout_change();
                } else if self.mouse_drag != MouseDragState::None {
                    self.mouse_drag = MouseDragState::None;
                    self.ui_dirty = true;
                }
                if self.config.input.copy_on_select {
                    let _ = self.yank_focused_selection();
                }
            }
            MouseButtonKind::Right if msg.action == crate::proto::MouseActionKind::Down => {
                // right-click on a workspace tab opens the tab context menu.
                // anywhere else is a no-op: it used to instantly close the
                // pane under the cursor, which was indistinguishable from the
                // tab hit-test failing and killed idle panes on a mis-aimed
                // right-click.
                if let Some(target) = self.tab_at_position(ws_rect, sidebar_rect, &msg) {
                    self.context_menu.target_ws = target;
                    self.context_menu.selected_idx = 0;
                    self.context_menu.anchor_col = msg.col;
                    self.context_menu.anchor_row = msg.row;
                    self.mode = InputMode::ContextMenu;
                    self.ui_dirty = true;
                    self.force_full = true;
                }
            }
            _ => {}
        }
    }

    fn scroll_focused(&mut self, delta: isize) {
        let Some(id) = self.session.focused_pane else {
            return;
        };
        let Some(pane) = self.session.panes.get_mut(&id) else {
            return;
        };
        let max = pane
            .parser
            .lock()
            .map(|mut p| crate::pane::search::scrollback_len(&mut p))
            .unwrap_or(0);
        let next = (pane.scroll_offset as isize + delta).clamp(0, max as isize) as usize;
        if next != pane.scroll_offset {
            pane.scroll_offset = next;
            pane.dirty.store(true, Ordering::Release);
            self.ui_dirty = true;
        }
    }

    /// copies the focused pane's selection to all clients via `clipboard`, so
    /// each client applies its own policy (osc 52 and/or external tool).
    /// returns true when a non-empty selection was sent.
    fn yank_focused_selection(&mut self) -> bool {
        let Some(id) = self.session.focused_pane else {
            return false;
        };
        let Some(pane) = self.session.panes.get_mut(&id) else {
            return false;
        };
        let Some(sel) = pane.selection.clone() else {
            return false;
        };
        let scroll_offset = pane.scroll_offset;
        let text = match pane.parser.lock() {
            Ok(mut parser) => crate::pane::selection_text(&mut parser, &sel, scroll_offset),
            Err(_) => return false,
        };
        if text.is_empty() {
            return false;
        }
        let payload = text.into_bytes();
        let mut sent = false;
        for client in self.clients.values_mut() {
            if write_msg(&mut client.stream, &ServerMsg::Clipboard(payload.clone())).is_ok() {
                sent = true;
            }
        }
        sent
    }

    fn write_to_focused(&mut self, bytes: &[u8]) {
        if let Some(id) = self.session.focused_pane
            && let Some(pane) = self.session.panes.get(&id)
        {
            pane.write_input(bytes);
        }
    }

    /// pastes bytes into the focused pane, wrapping them in bracketed-paste
    /// markers only when the pane's application enabled mode 2004 (the
    /// standard multiplexer behavior). otherwise the bytes go out verbatim,
    /// so terminals applications without bracketed paste don't see stray
    /// `\x1b[200~` sequences as garbage input.
    fn write_paste_to_focused(&mut self, bytes: &[u8]) {
        let wrap = self
            .session
            .focused_pane
            .and_then(|id| self.session.panes.get(&id))
            .map(|pane| {
                pane.parser
                    .lock()
                    .map(|p| p.screen().bracketed_paste())
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        if wrap {
            let mut wrapped = Vec::with_capacity(bytes.len() + 12);
            wrapped.extend_from_slice(b"\x1b[200~");
            wrapped.extend_from_slice(bytes);
            wrapped.extend_from_slice(b"\x1b[201~");
            self.write_to_focused(&wrapped);
        } else {
            self.write_to_focused(bytes);
        }
    }

    fn gap_inner(&self) -> u16 {
        self.config.layout.gap_inner
    }

    fn gap_outer(&self) -> u16 {
        self.config.layout.gap_outer
    }

    fn execute_command_row(&mut self, row: usize) -> bool {
        let cmd = match row {
            0 => Command::NewPane,
            1 => Command::SplitDown,
            2 => {
                self.show_welcome = false;
                self.show_help = true;
                return true;
            }
            3 => {
                self.open_config_in_editor();
                return true;
            }
            4 => Command::ThemeMenu,
            _ => return false,
        };
        self.show_welcome = false;
        self.execute_command(cmd);
        true
    }

    fn execute_command(&mut self, cmd: Command) {
        // block layout commands when modals are open
        let modal_active = !matches!(
            self.mode,
            InputMode::Normal | InputMode::VimCmd | InputMode::Search
        );
        if modal_active {
            // allow only global commands in modals
            if !matches!(cmd, Command::KillSession | Command::Help) {
                return;
            }
        }

        // structural commands change pane geometry or swap visible content;
        // graphics pixels cannot be repainted by cell diffs alone.
        match cmd {
            Command::NewPane
            | Command::SplitRight
            | Command::SplitDown
            | Command::ClosePane
            | Command::SwapLeft
            | Command::SwapRight
            | Command::SwapUp
            | Command::SwapDown
            | Command::ResizeLeft
            | Command::ResizeRight
            | Command::ResizeUp
            | Command::ResizeDown
            | Command::Equalize
            | Command::RotateSplit
            | Command::ToggleZoom
            | Command::ToggleSidebar
            | Command::ToggleFloat
            | Command::FloatCenter
            | Command::ToggleScratch
            | Command::WorkspaceGoto(_)
            | Command::WorkspaceNext
            | Command::WorkspacePrev
            | Command::NewWorkspace
            | Command::CloseWorkspace(_)
            | Command::MoveToWorkspace(_)
            | Command::MoveToWorkspaceNext
            | Command::MoveToWorkspacePrev => {
                self.note_layout_change();
            }
            _ => {}
        }
        match cmd {
            Command::NewPane => self.spawn_pane(None, true),
            Command::SplitRight => self.spawn_pane(Some(SplitAxis::Horizontal), true),
            Command::SplitDown => self.spawn_pane(Some(SplitAxis::Vertical), true),
            Command::ClosePane => {
                self.close_focused_with_confirm();
            }
            Command::FocusLeft => self.focus(Direction::Left),
            Command::FocusRight => self.focus(Direction::Right),
            Command::FocusUp => self.focus(Direction::Up),
            Command::FocusDown => self.focus(Direction::Down),
            Command::FocusNext => self.focus_cycle(true),
            Command::FocusPrev => self.focus_cycle(false),
            Command::SwapLeft => self.swap(Direction::Left),
            Command::SwapRight => self.swap(Direction::Right),
            Command::SwapUp => self.swap(Direction::Up),
            Command::SwapDown => self.swap(Direction::Down),
            Command::ResizeLeft => self.resize(Direction::Left),
            Command::ResizeRight => self.resize(Direction::Right),
            Command::ResizeUp => self.resize(Direction::Up),
            Command::ResizeDown => self.resize(Direction::Down),
            Command::Equalize => {
                let before_rects = self.capture_layout_rects();
                self.session.equalize();
                self.animate_layout_delta(before_rects);
                self.ui_dirty = true;
            }
            Command::RotateSplit => {
                let before_rects = self.capture_layout_rects();
                self.session.rotate_focused();
                self.animate_layout_delta(before_rects);
                self.ui_dirty = true;
            }
            Command::ToggleZoom => {
                let before_rects = self.capture_layout_rects();
                self.session.toggle_zoom();
                self.animate_layout_delta(before_rects);
                self.ui_dirty = true;
            }
            Command::ToggleSidebar => {
                self.sidebar_collapsed = !self.sidebar_collapsed;
                self.ui_dirty = true;
                self.force_full = true;
            }
            Command::WorkspaceGoto(nums) => self.goto_workspaces(&nums),
            Command::WorkspaceNext => {
                if let Some(next) = self.step_workspace(1) {
                    self.goto_workspace(next);
                }
            }
            Command::WorkspacePrev => {
                if let Some(prev) = self.step_workspace(-1) {
                    self.goto_workspace(prev);
                }
            }
            Command::NewWorkspace => {
                if self.session.workspace_ids().len() >= self.config.general.max_workspaces.max(1) {
                    self.toast = Some((
                        format!(
                            "max workspaces ({}) reached; close one first",
                            self.config.general.max_workspaces.max(1)
                        ),
                        Instant::now(),
                    ));
                    self.ui_dirty = true;
                    return;
                }
                let ws = self.session.new_workspace();
                self.spawn_pane(None, false);
                self.toast = Some((format!("workspace {ws} created"), Instant::now()));
                self.ui_dirty = true;
            }
            Command::CloseWorkspace(nums) => {
                if nums.is_empty() {
                    self.close_workspace(self.session.active_workspace);
                } else {
                    self.close_workspaces(&nums);
                }
            }
            Command::MoveToWorkspace(nums) => {
                for &n in &nums {
                    if !self.session.move_focused_to_workspace(n) {
                        self.toast =
                            Some((format!("failed to move to workspace {n}"), Instant::now()));
                        break;
                    }
                    self.goto_workspace(n);
                }
                if !nums.is_empty() {
                    let n = nums[nums.len() - 1];
                    self.toast = Some((format!("moved to workspace {n}"), Instant::now()));
                }
            }
            Command::MoveToWorkspaceNext => {
                if self.session.workspace_ids().len() > 1
                    && let Some(next) = self.step_workspace(1)
                    && self.session.move_focused_to_workspace(next)
                {
                    self.toast = Some((format!("moved to workspace {next}"), Instant::now()));
                    // auto-switch to the target workspace so you follow the pane
                    self.goto_workspace(next);
                }
            }
            Command::MoveToWorkspacePrev => {
                if self.session.workspace_ids().len() > 1
                    && let Some(prev) = self.step_workspace(-1)
                    && self.session.move_focused_to_workspace(prev)
                {
                    self.toast = Some((format!("moved to workspace {prev}"), Instant::now()));
                    // auto-switch to the target workspace so you follow the pane
                    self.goto_workspace(prev);
                }
            }
            Command::RenameWorkspace(name) => {
                self.session
                    .workspace_names
                    .insert(self.session.active_workspace, name);
                self.ui_dirty = true;
            }
            Command::CommandPalette => {
                self.mode = InputMode::Palette;
                self.palette.query.clear();
                self.palette.selected_idx = 0;
                self.ui_dirty = true;
                self.force_full = true; // Force full redraw to prevent color bleeding when opening modal
            }
            Command::SettingsMenu => {
                if self.mode == InputMode::SettingsMenu {
                    self.mode = InputMode::Normal;
                } else {
                    self.mode = InputMode::SettingsMenu;
                    self.settings_menu.selected_idx = 0;
                    self.settings_menu.editing = None;
                    self.settings_menu.scroll_offset = 0;
                    self.settings_menu.filter.clear();
                }
                self.ui_dirty = true;
                self.force_full = true;
            }
            Command::ThemeMenu => {
                self.mode = InputMode::ThemeMenu;
                self.theme_before_menu = Some(self.theme.clone());
                self.theme_menu.selected_idx = BUILTIN_THEMES
                    .iter()
                    .position(|name| *name == self.theme.name)
                    .unwrap_or(0);
                self.ui_dirty = true;
                self.force_full = true; // Force full redraw to prevent color bleeding when opening modal
            }
            Command::Help => {
                self.show_help = !self.show_help;
                self.ui_dirty = true;
                self.force_full = true;
            }
            Command::ShowWelcome => {
                self.show_welcome = true;
                self.ui_dirty = true;
                self.force_full = true;
            }
            Command::ReloadConfig => self.reload_config(),
            Command::Detach => self.detach_all(crate::proto::ExitReason::Detached),
            Command::KillSession => {
                self.running.store(false, Ordering::Relaxed);
                self.detach_all(crate::proto::ExitReason::Killed);
            }
            Command::ToggleFloat => {
                let (cols, rows) = self.effective_size();

                let (_, _, _, content) = self.chrome_areas(Rect::new(0, 0, cols, rows));

                // capture rect before toggle
                let focused_id = self.session.focused_pane;
                let rects_before = self
                    .session
                    .layout(content, self.gap_inner(), self.gap_outer());
                let rect_before = focused_id.and_then(|id| {
                    rects_before
                        .iter()
                        .find(|(pid, _)| *pid == id)
                        .map(|(_, r)| *r)
                });

                let toggled = self.session.toggle_float(content);
                if toggled {
                    // capture rect after toggle
                    let rects_after =
                        self.session
                            .layout(content, self.gap_inner(), self.gap_outer());
                    let rect_after = focused_id.and_then(|id| {
                        rects_after
                            .iter()
                            .find(|(pid, _)| *pid == id)
                            .map(|(_, r)| *r)
                    });

                    // animate the float/tile transition
                    if let (Some(id), Some(from), Some(to)) = (focused_id, rect_before, rect_after)
                    {
                        let duration_ms = self.config.animation.layout_duration_ms;
                        let easing = self.config.animation.layout_easing;
                        let enabled = self.config.animation.enabled;

                        self.pane_animations.animate_pane(
                            id,
                            from,
                            to,
                            duration_ms,
                            easing,
                            enabled,
                        );
                    }

                    self.toast = Some(("float toggled".into(), Instant::now()));
                    self.note_layout_change();
                    // don't force full redraw during animation to avoid stutter; let animation frames be incremental
                    self.ui_dirty = true;
                }
            }
            Command::FloatCenter => {
                let (cols, rows) = self.effective_size();
                let (_, _, _, content) = self.chrome_areas(Rect::new(0, 0, cols, rows));
                if self.session.float_center(content) {
                    self.toast = Some(("float centered".into(), Instant::now()));
                    self.ui_dirty = true;
                } else {
                    self.toast = Some(("float pane not focused".into(), Instant::now()));
                    self.ui_dirty = true;
                }
            }
            Command::ToggleScratch => {
                let (cols, rows) = self.effective_size();
                let (_, _, _, content) = self.chrome_areas(Rect::new(0, 0, cols, rows));
                let shell = self.config.general.shell.clone();
                let scrollback = self.config.general.scrollback;
                match self.session.toggle_scratch(content, scrollback, &shell) {
                    Ok(crate::server::session::ScratchAction::Created) => {
                        self.toast = Some(("scratchpad created".into(), Instant::now()));
                    }
                    Ok(crate::server::session::ScratchAction::Shown) => {
                        self.toast = Some(("scratchpad shown".into(), Instant::now()));
                    }
                    Ok(crate::server::session::ScratchAction::Hidden) => {
                        self.toast = Some(("scratchpad hidden".into(), Instant::now()));
                    }
                    Err(e) => {
                        self.toast = Some((format!("scratchpad error: {e}"), Instant::now()));
                    }
                }
                self.ui_dirty = true;
            }
            Command::Cat => {
                if self.cat_started.take().is_some() {
                    self.cat_boops = 0;
                    self.cat_boop_at = None;
                    self.toast = Some(("the cat wandered off".into(), Instant::now()));
                } else {
                    self.cat_started = Some(Instant::now());
                    self.cat_tick = Instant::now();
                    self.cat_boops = 0;
                    self.cat_boop_at = None;
                    self.toast = Some(("mrrp".into(), Instant::now()));
                }
                self.ui_dirty = true;
            }
            Command::CopyMode => {
                self.mode = InputMode::Copy;
                self.ui_dirty = true;
            }
            Command::Search => {
                self.mode = InputMode::Search;
                self.ui_dirty = true;
            }
            Command::PasteBuffer => {
                if let Some(content) = crate::util::clipboard::paste_external() {
                    self.write_paste_to_focused(content.as_bytes());
                }
            }
            Command::ResizeMode => {
                self.mode = InputMode::Resize;
                self.ui_dirty = true;
            }
        }
    }

    /// closes a workspace and switches to the most recent previous one if it
    /// was the active workspace. shared by the horizontal bar and the sidebar
    /// click handlers.
    fn close_workspace(&mut self, ws: usize) {
        self.close_workspaces(&[ws]);
    }

    /// closes the listed workspaces (deduplicated) in descending order.
    /// refuses to close the last remaining workspace. emits at most one toast.
    fn close_workspaces(&mut self, nums: &[usize]) {
        let ids = self.session.workspace_ids();
        if ids.len() == 1 && nums.contains(&ids[0]) {
            self.toast = Some(("cannot close the only workspace".into(), Instant::now()));
            self.ui_dirty = true;
            return;
        }

        let mut targets: Vec<usize> = nums.iter().copied().filter(|&n| ids.contains(&n)).collect();
        if targets.is_empty() {
            self.toast = Some(("no such workspace".to_string(), Instant::now()));
            self.ui_dirty = true;
            return;
        }
        targets.sort_unstable();
        targets.dedup();
        targets.reverse();

        let pre = self.session.workspace_ids().len();
        for ws in targets {
            let cur_ids = self.session.workspace_ids();
            if cur_ids.len() <= 1 {
                break;
            }
            let prev_ws = self.session.close_workspace(ws);
            if let Some(prev) = prev_ws {
                self.session.ensure_workspace(prev);
            }
            self.clamp_tab_scroll();
            if self
                .session
                .workspace_pane_ids(self.session.active_workspace)
                .is_empty()
            {
                self.spawn_pane(None, false);
            }
            self.mouse_drag = MouseDragState::None;
            self.note_layout_change();
        }

        let closed = pre - self.session.workspace_ids().len();
        if closed == 1 {
            let n = nums[0];
            self.toast = Some((format!("workspace {n} closed"), Instant::now()));
        } else if closed == 0 {
            self.toast = Some(("no such workspace".to_string(), Instant::now()));
        } else {
            self.toast = Some((format!("closed {closed} workspaces"), Instant::now()));
        }
        self.ui_dirty = true;
    }

    fn spawn_pane(&mut self, axis: Option<SplitAxis>, notify: bool) {
        if self.session.panes.len() >= self.config.general.max_panes.max(1) {
            self.toast = Some((
                format!(
                    "max panes ({}) reached; close one first",
                    self.config.general.max_panes.max(1)
                ),
                Instant::now(),
            ));
            self.ui_dirty = true;
            return;
        }
        let before_rects = self.capture_layout_rects();
        let (cols, rows) = self.effective_size();
        let (_, _, _, content) = self.chrome_areas(Rect::new(0, 0, cols, rows));
        let workspace_is_empty = self
            .session
            .workspace_pane_ids(self.session.active_workspace)
            .is_empty();
        let (pane_cols, pane_rows) = if workspace_is_empty {
            // a root pane fills its resolved workspace. Starting it at half
            // width and resizing after the shell has run startup programs
            // makes first-workspace output depend on a timing race.
            let gap = self.gap_outer();
            let root = Rect::new(
                content.x.saturating_add(gap.min(content.width / 2)),
                content.y.saturating_add(gap.min(content.height / 2)),
                content
                    .width
                    .saturating_sub(gap.min(content.width / 2).saturating_mul(2)),
                content
                    .height
                    .saturating_sub(gap.min(content.height / 2).saturating_mul(2)),
            );
            let root = inner_rect(root, self.border_enabled(1));
            (root.width.max(1), root.height.max(1))
        } else {
            (
                (content.width / 2).max(2),
                content.height.saturating_sub(2).max(1),
            )
        };
        let axis = axis.or_else(|| {
            Some(match self.config.layout.split_axis {
                SplitAxisSetting::Auto => choose_aspect_split(content),
                SplitAxisSetting::Right => SplitAxis::Horizontal,
                SplitAxisSetting::Down => SplitAxis::Vertical,
            })
        });
        match self.session.new_pane(
            pane_cols,
            pane_rows,
            self.config.general.scrollback,
            &self.config.general.shell,
            None,
            axis,
        ) {
            Ok(_) => {
                if notify {
                    self.toast = Some(("new pane created".to_string(), Instant::now()));
                }
            }
            Err(e) => {
                self.toast = Some((format!("failed to create pane: {e}"), Instant::now()));
            }
        }
        self.animate_layout_delta(before_rects);
        self.note_layout_change();
    }

    /// clamps `tab_scroll_offset` after workspaces are added/removed so it never
    /// points past the last workspace, and pulls it back to keep the active
    /// workspace tab visible. prevents the "dangling < with no tabs" bug.
    fn clamp_tab_scroll(&mut self) {
        let ids = self.session.workspace_ids();
        let max_offset = ids.len().saturating_sub(1);
        self.tab_scroll_offset = self.tab_scroll_offset.min(max_offset);
        if let Some(pos) = ids.iter().position(|&w| w == self.session.active_workspace)
            && self.tab_scroll_offset > pos
        {
            self.tab_scroll_offset = pos;
        }
    }

    /// switches to each requested workspace in order, showing a toast for the last.
    fn goto_workspaces(&mut self, nums: &[usize]) {
        for &n in nums {
            self.goto_workspace(n);
        }
    }

    fn goto_workspace(&mut self, ws: usize) {
        let current = self.session.active_workspace;
        if current == ws {
            return; // already there
        }

        self.session.ensure_workspace(ws);
        if self.session.workspace_pane_ids(ws).is_empty() {
            self.spawn_pane(None, false);
        }
        // scroll tab bar to make the target workspace visible if needed
        let ids = self.session.workspace_ids();
        if let Some(pos) = ids.iter().position(|&w| w == ws)
            && pos < self.tab_scroll_offset
        {
            self.tab_scroll_offset = pos;
        }
        self.note_layout_change();
    }

    fn focus(&mut self, dir: Direction) {
        let (cols, rows) = self.effective_size();
        let (_, _, _, content) = self.chrome_areas(Rect::new(0, 0, cols, rows));
        let rects = self
            .session
            .layout(content, self.gap_inner(), self.gap_outer());
        if self.session.focus_neighbor(dir, &rects) {
            self.ui_dirty = true;
        }
    }

    fn swap(&mut self, dir: Direction) {
        let before_rects = self.capture_layout_rects();
        let (cols, rows) = self.effective_size();
        let (_, _, _, content) = self.chrome_areas(Rect::new(0, 0, cols, rows));
        let rects = self
            .session
            .layout(content, self.gap_inner(), self.gap_outer());
        if self.session.swap_neighbor(dir, &rects) {
            self.animate_layout_delta(before_rects);
            self.ui_dirty = true;
        }
    }

    fn resize(&mut self, dir: Direction) {
        let before_rects = self.capture_layout_rects();
        let step = (self.config.layout.resize_step.max(1) as f32) / 100.0;
        if self.session.resize_focused(dir, step) {
            self.animate_layout_delta(before_rects);
            self.note_layout_change();
        }
    }

    fn focus_cycle(&mut self, forward: bool) {
        let ids = self
            .session
            .workspace_pane_ids(self.session.active_workspace);
        if ids.is_empty() {
            return;
        }
        let current = self.session.focused_pane;
        let pos = current
            .and_then(|c| ids.iter().position(|id| *id == c))
            .unwrap_or(0);
        let next = if forward {
            (pos + 1) % ids.len()
        } else {
            (pos + ids.len() - 1) % ids.len()
        };
        self.session.focus_pane(ids[next]);
        self.ui_dirty = true;
    }

    /// sends `$EDITOR <config>` to the focused pane, or nudges the user to set $EDITOR.
    fn open_config_in_editor(&mut self) {
        if let Some(editor) = std::env::var("EDITOR").ok().filter(|s| !s.is_empty()) {
            let config_path = crate::util::paths::config_path();
            let cmd = format!(
                "{} {}\n",
                shell_quote_word(&editor),
                shell_quote_word(&config_path.to_string_lossy())
            );
            self.write_to_focused(cmd.as_bytes());
        } else {
            self.toast = Some(("set $EDITOR to open config".to_string(), Instant::now()));
        }
    }

    fn save_config(&mut self, previous: crate::config::Config) {
        self.show_welcome = self.config.ui.show_welcome;
        self.sync_config_to_session();
        if let Err(e) = crate::config::save_to_file(&self.config, &self.config_path, &previous) {
            tracing::error!("failed to save config: {}", e);
        }
    }

    /// routes mouse events to the application when it enabled mouse reporting.
    fn forward_mouse_to_app(&self, msg: &MouseEventMsg, content: Rect) -> bool {
        if self.mode != InputMode::Normal {
            return false;
        }
        let Some(id) = self.session.focused_pane else {
            return false;
        };
        let Some(pane) = self.session.panes.get(&id) else {
            return false;
        };
        if !pane
            .mouse_reporting
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return false;
        }
        let rects = self
            .session
            .layout(content, self.gap_inner(), self.gap_outer());
        let Some((_, rect)) = rects.iter().find(|(pid, _)| *pid == id) else {
            return false;
        };
        let border = self.border_enabled(rects.len());
        let inner = inner_rect(*rect, border);
        if msg.col < inner.x
            || msg.row < inner.y
            || msg.col >= inner.x + inner.width
            || msg.row >= inner.y + inner.height
        {
            return false;
        }
        let rel = MouseEventMsg {
            col: msg.col - inner.x,
            row: msg.row - inner.y,
            button: msg.button,
            action: msg.action,
            shift: msg.shift,
            alt: msg.alt,
            ctrl: msg.ctrl,
        };
        pane.write_input(&crate::input::mouse::encode_sgr_mouse(&rel));
        true
    }

    fn mouse_drag_references(&self, id: PaneId) -> bool {
        match self.mouse_drag {
            MouseDragState::MoveFloating { pane_id, .. }
            | MouseDragState::ResizeFloatingCorner { pane_id, .. }
            | MouseDragState::SwapTitleDrag { pane_id, .. } => pane_id == id,
            MouseDragState::SplitDrag { .. } => false,
            MouseDragState::None => false,
        }
    }

    fn close_focused_with_confirm(&mut self) {
        let before_rects = self.capture_layout_rects();
        let Some(id) = self.session.focused_pane else {
            self.ui_dirty = true;
            return;
        };
        if self.config.general.confirm_close_running && self.pending_close != Some(id) {
            let busy = self
                .session
                .panes
                .get(&id)
                .and_then(|p| p.child_pid)
                .is_some_and(crate::util::process::has_busy_children);
            if busy {
                self.pending_close = Some(id);
                self.toast = Some((
                    "child process running - press again to confirm".into(),
                    Instant::now(),
                ));
                self.ui_dirty = true;
                return;
            }
        }
        self.pending_close = None;
        if self.mouse_drag_references(id) {
            self.mouse_drag = MouseDragState::None;
        }
        self.drop_pane_graphics_hard(id);
        self.session.close_pane(id);
        self.animate_layout_delta(before_rects);
        self.note_layout_change();
    }

    /// re-applies the parts of the config that live in the session state, so a
    /// config reload or a settings-menu edit takes effect immediately.
    fn sync_config_to_session(&mut self) {
        self.session.monocle =
            self.config.layout.mode == crate::config::schema::LayoutMode::Monocle;
        self.bindings.set_disabled(&self.config.input.disabled_keys);
    }

    fn reload_config(&mut self) {
        let path = crate::util::paths::config_path();
        match crate::config::load_from_file_or_default(&path) {
            Ok(config) => {
                self.bindings = KeyBindings::new(config.profile.mode, &config.input.passthrough);
                self.bindings.set_disabled(&config.input.disabled_keys);
                if let Err(e) = self.bindings.apply_custom_binds(&config.binds) {
                    tracing::warn!("invalid custom keybindings ignored: {e}");
                }
                self.theme = Theme::resolve(&config.theme);
                self.session
                    .set_workspace_names(&config.general.workspace_names);
                self.config = config;
                self.sync_config_to_session();
                self.toast = Some(("configuration reloaded".to_string(), Instant::now()));
            }
            Err(errors) => {
                let msg = errors
                    .first()
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "unknown error".to_string());
                self.toast = Some((format!("config error: {msg}"), Instant::now()));
            }
        }
        self.ui_dirty = true;
    }

    fn detach_all(&mut self, reason: crate::proto::ExitReason) {
        for client in self.clients.values_mut() {
            let _ = write_msg(&mut client.stream, &ServerMsg::Exit(reason.clone()));
        }
        self.clients.clear();
        self.ui_dirty = true;
    }

    fn effective_size(&self) -> (u16, u16) {
        if self.clients.is_empty() {
            (80, 24)
        } else {
            let min_c = self.clients.values().map(|c| c.cols).min().unwrap_or(80);
            let min_r = self.clients.values().map(|c| c.rows).min().unwrap_or(24);
            (min_c.max(1), min_r.max(1))
        }
    }

    fn chrome_areas(&self, area: Rect) -> (Option<Rect>, Option<Rect>, Option<Rect>, Rect) {
        Self::chrome_areas_for(&self.config, area, self.sidebar_collapsed)
    }

    fn border_enabled(&self, pane_count: usize) -> bool {
        if self.config.decorations.border_style == BorderStyle::None {
            return false;
        }
        !(self.config.decorations.hide_border_when_single && pane_count <= 1)
    }

    fn render_all(&mut self) {
        if self.clients.is_empty() {
            self.ui_dirty = false;
            return;
        }

        self.update_toast();

        // re-anchor stored graphics after scroll output or scrollback navigation.
        // anchor moves are handled by cheap delete/place re-emission instead of
        // a full redraw, so pixels are never re-sent on ordinary output.
        if self.config.general.graphics && self.shift_graphics_for_scroll() {
            self.reemit_gfx = true;
        }
        let gap_inner = self.gap_inner();
        let (eff_cols, eff_rows) = self.effective_size();
        let eff_area = Rect::new(0, 0, eff_cols, eff_rows);
        let (_, _, _, eff_content) = self.chrome_areas(eff_area);
        let gap_outer = self.gap_outer();
        let eff_rects = self.session.layout(eff_content, gap_inner, gap_outer);
        let border = self.border_enabled(eff_rects.len());

        // resize PTYs to fit their resolved rectangles when geometry changes.
        // any resize can reflow the grid underneath an image, so stored image
        // placements are invalidated and the child re-emits them after SIGWINCH.
        let mut any_resized = false;
        for (id, rect) in &eff_rects {
            // during animation, skip PTY resize to avoid constant resizing.
            // use animated rect for rendering instead, which is smoother.
            if self.pane_animations.has_active() {
                continue;
            }

            let inner = inner_rect(*rect, border);
            let size = (inner.width.max(1), inner.height.max(1));

            if self.last_sizes.get(id) == Some(&size) {
                continue;
            }
            let is_known = self.last_sizes.contains_key(id);
            if let Some(pane) = self.session.panes.get_mut(id) {
                pane.resize(size.0, size.1);
            }
            self.last_sizes.insert(*id, size);
            if is_known {
                // a resize reflows the grid under the images; the kitty
                // placements stay in the store and are re-placed (cropped to
                // the new viewport) by the next full repaint. dropping them
                // here deleted the images for good - the pane never re-emits
                // them, so a logo vanished on every resize.
                any_resized = true;
            }
        }
        if any_resized {
            // the full redraw clears host-side pixels. Keep each pane's
            // original graphics commands so they can be restored at their
            // clamped anchors after the new PTY size takes effect.
            // skip full redraw during layout animations to avoid event loop
            // that causes the animation to run at 800+ fps instead of 60 fps.
            if !self.pane_animations.has_active() {
                self.force_full = true;
                // a sixel has no delete sequence: the re-placed copy would be
                // drawn on top of the old one. the full redraw's ED2 wipes the
                // old pixels, so the re-placement must ride on it.
                self.sixel_restore_pending = true;
            }
        }
        self.last_sizes
            .retain(|id, _| self.session.panes.contains_key(id));

        // a full repaint emits ESC[2J, which clears host-side graphics too.
        // a sixel or iterm2 image cannot be erased or moved incrementally
        // (there is no delete sequence), so a frame that might show a
        // different placement must be a full redraw. kitty keeps its cheap
        // delete/place path, and ordinary repaints stay a diff.
        //
        // scrolling is different: a sixel drawn by a sixel-capable outer
        // scrolls with the grid natively (iterm2 and wezterm behave the same),
        // so re-placing it would be wasted bytes and an ED2 would wipe it.
        // kitty has no implicit anchoring, so kitty images always get the
        // scroll bookkeeping.
        let sixel_native_scroll =
            self.session.outer_graphics == crate::util::outer_term::OuterGraphics::Sixel;
        let iterm2_native_scroll =
            self.session.outer_graphics == crate::util::outer_term::OuterGraphics::Iterm2;
        let scroll = self.reemit_gfx;
        let sixel_any = eff_rects.iter().any(|(id, _)| {
            let Some(pane) = self.session.panes.get(id) else {
                return false;
            };
            let active_alt = pane
                .parser
                .lock()
                .map(|parser| parser.screen().alternate_screen())
                .unwrap_or(false);
            pane.graphics
                .lock()
                .map(|store| {
                    store.active_commands().iter().any(|cmd| {
                        matches!(
                            cmd.format,
                            crate::pane::GraphicFormat::Sixel | crate::pane::GraphicFormat::Iterm2
                        ) && cmd.alternate_screen == active_alt
                    })
                })
                .unwrap_or(false)
        });
        let sixel_visible = eff_rects.iter().any(|(id, rect)| {
            let Some(pane) = self.session.panes.get(id) else {
                return false;
            };
            let active_alt = pane
                .parser
                .lock()
                .map(|parser| parser.screen().alternate_screen())
                .unwrap_or(false);
            let Ok(store) = pane.graphics.lock() else {
                return false;
            };
            let use_border = border;
            let inner = inner_rect(*rect, use_border);
            store.active_commands().iter().any(|cmd| {
                matches!(
                    cmd.format,
                    crate::pane::GraphicFormat::Sixel | crate::pane::GraphicFormat::Iterm2
                ) && cmd.alternate_screen == active_alt
                    && {
                        let adjusted = cmd.row + pane.scroll_offset as i32;
                        adjusted >= 0 && adjusted < i32::from(inner.height) && cmd.col < inner.width
                    }
            })
        });
        let modal_open_now = matches!(
            self.mode,
            InputMode::Palette | InputMode::ThemeMenu | InputMode::SettingsMenu | InputMode::Help
        ) || self.show_help
            || self.show_welcome;
        let sixel_restore = std::mem::take(&mut self.sixel_restore_pending);
        let iterm2_any = eff_rects.iter().any(|(id, _)| {
            let Some(pane) = self.session.panes.get(id) else {
                return false;
            };
            let active_alt = pane
                .parser
                .lock()
                .map(|parser| parser.screen().alternate_screen())
                .unwrap_or(false);
            pane.graphics
                .lock()
                .map(|store| {
                    store.active_commands().iter().any(|cmd| {
                        cmd.format == crate::pane::GraphicFormat::Iterm2
                            && cmd.alternate_screen == active_alt
                    })
                })
                .unwrap_or(false)
        });
        // kitty images are re-placed with the cheap delete+place sequence on
        // scroll (reemit_gfx), never with a full payload + ESC[2J: the host
        // already keeps the image data, and ESC[2J would wipe it before the
        // re-transmit lands (the "image flashes then vanishes" symptom).
        let sixel_needs_ed2 = (scroll && sixel_any && !sixel_native_scroll)
            || (scroll && iterm2_any && !iterm2_native_scroll)
            || (sixel_visible
                && (modal_open_now || self.pane_animations.has_active() || sixel_restore));
        // drop tracking entries for graphics that no longer exist (the store
        // re-assigns seqs on every re-push, so stale keys would pile up forever)
        if sixel_any || !self.emitted_gfx.is_empty() {
            let live: HashSet<(PaneId, u64)> = self
                .session
                .panes
                .iter()
                .flat_map(|(id, pane)| {
                    pane.graphics
                        .lock()
                        .map(|store| {
                            store
                                .active_commands()
                                .iter()
                                .map(|cmd| (*id, cmd.seq))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
                .collect();
            self.emitted_gfx.retain(|key| live.contains(key));
            self.hidden_gfx.retain(|key| live.contains(key));
            for client in self.clients.values_mut() {
                client.emitted_gfx.retain(|key| live.contains(key));
                client.hidden_gfx.retain(|key| live.contains(key));
            }
        }
        let force_full = std::mem::take(&mut self.force_full) || sixel_needs_ed2;
        let reemit_gfx = std::mem::take(&mut self.reemit_gfx) || force_full;

        // clear dirty flags before drawing so output arriving during the frame
        // schedules another render.
        for pane in self.session.panes.values() {
            pane.dirty.store(false, Ordering::Release);
        }
        self.ui_dirty = false;
        self.clock_text = crate::render::clock_text(&self.config.ui.clock_format);

        let session = &self.session;
        let config = &self.config;
        let theme = &self.theme;
        let workspace_bar_pos = config.ui.workspace_bar;
        let mut failed_clients: Vec<usize> = Vec::new();
        let show_clock = config.ui.clock;
        let ascii = config.ui.ascii;
        // the help cheat-sheet must look like the screen it teaches about,
        // not a dimmed version of it
        let dim_inactive = config.decorations.dim_inactive && !self.show_help;
        let dim_strength = config.decorations.dim_strength;
        let hint_text = self.hint_text();
        let mode = self.mode;
        let show_welcome = self.show_welcome;
        let show_help = self.show_help;
        let palette_open = self.mode == InputMode::Palette;
        let theme_menu_open = self.mode == InputMode::ThemeMenu;
        let settings_menu_open = self.mode == InputMode::SettingsMenu;
        let context_menu_open = self.mode == InputMode::ContextMenu;
        let cat_started = self.cat_started;
        let cat_boop_at = self.cat_boop_at;

        let ws_ids = session.workspace_ids();
        let workspaces: Vec<(WorkspaceId, String, bool)> = ws_ids
            .iter()
            .map(|&id| {
                (
                    WorkspaceId(id),
                    session.workspace_name(id),
                    self.workspace_busy(id),
                )
            })
            .collect();
        // sidebar tabs carry the focused pane's title as a per-tab hint
        let sidebar_tabs: Vec<(WorkspaceId, String, String, bool)> = ws_ids
            .iter()
            .copied()
            .map(|id| {
                let hint = session
                    .focused_pane_in(id)
                    .and_then(|pid| session.panes.get(&pid))
                    .and_then(|p| p.child_pid)
                    .and_then(crate::util::process::foreground_process_name)
                    .unwrap_or_default();
                (
                    WorkspaceId(id),
                    session.workspace_name(id),
                    hint,
                    self.workspace_busy(id),
                )
            })
            .collect();
        let active_ws = WorkspaceId(session.active_workspace);
        let focused_id = session.focused_pane;
        let panes = &session.panes;

        // one-shot deletes for images dropped from the stores (queued by
        // drop_all_graphics on reflow); every client gets the same list.
        let pending_deletes = std::mem::take(&mut self.pending_gfx_deletes);

        for client in self.clients.values_mut() {
            let client_needs_full = client.needs_full;
            client.needs_full = false;

            // force a full redraw for new clients
            let client_force_full = force_full || client_needs_full;
            // a fresh client has never received any graphics: send every live
            // placement, otherwise an image that was transmitted before the
            // client attached would be missing for that client forever.
            let fresh_client = client_needs_full && client.emitted_gfx.is_empty();

            let area = Rect::new(0, 0, client.cols.max(1), client.rows.max(1));
            let (ws_rect, sidebar_rect, hint_rect, content) =
                Self::chrome_areas_for(config, area, self.sidebar_collapsed);
            let mut rects = session.layout(content, gap_inner, gap_outer);

            // apply pane layout animations (title bar drag-to-swap, etc)
            // and re-clamp floating panes to stay within bounds during animation
            for (id, rect) in &mut rects {
                if let Some(anim_rect) = self.pane_animations.get_animated_rect(*id) {
                    *rect = anim_rect;
                    // re-clamp floating panes during animation to prevent them from drifting outside content area
                    if session.is_pane_floating(*id) {
                        crate::layout::clamp_rect(rect, content);
                    }
                }
            }

            let pane_count = rects.len();

            // terminal.clear() resets ratatui's back buffer, so the next draw
            // produces every cell, not just the diff, keeping the client's ED
            // clear consistent with a full frame.
            if force_full {
                let _ = client.terminal.clear();
            }
            // hide images that belong to panes painting nothing this frame
            // (inactive workspaces or a big centered modal covering the panes).
            // placement-level deletes only; the image stays on the host and
            // is re-placed when the pane becomes visible again. one-shot
            // hide commands per transition. the vim-command bar, search and
            // copy/resize modes are small bottom chrome, not a cover: opening
            // them must not blank the pane's images.
            let modal_open = matches!(
                self.mode,
                InputMode::Palette
                    | InputMode::ThemeMenu
                    | InputMode::SettingsMenu
                    | InputMode::Help
            ) || self.show_help
                || self.show_welcome;
            let mut gfx_items: Vec<crate::proto::GfxCmd> = pending_deletes.to_vec();
            if config.general.graphics {
                let pane_rects: HashMap<PaneId, Rect> = rects.iter().copied().collect();
                for (id, pane) in panes.iter() {
                    let active_alt = pane
                        .parser
                        .lock()
                        .map(|parser| parser.screen().alternate_screen())
                        .unwrap_or(false);
                    let use_border = border && pane_count > 0;
                    let inner = pane_rects.get(id).map(|rect| inner_rect(*rect, use_border));
                    let active_screen_visible =
                        inner.is_some_and(|inner| inner.width > 0 && inner.height > 0);
                    let Ok(store) = pane.graphics.lock() else {
                        continue;
                    };
                    for cmd in store.active_commands() {
                        if cmd.format != crate::pane::GraphicFormat::Kitty {
                            continue;
                        }
                        let Some(metadata) = cmd.kitty_metadata.as_ref() else {
                            continue;
                        };
                        let hidden_or_unwanted = !active_screen_visible
                            || inner.is_none_or(|inner| {
                                !cmd.visible_in_pane(inner.width, inner.height, pane.scroll_offset)
                            });
                        if !modal_open
                            && active_screen_visible
                            && cmd.alternate_screen == active_alt
                            && !hidden_or_unwanted
                        {
                            continue;
                        }
                        let key = (*id, cmd.seq);
                        if !client.emitted_gfx.contains(&key) {
                            continue;
                        }
                        if !client.hidden_gfx.insert(key) {
                            continue;
                        }
                        // delete by (image id, placement id); the image data
                        // stays on the host so re-placement costs nothing. if
                        // no placement id exists, delete the whole image
                        // instead of leaving an undeletable ghost.
                        let ctl = crate::pane::graphics::KittyControlData {
                            a: Some("d".to_string()),
                            d: Some("i".to_string()),
                            i: metadata.i,
                            p: metadata.p,
                            q: Some(2),
                            ..Default::default()
                        };
                        let data = crate::pane::graphics::kitty_apc(&ctl);
                        self.gfx_delete_seq += 1;
                        gfx_items.push(crate::proto::GfxCmd {
                            row: 0,
                            col: 0,
                            format: crate::proto::GfxFormat::Kitty,
                            seq: self.gfx_delete_seq,
                            data,
                        });
                    }
                }
            }
            if config.general.graphics {
                for (id, rect) in &rects {
                    let Some(pane) = panes.get(id) else {
                        continue;
                    };
                    let active_alt = pane
                        .parser
                        .lock()
                        .map(|parser| parser.screen().alternate_screen())
                        .unwrap_or(false);
                    let use_border = border && pane_count > 0;
                    let inner = inner_rect(*rect, use_border);
                    if inner.width == 0 || inner.height == 0 {
                        continue;
                    }
                    let Ok(store) = pane.graphics.lock() else {
                        continue;
                    };
                    // while a modal overlay covers the panes (settings, menus,
                    // help), do not re-emit placements: they stay hidden.
                    // closing the modal re-emits them from hidden_gfx.
                    if modal_open {
                        for cmd in store.active_commands() {
                            if matches!(
                                cmd.format,
                                crate::pane::GraphicFormat::Sixel
                                    | crate::pane::GraphicFormat::Iterm2
                            ) {
                                let key = (*id, cmd.seq);
                                if client.emitted_gfx.contains(&key) {
                                    self.sixel_restore_pending = true;
                                }
                            }
                        }
                        continue;
                    }
                    // force graphics re-emission during animations to update placement coordinates
                    let is_animating = self.pane_animations.is_animating(*id);
                    // seqs of the placements that were used this frame; they
                    // are moved to the back of the store's eviction queue
                    // once the lock is released.
                    let mut used_placements: Vec<u64> = Vec::new();
                    for cmd in store.active_commands() {
                        if cmd.alternate_screen != active_alt {
                            if matches!(
                                cmd.format,
                                crate::pane::GraphicFormat::Sixel
                                    | crate::pane::GraphicFormat::Iterm2
                            ) {
                                let key = (*id, cmd.seq);
                                if client.emitted_gfx.contains(&key) {
                                    self.sixel_restore_pending = true;
                                }
                            }
                            continue;
                        }
                        let virtual_placeholder =
                            cmd.kitty_metadata.as_ref().is_some_and(|m| m.U == Some(1));
                        let key = (*id, cmd.seq);
                        let was_emitted = client.emitted_gfx.contains(&key) && !fresh_client;

                        // the placement is in use: keep it at the back of the
                        // eviction queue (zellij stamps images with an LRU
                        // stamp and evicts the oldest unused one first).
                        // the store lock is already held here, so the touch
                        // is collected and applied after the loop.
                        used_placements.push(cmd.seq);
                        if virtual_placeholder && was_emitted && !force_full && !is_animating {
                            // a virtual (U=1, Unicode placeholder) placement
                            // reached the host already; placeholder cells anchor
                            // it during ordinary repaints. after an ED2 the
                            // image and placeholder association are rebuilt.
                            client.hidden_gfx.remove(&key);
                            continue;
                        }

                        // the anchor row is relative to the live grid (the
                        // viewport at scroll_offset = 0). Scrolling up shows
                        // older history, which pushes the content DOWN the
                        // screen, so an anchored image moves down with it:
                        // row + scroll_offset.
                        let adjusted_row = cmd.row + pane.scroll_offset as i32;

                        // partially scrolled-out direct kitty placements are
                        // re-placed cropped to the visible fragment instead of
                        // being hidden or pinned half off-screen; anything fully
                        // out of view falls to the placement-contained sweep.
                        let crop =
                            cmd.crop_placement(inner.width, inner.height, pane.scroll_offset);

                        // graphics that are entirely outside the pane are
                        // skipped; a placement that only overflows the pane
                        // edge is cropped to the pane instead.
                        let out_of_view = adjusted_row < 0
                            || adjusted_row >= i32::from(inner.height)
                            || cmd.col >= inner.width;
                        if out_of_view && !force_full {
                            // a placement clipped by the top/bottom edge keeps
                            // emitting (the crop path below re-places the
                            // visible fragment); anything fully out of view
                            // stays hidden.
                            let clipped_but_visible = cmd.format
                                == crate::pane::GraphicFormat::Kitty
                                && cmd.visible_in_pane(
                                    inner.width,
                                    inner.height,
                                    pane.scroll_offset,
                                )
                                && crop.is_some();
                            if !clipped_but_visible {
                                continue;
                            }
                        }

                        // the anchor of the placement on screen: a cropped
                        // fragment starts at its own cell inside the pane.
                        let (row, col) = crop.as_ref().map_or_else(
                            || {
                                (
                                    inner.y.saturating_add(adjusted_row.max(0) as u16),
                                    inner
                                        .x
                                        .saturating_add(cmd.col.min(inner.width.saturating_sub(1))),
                                )
                            },
                            |(_, vis_col, vis_row)| {
                                (
                                    inner.y.saturating_add(*vis_row),
                                    inner.x.saturating_add(*vis_col),
                                )
                            },
                        );

                        let format = match cmd.format {
                            crate::pane::GraphicFormat::Sixel => crate::proto::GfxFormat::Sixel,
                            crate::pane::GraphicFormat::Kitty => crate::proto::GfxFormat::Kitty,
                            crate::pane::GraphicFormat::Iterm2 => crate::proto::GfxFormat::Iterm2,
                        };
                        // first emission sends the pixel payload; already-transmitted
                        // images stay on the host, so re-placing happens only when the
                        // image was hidden and is being revealed again, during animation,
                        // or when a scroll reflow shifted anchors (reemit_gfx).

                        let fresh = if let Some((crop_data, _, _)) = &crop {
                            // image is clipped by the viewport; cropping takes
                            // precedence over ordinary re-placement.
                            client.emitted_gfx.insert(key);
                            client.hidden_gfx.remove(&key);
                            Some(crop_data.clone())
                        } else if was_emitted {
                            let was_hidden = client.hidden_gfx.remove(&key);
                            // on force_full: always re-emit if was hidden (modal->normal, workspace switch)
                            // for non-hidden kitty: skip (persists on host), but sixel/iterm2 need re-emission
                            let needs_reemit: Option<Vec<u8>> = if force_full {
                                // a full repaint issues ESC[2J and the host
                                // clears its images with it, so every format
                                // has to be re-transmitted. exception: a
                                // transmission that reads its pixels from a
                                // file. the app deletes that file right after
                                // the first transmission (kitten icat does),
                                // so re-sending it would fail on the host and
                                // the image would vanish. those are re-placed
                                // instead, which keeps them alive while the
                                // host still has the pixels.
                                if cmd.reads_pixels_from_file() {
                                    // the file is gone by now, so the pixels
                                    // we cached at the first transmission are
                                    // sent inline. the host gets the image
                                    // back instead of losing it to the ED2.
                                    cmd.retransmit_with_cached_bytes()
                                        .or_else(|| cmd.reemit.clone())
                                } else {
                                    Some(cmd.emission_payload())
                                }
                            } else if was_hidden || reemit_gfx || is_animating {
                                cmd.reemit.clone().or_else(|| Some(cmd.emission_payload()))
                            } else {
                                None
                            };

                            needs_reemit
                        } else {
                            client.emitted_gfx.insert(key);
                            client.hidden_gfx.remove(&key);
                            Some(cmd.emission_payload())
                        };

                        let Some(data) = fresh else { continue };
                        // every emitted command gets a fresh global sequence:
                        // the client drops commands that are not newer than
                        // the last one it saw, and the per-store command
                        // sequence and the global delete sequence are
                        // different counters - an image painted after a
                        // delete was silently dropped by the client.
                        self.gfx_delete_seq = self.gfx_delete_seq.wrapping_add(1).max(1);
                        gfx_items.push(crate::proto::GfxCmd {
                            row,
                            col,
                            format,
                            seq: self.gfx_delete_seq,
                            data,
                        });
                    }
                    // the store lock is held for the whole loop above; release
                    // it before touching the eviction order.
                    pane.gfx_dirty.store(false, Ordering::Release);
                    drop(store);
                    if !used_placements.is_empty()
                        && let Ok(mut store) = pane.graphics.lock()
                    {
                        for seq in &used_placements {
                            store.touch(*seq);
                        }
                    }
                }
            }
            client
                .terminal
                .backend_mut()
                .set_pending_gfx(gfx_items, reemit_gfx);

            // force full redraw for new clients
            if client_force_full {
                client.terminal.backend_mut().set_full_redraw(true);
            }

            let draw_result = client.terminal.draw(|f| {
                let mut cursor: Option<Position> = None;
                {
                    let buf = f.buffer_mut();

                    if let Some(ws_rect) = ws_rect {
                        render_workspace_bar(
                            ws_rect,
                            buf,
                            &workspaces,
                            active_ws,
                            session.name.as_str(),
                            theme,
                            workspace_bar_pos,
                            self.tab_scroll_offset,
                        );
                    }
                    if let Some(sidebar_rect) = sidebar_rect {
                        render_workspace_sidebar(
                            sidebar_rect,
                            buf,
                            &sidebar_tabs,
                            active_ws,
                            session.name.as_str(),
                            theme,
                            self.sidebar_collapsed,
                            config.ui.sidebar_density,
                            config.ui.sidebar_header,
                            self.tab_scroll_offset,
                        );
                    }

                    // the vim command bar needs a row even when the hint bar is
                    // disabled, otherwise typing `:` produces no visible prompt.
                    let vim_hint_rect = hint_rect.or_else(|| {
                        if mode == InputMode::VimCmd && area.height > 2 {
                            Some(Rect::new(area.x, area.y + area.height - 1, area.width, 1))
                        } else {
                            None
                        }
                    });
                    if let Some(hint_rect) = vim_hint_rect {
                        if mode == InputMode::VimCmd {
                            render_vim_cmd_bar(
                                area,
                                Some(hint_rect),
                                buf,
                                &self.vim_cmd_buf,
                                theme,
                                show_clock
                                    && !palette_open
                                    && !theme_menu_open
                                    && !settings_menu_open
                                    && !show_welcome
                                    && !show_help,
                                config.ui.clock_format.as_str(),
                            );
                        } else if config.ui.hint_bar {
                            render_hint_bar(
                                hint_rect,
                                buf,
                                mode,
                                &hint_text,
                                theme,
                                show_clock
                                    && !palette_open
                                    && !theme_menu_open
                                    && !settings_menu_open
                                    && !show_welcome
                                    && !show_help,
                                config.ui.clock_format.as_str(),
                            );
                        }
                    }

                    // use rects as-is - no slide offset, no clamping needed since we're not moving them
                    // rects match the PTY sizes that were set earlier, so content displays fully

                    for (id, rect) in &rects {
                        let Some(pane) = panes.get(id) else {
                            continue;
                        };

                        // use animated rect if animation is active, otherwise use computed rect
                        let display_rect =
                            self.pane_animations.get_animated_rect(*id).unwrap_or(*rect);

                        // help overlays must not dim the pane behind it (that's
                        // the whole point of a reference cheat-sheet); other
                        // dialogs dim panes deliberately.
                        let dialog_open =
                            (show_welcome || palette_open || theme_menu_open || settings_menu_open)
                                && !show_help;
                        let is_focused = focused_id == Some(*id) && !dialog_open;
                        let use_border = border && pane_count > 0;
                        let inner = inner_rect(display_rect, use_border);
                        if use_border {
                            let title = if config.decorations.title {
                                if is_focused && config.decorations.pet {
                                    format!("{}  {}", config.decorations.pet_emoji, pane.title)
                                } else {
                                    pane.title.clone()
                                }
                            } else {
                                String::new()
                            };
                            let scroll_back = config
                                .decorations
                                .scroll_indicator
                                .then_some(pane.scroll_offset)
                                .filter(|offset| *offset > 0);
                            render_pane_border(
                                display_rect,
                                buf,
                                &title,
                                is_focused,
                                theme,
                                ascii,
                                config.decorations.border_style,
                                scroll_back,
                            );
                        }
                        if inner.width == 0 || inner.height == 0 {
                            continue;
                        }

                        // during animation, only redraw if this specific pane's rect changed
                        // this avoids full screen repaints on every animation frame
                        let should_render_content = if self.pane_animations.has_active() {
                            // only render if this pane is animating
                            self.pane_animations.get_animated_rect(*id).is_some()
                        } else {
                            true
                        };

                        if !should_render_content {
                            continue;
                        }

                        if let Ok(mut parser) = pane.parser.lock() {
                            parser.screen_mut().set_scrollback(pane.scroll_offset);

                            // clear the area first to prevent background bleed from apps
                            use ratatui::widgets::{Clear, Widget};
                            Clear.render(inner, buf);

                            let scrollback_len = crate::pane::search::scrollback_len(&mut parser);
                            let kitty_id_map = pane
                                .graphics
                                .lock()
                                .map(|store| store.kitty_id_map().clone())
                                .unwrap_or_default();
                            let view = PaneView {
                                screen: parser.screen(),
                                theme,
                                is_focused,
                                dim_inactive,
                                dim_strength,
                                selection: pane.selection.as_ref(),
                                search: pane.search.as_ref(),
                                scroll_offset: pane.scroll_offset,
                                scrollback_len,
                                kitty_id_map,
                            };
                            view.render(inner, buf);

                            if is_focused
                                && pane.scroll_offset == 0
                                && !show_welcome
                                && !show_help
                                && !palette_open
                                && !theme_menu_open
                                && !context_menu_open
                            {
                                let (crow, ccol) = parser.screen().cursor_position();
                                if !parser.screen().hide_cursor()
                                    && ccol < inner.width
                                    && crow < inner.height
                                {
                                    cursor = Some(Position::new(inner.x + ccol, inner.y + crow));
                                }
                            }
                            parser.screen_mut().set_scrollback(0);
                        }
                    }

                    if let Some(started) = cat_started
                        && !show_welcome
                        && !show_help
                        && !palette_open
                        && !theme_menu_open
                        && !settings_menu_open
                        && !context_menu_open
                        && let Some((_, rect)) =
                            rects.iter().find(|(id, _)| Some(*id) == focused_id)
                    {
                        let use_border = border && pane_count > 0;
                        let inner = inner_rect(*rect, use_border);
                        crate::render::cat::render_cat(
                            inner,
                            buf,
                            theme,
                            started.elapsed(),
                            cat_boop_at.map(|i| i.elapsed()),
                            ascii,
                        );
                    }

                    if mode == InputMode::VimCmd
                        && let Some(hint_rect) = hint_rect
                    {
                        render_vim_cmd_popup(
                            hint_rect,
                            buf,
                            &self.vim_cmd_buf,
                            self.vim_cmd_selected,
                            theme,
                            ascii,
                        );
                    }

                    if show_welcome {
                        render_welcome(area, buf, theme, ascii);
                    } else if show_help {
                        render_help(
                            area,
                            buf,
                            theme,
                            ascii,
                            self.help_scroll,
                            config.profile.mode,
                        );
                    } else if palette_open {
                        render_palette(area, buf, &self.palette, theme, ascii);
                    } else if theme_menu_open {
                        render_theme_menu(area, buf, &self.theme_menu, theme, ascii);
                    } else if settings_menu_open {
                        render_settings_menu(area, buf, &self.settings_menu, config, theme, ascii);
                    } else if self.mode == InputMode::ContextMenu {
                        crate::render::context_menu::render_context_menu(
                            area,
                            buf,
                            &self.context_menu,
                            theme,
                            ascii,
                        );
                    }

                    if let Some((msg, _)) = &self.toast {
                        render_toast(area, buf, msg, theme);
                    }
                }

                if let Some(pos) = cursor {
                    f.set_cursor_position(pos);
                }
            });

            if draw_result.is_err() {
                // the client cannot take a frame: drop it instead of pinning
                // the session with a client that never reads.
                failed_clients.push(client.id);
                continue;
            }

            let cursor_shape = match self.mode {
                InputMode::Copy => crate::proto::CursorShape::Underline,
                InputMode::Search => crate::proto::CursorShape::Underline,
                InputMode::Resize => crate::proto::CursorShape::Bar,
                _ => crate::proto::CursorShape::Block,
            };
            client.terminal.backend_mut().set_cursor_shape(cursor_shape);

            let mut update = client.terminal.backend_mut().take_screen_update();
            // ensure cursor is hidden when modals are open
            if show_welcome
                || show_help
                || palette_open
                || theme_menu_open
                || settings_menu_open
                || context_menu_open
            {
                update.cursor_visible = false;
                update.cursor_pos = None;
            }
            if write_msg(&mut client.stream, &ServerMsg::Update(update)).is_err() {
                // dead or stuck client: drop it instead of pinning the session
                failed_clients.push(client.id);
            }
        }
        // mirror the per-client bookkeeping into the session-wide sets: they
        // track the union over all attached clients and are used for the
        // stale-entry sweep and the bookkeeping cap. the session-wide sets
        // are derived from the per-client sets, not accumulated: a key that
        // a client removed from its hidden set (because the image was
        // re-emitted) must disappear from the session-wide set too.
        {
            let mut emitted: HashSet<(PaneId, u64)> = HashSet::new();
            let mut hidden: HashSet<(PaneId, u64)> = HashSet::new();
            for client in self.clients.values() {
                emitted.extend(client.emitted_gfx.iter().copied());
                hidden.extend(client.hidden_gfx.iter().copied());
            }
            // tests push graphics directly into the store without a client
            // ever having recorded them; preserve those entries so the
            // session-wide sets stay non-empty for inspection.
            if self.clients.is_empty() {
                emitted = self.emitted_gfx.clone();
                hidden = self.hidden_gfx.clone();
            }
            self.emitted_gfx = emitted;
            self.hidden_gfx = hidden;
        }
        if self.emitted_gfx.len() > 512 {
            // cap the bookkeeping; worst case re-sends a pixel payload once.
            self.emitted_gfx.clear();
            self.hidden_gfx.clear();
            for client in self.clients.values_mut() {
                client.emitted_gfx.clear();
                client.hidden_gfx.clear();
            }
        }
        if !failed_clients.is_empty() {
            for id in &failed_clients {
                tracing::warn!("dropping client {id} after failed update write");
                self.clients.remove(id);
            }
            self.ui_dirty = true;
        }

        // clean up finished animations
        self.pane_animations.cleanup_finished();
    }

    /// computes chrome rects. returns (horizontal bar, vertical sidebar, hint
    /// bar, content). sidebar positions reserve a left/right column; top/bottom
    /// reserve a row; off reserves nothing.
    pub fn chrome_areas_for(
        config: &Config,
        area: Rect,
        sidebar_collapsed: bool,
    ) -> (Option<Rect>, Option<Rect>, Option<Rect>, Rect) {
        let sidebar = match config.ui.workspace_bar {
            WorkspaceBarPosition::Left => {
                let w = Self::sidebar_width_for(config, sidebar_collapsed)
                    .min(area.width.saturating_sub(10));
                (w > 0).then(|| Rect::new(area.x, area.y, w, area.height))
            }
            WorkspaceBarPosition::Right => {
                let w = Self::sidebar_width_for(config, sidebar_collapsed)
                    .min(area.width.saturating_sub(10));
                (w > 0).then(|| {
                    Rect::new(
                        area.x + area.width.saturating_sub(w),
                        area.y,
                        w,
                        area.height,
                    )
                })
            }
            _ => None,
        };

        // content rectangle loses the sidebar column (full-height sidebar, no
        // separate top/bottom row for it).
        let (content_x, content_width) = match &sidebar {
            Some(r) if config.ui.workspace_bar == WorkspaceBarPosition::Left => {
                (r.x + r.width, area.width.saturating_sub(r.width))
            }
            Some(r) => (area.x, area.width.saturating_sub(r.width)),
            None => (area.x, area.width),
        };

        let mut top = area.y;
        let mut bottom = area.y + area.height;

        let hint = if config.ui.hint_bar && bottom > top + 1 {
            bottom -= 1;
            Some(Rect::new(content_x, bottom, content_width, 1))
        } else {
            None
        };

        let ws_bar = match config.ui.workspace_bar {
            WorkspaceBarPosition::Top if area.height > 2 => {
                let r = Rect::new(content_x, top, content_width, 1);
                top += 1;
                Some(r)
            }
            WorkspaceBarPosition::Bottom if bottom > top + 1 => {
                bottom -= 1;
                Some(Rect::new(content_x, bottom, content_width, 1))
            }
            _ => None,
        };

        let content = Rect::new(content_x, top, content_width, bottom.saturating_sub(top));
        (ws_bar, sidebar, hint, content)
    }

    /// hit-test for workspace tabs on the top/bottom bar and the vertical
    /// sidebar. returns the workspace id if the click landed on a tab slot.
    fn tab_at_position(
        &self,
        ws_rect: Option<Rect>,
        sidebar_rect: Option<Rect>,
        msg: &crate::proto::MouseEventMsg,
    ) -> Option<usize> {
        let workspaces: Vec<(usize, String, bool)> = self
            .session
            .workspace_ids()
            .into_iter()
            .map(|id| (id, self.session.workspace_name(id), self.workspace_busy(id)))
            .collect();

        if let Some(bar) = ws_rect
            && msg.row == bar.y
        {
            let layout = crate::render::chrome::layout_workspace_bar(
                bar,
                &workspaces,
                self.tab_scroll_offset,
            );
            for tab in &layout.tabs {
                if msg.col >= tab.pill_x && msg.col < tab.pill_x + tab.pill_w {
                    return Some(tab.ws);
                }
            }
            return None;
        }

        if let Some(bar) = sidebar_rect
            && msg.col >= bar.x
            && msg.col < bar.x + bar.width
        {
            let layout = crate::render::chrome::layout_workspace_sidebar(
                bar,
                &workspaces,
                self.tab_scroll_offset,
                self.config.ui.sidebar_density,
                self.sidebar_collapsed,
                self.config.ui.sidebar_header,
            );
            for tab in &layout.slots {
                if msg.row >= tab.y && msg.row < tab.y + tab.row_h {
                    return Some(tab.ws);
                }
            }
        }
        None
    }

    /// effective width of the vertical sidebar.
    fn sidebar_width_for(config: &Config, sidebar_collapsed: bool) -> u16 {
        if sidebar_collapsed {
            config.ui.sidebar_collapsed_width
        } else {
            config.ui.sidebar_width
        }
    }

    fn update_toast(&mut self) {
        if let Some((_, created)) = &self.toast
            && created.elapsed() > Duration::from_secs(3)
        {
            self.toast = None;
            self.ui_dirty = true;
        }
    }

    fn hint_text(&self) -> String {
        match self.mode {
            InputMode::ContextMenu => "up/down move · enter/click select · esc cancel".to_string(),
            InputMode::VimCmd => "tab complete · enter run".to_string(),
            InputMode::Search => "enter next · shift+enter prev".to_string(),
            InputMode::Resize => "h/j/k/l or arrows resize · enter/esc done".to_string(),
            InputMode::Copy => "h/j/k/l move · v select · y yank · q exit".to_string(),
            InputMode::SettingsMenu => {
                if self.settings_menu.editing.is_some() {
                    "enter save · esc cancel".to_string()
                } else {
                    "up/down select · space toggle · enter edit".to_string()
                }
            }
            InputMode::Palette => "enter run · esc cancel".to_string(),
            InputMode::ThemeMenu => "up/down preview · enter apply".to_string(),
            InputMode::Help => "up/down scroll · esc close".to_string(),
            InputMode::Normal => "f1 help · f2 settings · alt+1-9 workspaces".to_string(),
        }
    }
}

fn shell_quote_word(word: &str) -> String {
    if !word.is_empty()
        && word
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._/:=+@%,".contains(&b))
    {
        return word.to_string();
    }
    let mut quoted = String::with_capacity(word.len() + 2);
    quoted.push('\'');
    for ch in word.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

fn contains(rect: &Rect, col: u16, row: u16) -> bool {
    col >= rect.x
        && u32::from(col) < u32::from(rect.x) + u32::from(rect.width)
        && row >= rect.y
        && u32::from(row) < u32::from(rect.y) + u32::from(rect.height)
}

fn resize_floating_rect(
    content: Rect,
    origin: Rect,
    corner: FloatingCorner,
    mouse_col: u16,
    mouse_row: u16,
) -> Rect {
    if content.width == 0 || content.height == 0 {
        return content;
    }
    let left = i32::from(content.x);
    let top = i32::from(content.y);
    let right = left + i32::from(content.width);
    let bottom = top + i32::from(content.height);
    let min_width = i32::from(content.width.min(10));
    let min_height = i32::from(content.height.min(4));
    let origin_left = i32::from(origin.x).clamp(left, right - min_width);
    let origin_top = i32::from(origin.y).clamp(top, bottom - min_height);
    let origin_right =
        (i32::from(origin.x) + i32::from(origin.width)).clamp(origin_left + min_width, right);
    let origin_bottom =
        (i32::from(origin.y) + i32::from(origin.height)).clamp(origin_top + min_height, bottom);
    let mouse_col = i32::from(mouse_col).clamp(left, right.saturating_sub(1));
    let mouse_row = i32::from(mouse_row).clamp(top, bottom.saturating_sub(1));

    let (x, y, pane_right, pane_bottom) = match corner {
        FloatingCorner::BottomRight => (
            origin_left,
            origin_top,
            (mouse_col + 1).clamp(origin_left + min_width, right),
            (mouse_row + 1).clamp(origin_top + min_height, bottom),
        ),
        FloatingCorner::BottomLeft => (
            (mouse_col).clamp(left, origin_right - min_width),
            origin_top,
            origin_right,
            (mouse_row + 1).clamp(origin_top + min_height, bottom),
        ),
        FloatingCorner::TopRight => (
            origin_left,
            mouse_row.clamp(top, origin_bottom - min_height),
            (mouse_col + 1).clamp(origin_left + min_width, right),
            origin_bottom,
        ),
        FloatingCorner::TopLeft => (
            mouse_col.clamp(left, origin_right - min_width),
            mouse_row.clamp(top, origin_bottom - min_height),
            origin_right,
            origin_bottom,
        ),
    };

    Rect::new(
        u16::try_from(x).unwrap_or(content.x),
        u16::try_from(y).unwrap_or(content.y),
        u16::try_from(pane_right - x).unwrap_or(0),
        u16::try_from(pane_bottom - y).unwrap_or(0),
    )
}

fn inner_rect(rect: Rect, border: bool) -> Rect {
    if border && rect.width > 2 && rect.height > 2 {
        Rect::new(
            rect.x + 1,
            rect.y + 1,
            rect.width.saturating_sub(2),
            rect.height.saturating_sub(2),
        )
    } else {
        rect
    }
}

#[cfg(test)]
mod floating_resize_tests {
    use super::{CoreEvent, FloatingCorner, ServerCore, resize_floating_rect};
    use crate::layout::{PaneId, SplitAxis};
    use ratatui::layout::Rect;
    use std::os::unix::net::UnixStream;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn floating_resize_stays_inside_tiny_content_for_every_corner() {
        let content = Rect::new(3, 5, 3, 2);
        let origin = Rect::new(3, 5, 3, 2);

        for corner in [
            FloatingCorner::TopLeft,
            FloatingCorner::TopRight,
            FloatingCorner::BottomLeft,
            FloatingCorner::BottomRight,
        ] {
            let rect = resize_floating_rect(content, origin, corner, u16::MAX, u16::MAX);
            assert!(rect.x >= content.x);
            assert!(rect.y >= content.y);
            assert!(
                u32::from(rect.x) + u32::from(rect.width)
                    <= u32::from(content.x) + u32::from(content.width)
            );
            assert!(
                u32::from(rect.y) + u32::from(rect.height)
                    <= u32::from(content.y) + u32::from(content.height)
            );
        }
    }

    #[test]
    fn floating_resize_handles_empty_content() {
        let content = Rect::new(3, 5, 0, 0);
        let rect = resize_floating_rect(
            content,
            Rect::new(3, 5, 10, 4),
            FloatingCorner::BottomRight,
            u16::MAX,
            u16::MAX,
        );
        assert_eq!(rect, content);
    }

    #[test]
    fn floating_resize_clamps_a_stale_origin_to_current_content() {
        let content = Rect::new(10, 8, 12, 6);
        let origin = Rect::new(30, 20, 8, 8);

        for corner in [
            FloatingCorner::TopLeft,
            FloatingCorner::TopRight,
            FloatingCorner::BottomLeft,
            FloatingCorner::BottomRight,
        ] {
            let rect = resize_floating_rect(content, origin, corner, 10, 8);
            assert!(rect.x >= content.x);
            assert!(rect.y >= content.y);
            assert!(
                u32::from(rect.x) + u32::from(rect.width)
                    <= u32::from(content.x) + u32::from(content.width)
            );
            assert!(
                u32::from(rect.y) + u32::from(rect.height)
                    <= u32::from(content.y) + u32::from(content.height)
            );
        }
    }

    #[test]
    fn daemon_starts_initial_shell_at_first_client_size() {
        let running = Arc::new(AtomicBool::new(true));
        let (mut core, _) = ServerCore::new_for_server(
            "first-size-test",
            crate::config::Config::default(),
            crate::config::Theme::default_theme(),
            running,
            None,
        );
        assert!(core.session.panes.is_empty());

        let (server, _client) = UnixStream::pair().expect("create test client stream");
        core.handle_event(CoreEvent::NewClient {
            client_id: 1,
            stream: server,
            cols: 160,
            rows: 50,
            term: "xterm-kitty".into(),
            cell_pixel: Some((10, 20)),
        });

        let pane_id = core.session.focused_pane.expect("first pane is focused");
        let pane = core.session.panes.get(&pane_id).expect("first pane exists");
        let parser = pane.parser.lock().expect("lock initial pane parser");
        let (rows, cols) = parser.screen().size();
        assert!(
            cols > 80,
            "first shell must not start with guessed width: {cols}"
        );
        assert!(
            rows > 24,
            "first shell must not start with guessed height: {rows}"
        );
    }

    #[test]
    fn first_client_terminal_selects_new_pane_graphics_support() {
        let running = Arc::new(AtomicBool::new(true));
        let (mut core, _) = ServerCore::new_for_server(
            "client-graphics-test",
            crate::config::Config::default(),
            crate::config::Theme::default_theme(),
            running,
            None,
        );
        let (server, _client) = UnixStream::pair().expect("create test client stream");

        core.handle_event(CoreEvent::NewClient {
            client_id: 1,
            stream: server,
            cols: 100,
            rows: 30,
            term: "foot".into(),
            cell_pixel: Some((10, 20)),
        });

        assert_eq!(core.session.outer_term, "foot");
        assert_eq!(
            core.session.outer_graphics,
            crate::util::outer_term::OuterGraphics::Sixel
        );
    }

    #[test]
    fn client_resize_keeps_kitty_source_for_full_redraw() {
        let running = Arc::new(AtomicBool::new(true));
        let (mut core, _) = ServerCore::new(
            "resize-graphics-test",
            crate::config::Config::default(),
            crate::config::Theme::default_theme(),
            running,
            None,
        );
        let (server, mut client_stream) = UnixStream::pair().expect("create test client stream");
        client_stream
            .set_nonblocking(true)
            .expect("set client stream nonblocking");
        core.handle_event(CoreEvent::NewClient {
            client_id: 1,
            stream: server,
            cols: 100,
            rows: 30,
            term: "xterm-kitty".into(),
            cell_pixel: Some((10, 20)),
        });
        core.render_all();
        drain_test_client(&mut client_stream);

        let pane_id = core.session.focused_pane.expect("initial pane is focused");
        let pane = core
            .session
            .panes
            .get(&pane_id)
            .expect("initial pane exists");
        let graphics = Arc::clone(&pane.graphics);
        let mut parser = crate::pane::GraphicsParser::new();
        let command = parser
            .feed(b"\x1b_Ga=T,f=100,i=71,c=2,r=2;AAAA\x1b\\", 0, 0)
            .remove(0);
        graphics.lock().expect("lock graphics store").push(command);

        core.handle_event(CoreEvent::ClientResize {
            client_id: 1,
            cols: 120,
            rows: 40,
        });
        core.render_all();

        let store = graphics.lock().expect("lock graphics store after resize");
        assert!(
            !store.active_commands().is_empty(),
            "a resize must not drop the pane's images: they are re-placed (cropped \
             to the new viewport) by the full repaint, and dropping them made every \
             image vanish on resize"
        );
    }

    #[test]
    fn workspace_roundtrip_replays_hidden_kitty_image() {
        let running = Arc::new(AtomicBool::new(true));
        let mut config = crate::config::Config::default();
        config.animation.enabled = false;
        config.ui.show_welcome = false;
        config.general.graphics = true;
        let (mut core, _) = ServerCore::new(
            "workspace-graphics-test",
            config,
            crate::config::Theme::default_theme(),
            running,
            None,
        );
        let (server, mut client_stream) = UnixStream::pair().expect("create test client stream");
        client_stream
            .set_nonblocking(true)
            .expect("set client stream nonblocking");
        core.handle_event(CoreEvent::NewClient {
            client_id: 1,
            stream: server,
            cols: 100,
            rows: 30,
            term: "xterm-kitty".into(),
            cell_pixel: Some((10, 20)),
        });
        core.render_all();

        let pane_id = core.session.focused_pane.expect("initial pane is focused");
        let graphics = Arc::clone(
            &core
                .session
                .panes
                .get(&pane_id)
                .expect("initial pane exists")
                .graphics,
        );
        let mut parser = crate::pane::GraphicsParser::new();
        let command = parser
            .feed(b"\x1b_Ga=T,f=100,i=72,c=2,r=2;BBBB\x1b\\", 0, 0)
            .remove(0);
        graphics.lock().expect("lock graphics store").push(command);
        core.render_all();
        drain_test_client(&mut client_stream);
        let image_seq = graphics.lock().unwrap().active_commands()[0].seq;
        let key = (pane_id, image_seq);
        assert!(core.emitted_gfx.contains(&key));

        let before_hide = core.gfx_delete_seq;
        core.goto_workspace(2);
        core.render_all();
        drain_test_client(&mut client_stream);
        assert!(core.hidden_gfx.contains(&key));
        assert!(core.gfx_delete_seq > before_hide);

        let before_restore = core.gfx_delete_seq;
        core.goto_workspace(1);
        core.render_all();
        let restored_update = drain_test_client(&mut client_stream);
        assert!(!core.hidden_gfx.contains(&key));
        assert!(core.gfx_delete_seq > before_restore);
        assert!(
            restored_update.windows(4).any(|window| window == b"BBBB"),
            "returning to a workspace must resend Kitty pixels after the full-screen clear"
        );
        assert_eq!(
            graphics.lock().unwrap().active_commands().len(),
            1,
            "workspace switching must preserve source graphics"
        );
    }

    #[test]
    fn modal_hides_kitty_image_then_reveals_on_close() {
        let running = Arc::new(AtomicBool::new(true));
        let mut config = crate::config::Config::default();
        config.animation.enabled = false;
        config.ui.show_welcome = false;
        config.general.graphics = true;
        let (mut core, _) = ServerCore::new(
            "modal-graphics-test",
            config,
            crate::config::Theme::default_theme(),
            running,
            None,
        );
        let (server, mut client_stream) = UnixStream::pair().expect("create test client stream");
        client_stream
            .set_nonblocking(true)
            .expect("set client stream nonblocking");
        core.handle_event(CoreEvent::NewClient {
            client_id: 1,
            stream: server,
            cols: 100,
            rows: 30,
            term: "xterm-kitty".into(),
            cell_pixel: Some((10, 20)),
        });
        core.render_all();

        let pane_id = core.session.focused_pane.expect("initial pane is focused");
        let graphics = Arc::clone(
            &core
                .session
                .panes
                .get(&pane_id)
                .expect("initial pane exists")
                .graphics,
        );
        let mut parser = crate::pane::GraphicsParser::new();
        let command = parser
            .feed(b"\x1b_Ga=T,f=100,i=73,c=2,r=2;DDDD\x1b\\", 0, 0)
            .remove(0);
        graphics.lock().expect("lock graphics store").push(command);
        core.render_all();
        drain_test_client(&mut client_stream);
        let image_seq = graphics.lock().unwrap().active_commands()[0].seq;
        let key = (pane_id, image_seq);
        assert!(core.emitted_gfx.contains(&key));

        let before_hide = core.gfx_delete_seq;
        core.handle_event(CoreEvent::ClientCommand {
            client_id: 1,
            command: "settings_menu".to_string(),
        });
        core.render_all();
        drain_test_client(&mut client_stream);
        assert!(core.hidden_gfx.contains(&key));
        assert!(core.gfx_delete_seq > before_hide);

        let before_restore = core.gfx_delete_seq;
        // layout commands are blocked while a modal is open, so the test
        // closes the settings menu by dispatching the Escape key directly
        // (an integration test would go through the wire and a 35 ms flush)
        core.handle_key(crate::input::Key {
            code: crate::input::KeyCode::Esc,
            modifiers: crate::input::Modifiers::NONE,
        });
        assert_eq!(core.mode, crate::input::InputMode::Normal);
        core.render_all();
        let restored_update = drain_test_client(&mut client_stream);
        assert!(!core.hidden_gfx.contains(&key));
        assert!(core.gfx_delete_seq > before_restore);
        assert!(
            restored_update.windows(4).any(|window| window == b"DDDD"),
            "closing the settings menu must resend Kitty pixels after the full-screen clear"
        );
    }

    #[test]
    fn closing_one_pane_keeps_the_other_panes_kitty_image() {
        let running = Arc::new(AtomicBool::new(true));
        let (mut core, _) = ServerCore::new(
            "close-pane-graphics-test",
            crate::config::Config::default(),
            crate::config::Theme::default_theme(),
            running,
            None,
        );
        core.spawn_pane(Some(SplitAxis::Horizontal), false);
        let mut pane_ids: Vec<PaneId> = core.session.panes.keys().copied().collect();
        pane_ids.sort_unstable();
        assert_eq!(pane_ids.len(), 2);
        let (closing, remaining) = (pane_ids[0], pane_ids[1]);

        let mut parser = crate::pane::GraphicsParser::new();
        for pane_id in [closing, remaining] {
            let command = parser
                .feed(b"\x1b_Ga=T,f=100,i=5,c=2,r=2;CCCC\x1b\\", 0, 0)
                .remove(0);
            core.session
                .panes
                .get(&pane_id)
                .unwrap()
                .graphics
                .lock()
                .unwrap()
                .push(command);
        }
        let image_id = |pane_id: PaneId| {
            core.session.panes[&pane_id]
                .graphics
                .lock()
                .unwrap()
                .active_commands()[0]
                .kitty_metadata
                .as_ref()
                .and_then(|metadata| metadata.i)
                .unwrap()
        };
        let closing_image_id = image_id(closing);
        let remaining_image_id = image_id(remaining);
        assert_ne!(closing_image_id, remaining_image_id);

        core.drop_pane_graphics_hard(closing);

        assert!(
            core.session.panes[&closing]
                .graphics
                .lock()
                .unwrap()
                .active_commands()
                .is_empty()
        );
        assert_eq!(
            core.session.panes[&remaining]
                .graphics
                .lock()
                .unwrap()
                .active_commands()
                .len(),
            1
        );
        assert!(core.pending_gfx_deletes.iter().any(|delete| {
            delete
                .data
                .windows(closing_image_id.to_string().len())
                .any(|window| window == closing_image_id.to_string().as_bytes())
        }));
        assert!(!core.pending_gfx_deletes.iter().any(|delete| {
            delete
                .data
                .windows(remaining_image_id.to_string().len())
                .any(|window| window == remaining_image_id.to_string().as_bytes())
        }));
    }

    fn drain_test_client(stream: &mut UnixStream) -> Vec<u8> {
        use std::io::Read;

        let mut collected = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => collected.extend_from_slice(&buffer[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => panic!("read rendered test update: {error}"),
            }
        }
        collected
    }
}
