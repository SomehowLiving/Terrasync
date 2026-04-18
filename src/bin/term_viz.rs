//! Discrete 10×10 grid terminal visualization — demo script + optional stdin JSON (viz bridge format).

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, BufRead};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use rand::Rng;
use serde_json::Value;

const GRID_W: i32 = 10;
const GRID_H: i32 = 10;
const EVENT_LOG_MAX: usize = 24;

#[derive(Parser, Debug)]
#[command(name = "term-viz")]
struct Args {
    /// Run the scripted “wow” demo (5 agents, target, resolution, kill, recovery).
    #[arg(long)]
    demo: bool,

    /// Read JSON events from stdin (one JSON object per line), same shape as viz bridge.
    #[arg(long)]
    live: bool,

    /// Milliseconds between automatic ticks in normal mode (slow motion).
    #[arg(long, default_value = "750")]
    tick_ms: u64,

    /// After demo stability, auto-kill winner after this many seconds (0 = wait for K only).
    #[arg(long, default_value = "8")]
    auto_kill_secs: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TickMode {
    /// Automatic ticks every `interval_ms`.
    Normal,
    /// One tick per key (`n` / space).
    Step,
}

struct App {
    /// Agent id -> state
    agents: HashMap<String, Agent>,
    target: Option<(i32, i32)>,
    winner_id: Option<String>,
    round: u64,
    claims: Vec<String>,
    event_log: VecDeque<String>,
    banner: String,
    tick_mode: TickMode,
    interval_ms: u64,
    /// Demo-only: scripted phase
    demo_phase: DemoPhase,
    /// When set, wall-clock pause before advancing demo
    demo_wait_until: Option<Instant>,
    auto_kill_at: Option<Instant>,
    args_auto_kill_secs: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DemoPhase {
    Off,
    /// Phase 1 — scattered + target
    Setup,
    /// Phase 2 — move toward F
    Converge,
    /// Phase 3 — dramatic pause
    Freeze,
    /// Phase 4 — winner + reroute
    Resolve,
    /// Phase 5 — idle on grid
    Stability,
    /// Phase 7 — reconverge
    Recovery,
}

struct Agent {
    #[allow(dead_code)]
    id: String,
    x: i32,
    y: i32,
    path: Vec<(i32, i32)>,
    /// Losers moving to new cells after resolution.
    rerouting: bool,
}

impl App {
    fn new(args: &Args) -> Self {
        Self {
            agents: HashMap::new(),
            target: None,
            winner_id: None,
            round: 0,
            claims: Vec::new(),
            event_log: VecDeque::new(),
            banner: String::new(),
            tick_mode: TickMode::Normal,
            interval_ms: args.tick_ms,
            demo_phase: if args.demo {
                DemoPhase::Setup
            } else {
                DemoPhase::Off
            },
            demo_wait_until: None,
            auto_kill_at: None,
            args_auto_kill_secs: args.auto_kill_secs,
        }
    }

    fn push_event(&mut self, s: String) {
        if self.event_log.len() >= EVENT_LOG_MAX {
            self.event_log.pop_front();
        }
        self.event_log.push_back(s);
    }

    fn clamp(x: i32, y: i32) -> (i32, i32) {
        (x.clamp(0, GRID_W - 1), y.clamp(0, GRID_H - 1))
    }

    fn manhattan_path(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(i32, i32)> {
        let mut path = vec![(x0, y0)];
        let mut x = x0;
        let mut y = y0;
        while x != x1 {
            x += if x < x1 { 1 } else { -1 };
            path.push((x, y));
        }
        while y != y1 {
            y += if y < y1 { 1 } else { -1 };
            path.push((x, y));
        }
        path
    }

    fn get_or_create_agent(&mut self, id: &str) -> &mut Agent {
        if !self.agents.contains_key(id) {
            let mut rng = rand::thread_rng();
            let (x, y) = Self::clamp(
                rng.gen_range(0..GRID_W),
                rng.gen_range(0..GRID_H),
            );
            self.agents.insert(
                id.to_string(),
                Agent {
                    id: id.to_string(),
                    x,
                    y,
                    path: Vec::new(),
                    rerouting: false,
                },
            );
        }
        self.agents.get_mut(id).unwrap()
    }

    fn cell_key(x: i32, y: i32) -> String {
        format!("{},{}", x, y)
    }

    fn allocate_loser_cells(&self, tx: i32, ty: i32, loser_ids: &[String]) -> HashMap<String, (i32, i32)> {
        let mut occupied: HashSet<String> = HashSet::new();
        occupied.insert(Self::cell_key(tx, ty));
        for a in self.agents.values() {
            occupied.insert(Self::cell_key(a.x, a.y));
        }
        let mut candidates = Vec::new();
        for d in 1i32..(GRID_W.max(GRID_H) * 2) {
            for dx in -d..=d {
                for dy in -d..=d {
                    if dx.abs() + dy.abs() != d {
                        continue;
                    }
                    let nx = tx + dx;
                    let ny = ty + dy;
                    if nx < 0 || nx >= GRID_W || ny < 0 || ny >= GRID_H {
                        continue;
                    }
                    if nx == tx && ny == ty {
                        continue;
                    }
                    candidates.push((nx, ny));
                }
            }
        }
        let mut out = HashMap::new();
        let mut ci = 0usize;
        for lid in loser_ids {
            while ci < candidates.len() {
                let (nx, ny) = candidates[ci];
                ci += 1;
                let k = Self::cell_key(nx, ny);
                if !occupied.contains(&k) {
                    occupied.insert(k);
                    out.insert(lid.clone(), (nx, ny));
                    break;
                }
            }
        }
        out
    }

    fn set_path(agent: &mut Agent, path_cells: &[(i32, i32)]) {
        if path_cells.is_empty() {
            return;
        }
        let cur = (agent.x, agent.y);
        let mut i = 0;
        if path_cells[0] == cur {
            i = 1;
        }
        agent.path = path_cells[i..].to_vec();
    }

    fn apply_round_update(&mut self, v: &Value) {
        let round = v
            .get("round")
            .and_then(|x| x.as_u64())
            .unwrap_or(self.round.saturating_add(1));
        self.round = round;

        let target = v
            .get("target")
            .or_else(|| v.get("region"))
            .and_then(|t| {
                let x = t.get("x")?.as_i64()? as i32;
                let y = t.get("y")?.as_i64()? as i32;
                Some(Self::clamp(x, y))
            });
        if let Some(t) = target {
            self.target = Some(t);
        }

        let claims: Vec<String> = v
            .get("claims")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        self.claims = claims.clone();

        let winner = v
            .get("winner")
            .and_then(|w| w.as_str())
            .map(String::from);
        self.winner_id = winner.clone();

        let Some((tx, ty)) = self.target else {
            return;
        };

        for id in &claims {
            self.get_or_create_agent(id);
        }

        if let Some(ref w) = winner {
            if claims.contains(w) {
                let losers: Vec<String> = claims.iter().filter(|id| *id != w).cloned().collect();
                let dests = self.allocate_loser_cells(tx, ty, &losers);

                {
                    let win = self.agents.get_mut(w).unwrap();
                    let p = Self::manhattan_path(win.x, win.y, tx, ty);
                    Self::set_path(win, &p);
                    win.rerouting = false;
                }

                for lid in &losers {
                    if let Some(a) = self.agents.get_mut(lid) {
                        if let Some(&(dx, dy)) = dests.get(lid) {
                            let p = Self::manhattan_path(a.x, a.y, dx, dy);
                            Self::set_path(a, &p);
                            a.rerouting = true;
                            self.push_event(format!("- {} lost → rerouting", lid));
                        }
                    }
                }

                for id in &claims {
                    if id == w || losers.contains(id) {
                        continue;
                    }
                    if let Some(a) = self.agents.get_mut(id) {
                        let p = Self::manhattan_path(a.x, a.y, tx, ty);
                        Self::set_path(a, &p);
                    }
                }
                return;
            }
        }

        for id in &claims {
            if let Some(a) = self.agents.get_mut(id) {
                let p = Self::manhattan_path(a.x, a.y, tx, ty);
                Self::set_path(a, &p);
                a.rerouting = false;
            }
        }
    }

    fn apply_agent_position(&mut self, v: &Value) {
        let Some(id) = v.get("agent_id").and_then(|x| x.as_str()) else {
            return;
        };
        let Some(x) = v.get("x").and_then(|x| x.as_i64()) else {
            return;
        };
        let Some(y) = v.get("y").and_then(|x| x.as_i64()) else {
            return;
        };
        let (cx, cy) = Self::clamp(x as i32, y as i32);
        let a = self.get_or_create_agent(id);
        a.x = cx;
        a.y = cy;
        a.path.clear();
        a.rerouting = false;
    }

    fn apply_named_event(&mut self, v: &Value) {
        let name = v.get("name").and_then(|x| x.as_str());
        match name {
            Some("reroute") => {
                if let Some(aid) = v.get("agent_id").and_then(|x| x.as_str()) {
                    let Some((tx, ty)) = self.target else {
                        return;
                    };
                    let mut occ: HashSet<String> = HashSet::new();
                    for (id, ag) in &self.agents {
                        if id != aid {
                            occ.insert(Self::cell_key(ag.x, ag.y));
                        }
                    }
                    let mut dest = None;
                    'outer: for d in 1i32..12 {
                        for dx in -d..=d {
                            for dy in -d..=d {
                                if dx.abs() + dy.abs() != d {
                                    continue;
                                }
                                let nx = tx + dx;
                                let ny = ty + dy;
                                if nx < 0 || nx >= GRID_W || ny < 0 || ny >= GRID_H {
                                    continue;
                                }
                                if nx == tx && ny == ty {
                                    continue;
                                }
                                let k = Self::cell_key(nx, ny);
                                if !occ.contains(&k) {
                                    dest = Some((nx, ny));
                                    break 'outer;
                                }
                            }
                        }
                    }
                    if let Some(a) = self.agents.get_mut(aid) {
                        let (dx, dy) = dest.unwrap_or_else(|| {
                            if a.x == tx {
                                Self::clamp(a.x + 1, a.y)
                            } else {
                                (tx, ty)
                            }
                        });
                        let p = Self::manhattan_path(a.x, a.y, dx, dy);
                        Self::set_path(a, &p);
                        a.rerouting = true;
                    }
                }
            }
            Some("owner_invalidated") => {
                self.winner_id = None;
                if let Some(arr) = v.get("region").and_then(|r| r.as_array()) {
                    if arr.len() >= 2 {
                        let x = arr[0].as_i64().unwrap_or(0) as i32;
                        let y = arr[1].as_i64().unwrap_or(0) as i32;
                        self.target = Some(Self::clamp(x, y));
                    }
                }
                self.push_event("- target owner invalidated — agents reconverge".to_string());
            }
            _ => {}
        }
    }

    fn apply_json_line(&mut self, line: &str) {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let ty = v.get("type").and_then(|x| x.as_str());
        match ty {
            Some("round_update") => {
                self.apply_round_update(&v);
                let w = self
                    .winner_id
                    .as_deref()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "—".to_string());
                self.push_event(format!("round {} winner {}", self.round, w));
            }
            Some("agent_position") => {
                let _ = self.apply_agent_position(&v);
            }
            Some("event") => self.apply_named_event(&v),
            _ => {}
        }
    }

    fn advance_paths_one_step(&mut self) {
        let ids: Vec<String> = self.agents.keys().cloned().collect();
        for id in ids {
            let Some(a) = self.agents.get_mut(&id) else {
                continue;
            };
            if a.path.is_empty() {
                continue;
            }
            let next = a.path.remove(0);
            a.x = next.0;
            a.y = next.1;
            if a.path.is_empty() && a.rerouting {
                a.rerouting = false;
            }
        }
    }

    fn all_paths_empty(&self) -> bool {
        self.agents.values().all(|a| a.path.is_empty())
    }

    fn demo_setup(&mut self) {
        self.agents.clear();
        let positions = [
            ("a1", 1, 1),
            ("a2", 8, 1),
            ("a3", 1, 8),
            ("a4", 8, 8),
            ("a5", 4, 2),
        ];
        for (id, x, y) in positions {
            self.agents.insert(
                id.to_string(),
                Agent {
                    id: id.to_string(),
                    x,
                    y,
                    path: Vec::new(),
                    rerouting: false,
                },
            );
        }
        self.target = Some((5, 5));
        self.winner_id = None;
        self.round = 42;
        self.claims = vec![
            "a1".into(),
            "a2".into(),
            "a3".into(),
            "a4".into(),
            "a5".into(),
        ];
        self.banner = "Phase 1 — Setup: 5 agents scattered · target F at (5,5)".into();
        self.event_log.clear();
        self.push_event("target appears at (5,5)".to_string());
        for id in &["a1", "a2", "a3", "a4", "a5"] {
            self.push_event(format!("- {} exploring", id));
        }
    }

    fn demo_start_convergence(&mut self) {
        let Some((tx, ty)) = self.target else {
            return;
        };
        self.banner = "Phase 2 — All agents move toward F (Manhattan)".into();
        for id in ["a1", "a2", "a3", "a4", "a5"] {
            if let Some(a) = self.agents.get_mut(id) {
                let p = Self::manhattan_path(a.x, a.y, tx, ty);
                Self::set_path(a, &p);
                a.rerouting = false;
            }
        }
    }

    fn demo_freeze(&mut self) {
        self.banner =
            "Phase 3 — Conflict: 5 agents claimed region · resolving…".into();
        self.push_event("5 agents claimed region".to_string());
        self.demo_wait_until = Some(Instant::now() + Duration::from_millis(500));
    }

    fn demo_resolve(&mut self) {
        let Some((tx, ty)) = self.target else {
            return;
        };
        self.winner_id = Some("a5".into());
        self.round = 1184356897;
        let claims = vec![
            "a1".into(),
            "a2".into(),
            "a3".into(),
            "a4".into(),
            "a5".into(),
        ];
        self.claims = claims.clone();
        self.banner = "Phase 4 — Winner: a5 · losers reroute".into();

        let w = "a5";
        let losers: Vec<String> = claims.iter().filter(|id| *id != w).cloned().collect();
        let dests = self.allocate_loser_cells(tx, ty, &losers);

        {
            let win = self.agents.get_mut(w).unwrap();
            let p = Self::manhattan_path(win.x, win.y, tx, ty);
            Self::set_path(win, &p);
            win.rerouting = false;
        }
        for lid in &losers {
            if let Some(a) = self.agents.get_mut(lid) {
                if let Some(&(dx, dy)) = dests.get(lid) {
                    let p = Self::manhattan_path(a.x, a.y, dx, dy);
                    Self::set_path(a, &p);
                    a.rerouting = true;
                    self.push_event(format!("- {} lost → rerouting", lid));
                }
            }
        }
        self.push_event("- a5 exploring".to_string());
    }

    fn demo_kill_winner(&mut self) {
        self.banner = "Phase 6 — Killing winner… cell freed".into();
        self.push_event("Killing winner…".to_string());
        self.winner_id = None;
        if let Some(a) = self.agents.remove("a5") {
            let _ = a;
        }
        self.push_event("winner removed — agents compete again".to_string());
    }

    fn demo_start_recovery(&mut self) {
        let Some((tx, ty)) = self.target else {
            return;
        };
        self.banner = "Phase 7 — Recovery: reconverge on F".into();
        for (id, ag) in self.agents.iter_mut() {
            if id == "a5" {
                continue;
            }
            let p = Self::manhattan_path(ag.x, ag.y, tx, ty);
            Self::set_path(ag, &p);
            ag.rerouting = false;
        }
        self.demo_wait_until = Some(Instant::now() + Duration::from_millis(400));
    }

    fn tick_demo(&mut self) {
        if self.demo_wait_until.is_some() {
            let u = self.demo_wait_until.unwrap();
            if Instant::now() < u {
                return;
            }
            self.demo_wait_until = None;
        }

        match self.demo_phase {
            DemoPhase::Off => {}
            DemoPhase::Setup => {
                self.demo_setup();
                self.demo_start_convergence();
                self.demo_phase = DemoPhase::Converge;
            }
            DemoPhase::Converge => {
                self.advance_paths_one_step();
                if self.all_paths_empty() {
                    self.demo_freeze();
                    self.demo_phase = DemoPhase::Freeze;
                }
            }
            DemoPhase::Freeze => {
                self.demo_resolve();
                self.demo_phase = DemoPhase::Resolve;
            }
            DemoPhase::Resolve => {
                self.advance_paths_one_step();
                if self.all_paths_empty() {
                    self.banner =
                        "Phase 5 — Stability: winner a5 on F · losers settled".into();
                    self.demo_phase = DemoPhase::Stability;
                    if self.args_auto_kill_secs > 0 {
                        self.auto_kill_at =
                            Some(Instant::now() + Duration::from_secs(self.args_auto_kill_secs));
                    }
                }
            }
            DemoPhase::Stability => {
                if let Some(t) = self.auto_kill_at {
                    if Instant::now() >= t {
                        self.auto_kill_at = None;
                        self.demo_kill_winner();
                        self.demo_start_recovery();
                        self.demo_phase = DemoPhase::Recovery;
                    }
                }
            }
            DemoPhase::Recovery => {
                self.advance_paths_one_step();
                if self.all_paths_empty() {
                    self.banner = "Recovery complete — press R to restart demo".into();
                    self.demo_phase = DemoPhase::Off;
                    self.push_event("new equilibrium (demo end)".to_string());
                }
            }
        }
    }

    fn kill_winner_key(&mut self) {
        if self.demo_phase == DemoPhase::Stability && self.winner_id.is_some() {
            self.auto_kill_at = None;
            self.demo_kill_winner();
            self.demo_start_recovery();
            self.demo_phase = DemoPhase::Recovery;
        } else if self.winner_id.is_some() {
            self.winner_id = None;
            self.push_event("winner cleared (live)".to_string());
        }
    }

    fn render_grid(&self) -> Text<'static> {
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(vec![
            Span::styled(
                format!("ROUND {}", self.round),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Grid:",
            Style::default().fg(Color::Gray),
        )]));

        let target = self.target;
        let w = self.winner_id.as_deref();

        for gy in 0..GRID_H {
            let mut spans: Vec<Span> = Vec::new();
            for gx in 0..GRID_W {
                let mut here: Vec<&str> = Vec::new();
                for (id, a) in &self.agents {
                    if a.x == gx && a.y == gy {
                        here.push(id.as_str());
                    }
                }
                let is_target = target == Some((gx, gy));
                let winner_here = w.is_some_and(|wid| here.contains(&wid));
                let any_reroute = here.iter().any(|id| {
                    self.agents
                        .get(*id)
                        .is_some_and(|a| a.rerouting || !a.path.is_empty())
                });

                let cell = if winner_here && is_target {
                    Span::styled(
                        " W ",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    )
                } else if !here.is_empty() && any_reroute && !winner_here {
                    Span::styled(" x ", Style::default().fg(Color::Red))
                } else if here.len() == 1 {
                    let id = here[0];
                    let style = if Some(id) == w {
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Cyan)
                    };
                    Span::styled(format!("{:^3}", id), style)
                } else if here.len() > 1 {
                    Span::styled(" A ", Style::default().fg(Color::Magenta))
                } else if is_target {
                    Span::styled(
                        " F ",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::styled(" . ", Style::default().fg(Color::DarkGray))
                };
                spans.push(cell);
            }
            lines.push(Line::from(spans));
        }

        Text::from(lines)
    }

    fn render_panel(&self) -> Text<'static> {
        let claims_s = if self.claims.is_empty() {
            "—".to_string()
        } else {
            self.claims.join(",")
        };
        let winner_display = self
            .winner_id
            .clone()
            .unwrap_or_else(|| "—".to_string());
        let mut lines: Vec<Line> = vec![
            Line::from(vec![Span::styled(
                "Event overlay",
                Style::default()
                    .fg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Round: ", Style::default().fg(Color::Gray)),
                Span::raw(format!("{}", self.round)),
            ]),
            Line::from(vec![
                Span::styled("Claims: ", Style::default().fg(Color::Gray)),
                Span::raw(claims_s),
            ]),
            Line::from(vec![
                Span::styled("Winner: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    winner_display,
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Events:",
                Style::default().fg(Color::Gray),
            )]),
        ];
        for e in &self.event_log {
            lines.push(Line::from(Span::raw(e.clone())));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "[K] kill winner  [R] restart demo  [Space] step  [S] slow/step toggle  +/- tick",
            Style::default().fg(Color::DarkGray),
        )]));
        Text::from(lines)
    }
}

fn spawn_stdin_thread(tx: mpsc::Sender<String>) {
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            if let Ok(l) = line {
                if tx.send(l).is_err() {
                    break;
                }
            }
        }
    });
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let (stdin_tx, stdin_rx) = mpsc::channel::<String>();
    if args.live {
        spawn_stdin_thread(stdin_tx);
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(&args);

    let res = run_loop(&mut terminal, &mut app, &args, stdin_rx);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    res
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    args: &Args,
    stdin_rx: mpsc::Receiver<String>,
) -> anyhow::Result<()> {
    let mut last_tick = if args.demo {
        Instant::now()
            .checked_sub(Duration::from_millis(app.interval_ms.saturating_add(1)))
            .unwrap_or_else(Instant::now)
    } else {
        Instant::now()
    };
    loop {
        if args.live {
            while let Ok(line) = stdin_rx.try_recv() {
                app.apply_json_line(&line.trim());
            }
        }

        terminal.draw(|f| {
            let area = f.area();
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(40), Constraint::Length(46)].as_ref())
                .split(area);

            let left = Block::default()
                .borders(Borders::ALL)
                .title(" vertex-hack · term-viz ");
            let inner_left = left.inner(chunks[0]);
            f.render_widget(left, chunks[0]);

            let mut text_lines: Vec<Line> = vec![];
            if !app.banner.is_empty() {
                text_lines.push(Line::from(vec![Span::styled(
                    app.banner.clone(),
                    Style::default().fg(Color::White),
                )]));
                text_lines.push(Line::from(""));
            }
            for line in app.render_grid().lines {
                text_lines.push(line);
            }
            let mode = match app.tick_mode {
                TickMode::Normal => format!("mode: normal  {}ms/tick", app.interval_ms),
                TickMode::Step => "mode: STEP (n/space)".into(),
            };
            text_lines.push(Line::from(""));
            text_lines.push(Line::from(Span::styled(
                mode,
                Style::default().fg(Color::DarkGray),
            )));

            let grid_par = Paragraph::new(Text::from(text_lines)).wrap(Wrap { trim: true });
            f.render_widget(grid_par, inner_left);

            let right = Block::default().borders(Borders::ALL).title(" judges ");
            let inner_right = right.inner(chunks[1]);
            f.render_widget(right, chunks[1]);
            let panel = Paragraph::new(app.render_panel()).wrap(Wrap { trim: true });
            f.render_widget(panel, inner_right);
        })?;

        let poll_ms = match app.tick_mode {
            TickMode::Normal => app.interval_ms.min(50).max(1),
            TickMode::Step => 250,
        };
        if event::poll(Duration::from_millis(poll_ms))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('k') | KeyCode::Char('K') => app.kill_winner_key(),
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        if args.demo {
                            app.demo_phase = DemoPhase::Setup;
                            app.demo_wait_until = None;
                            app.auto_kill_at = None;
                        }
                    }
                    KeyCode::Char(' ') | KeyCode::Char('n') | KeyCode::Enter => {
                        if app.tick_mode == TickMode::Step {
                            last_tick = Instant::now();
                            if args.demo && app.demo_phase != DemoPhase::Off {
                                app.tick_demo();
                            } else if !args.demo {
                                app.advance_paths_one_step();
                            }
                        }
                    }
                    KeyCode::Char('s') | KeyCode::Char('S') => {
                        app.tick_mode = match app.tick_mode {
                            TickMode::Normal => TickMode::Step,
                            TickMode::Step => TickMode::Normal,
                        };
                    }
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        app.interval_ms = (app.interval_ms + 100).min(5000);
                    }
                    KeyCode::Char('-') => {
                        app.interval_ms = app.interval_ms.saturating_sub(100).max(50);
                    }
                    _ => {}
                }
            }
        }

        if app.tick_mode == TickMode::Normal {
            let elapsed = last_tick.elapsed();
            let need = Duration::from_millis(app.interval_ms);
            if args.demo {
                if elapsed >= need {
                    last_tick = Instant::now();
                    app.tick_demo();
                }
            } else if elapsed >= need {
                last_tick = Instant::now();
                app.advance_paths_one_step();
            }
        }
    }
    Ok(())
}
