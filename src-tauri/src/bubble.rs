//! 划词悬浮按钮：跨平台的判断逻辑（mac / Windows 共用）。
//!
//! 平台层（mac_select / win_select）只负责：装全局鼠标钩子、把鼠标事件喂给 [`on_mouse`]、
//! 按 [`Backend`] 的要求显示 / 隐藏按钮窗口。这里决定「刚才是不是在选文字」「点没点中按钮」。
//!
//! 硬约束：这里**只看鼠标坐标和按键，不读取任何文字**；用户点了按钮才去取选中的内容。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 长条按钮的尺寸（逻辑像素）：五个功能等宽排开。
pub const WIDTH: f64 = 230.0;
pub const HEIGHT: f64 = 24.0;
/// 长条上的功能数，顺序和前端 Bubble.tsx 一致：翻译 / 总结 / 改写 / 回复 / 朗读。
pub const SEGMENTS: usize = 5;
/// 按钮顶边离光标的距离（逻辑像素）：选字时光标是 I 形，热点在中间，下半截约 8~9 像素。
pub const GAP: f64 = 12.0;
/// 拖动多远算「拖选」（逻辑像素）。
const DRAG_MIN: f64 = 6.0;
/// 两次点击离多近算双击（逻辑像素）。
const DOUBLE_DIST: f64 = 5.0;
/// 按钮自动消失的时间；鼠标停在按钮上时不消失。
const AUTO_HIDE: Duration = Duration::from_secs(5);
/// 松开鼠标后等多久再冒按钮：让目标程序先把选区处理完。
const SHOW_DELAY: Duration = Duration::from_millis(120);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    fn contains(&self, p: Pt) -> bool {
        p.x >= self.x && p.x < self.x + self.w && p.y >= self.y && p.y < self.y + self.h
    }

    /// 点在长条的第几段（0 起）。
    fn segment(&self, p: Pt) -> usize {
        let i = ((p.x - self.x) / self.w * SEGMENTS as f64).floor();
        (i.max(0.0) as usize).min(SEGMENTS - 1)
    }
}

/// 把 `w × h` 的窗口放在光标正下方、水平中点对齐光标。`scale` = 物理像素 / 逻辑像素。
pub fn below_cursor(cursor: Pt, w: f64, h: f64, scale: f64) -> Rect {
    Rect { x: cursor.x - w / 2.0, y: cursor.y + GAP * scale, w, h }
}

pub enum Mouse {
    LeftDown(Pt),
    /// `clicks`：系统给的连击次数（mac 有；Windows 没有，传 None 时按时间和距离自己判断）。
    LeftUp { pt: Pt, clicks: Option<i64> },
    /// 右键 / 中键按下、滚轮：收起按钮。
    Other,
    /// 鼠标移动：只用来判断是否停在按钮上（停着就不自动消失）。
    Move(Pt),
}

#[derive(Debug, PartialEq)]
enum Action {
    Pass,
    /// 吞掉这次事件（不传给任何程序）。
    Swallow,
    /// 吞掉，并且触发「点了按钮的第几段」。
    Click(usize),
    Hide,
    Show(Pt),
}

struct Detector {
    down: Option<Pt>,
    last_up: Option<(Pt, Instant)>,
    bubble: Option<Rect>,
    swallow_up: bool,
    hovering: bool,
}

impl Detector {
    const fn new() -> Self {
        Self { down: None, last_up: None, bubble: None, swallow_up: false, hovering: false }
    }

    fn handle(&mut self, ev: Mouse, now: Instant, cfg: &Config, ours: impl FnOnce() -> bool) -> Action {
        match ev {
            Mouse::LeftDown(p) => {
                if let Some(r) = self.bubble.take() {
                    if r.contains(p) {
                        self.swallow_up = true;
                        return Action::Click(r.segment(p));
                    }
                    self.down = Some(p);
                    return Action::Hide;
                }
                self.down = Some(p);
                Action::Pass
            }
            Mouse::LeftUp { pt, clicks } => {
                if std::mem::take(&mut self.swallow_up) {
                    return Action::Swallow;
                }
                let s = cfg.scale;
                let near = |a: Pt, b: Pt, d: f64| (a.x - b.x).hypot(a.y - b.y) < d * s;
                let dragged = self.down.is_some_and(|d| !near(d, pt, DRAG_MIN));
                let double = match clicks {
                    Some(n) => n >= 2,
                    None => self.last_up.is_some_and(|(u, t)| {
                        now.duration_since(t) <= cfg.double_click && near(u, pt, DOUBLE_DIST)
                    }),
                };
                self.down = None;
                self.last_up = Some((pt, now));
                if (dragged || double) && !ours() { Action::Show(pt) } else { Action::Pass }
            }
            Mouse::Other => {
                if self.bubble.take().is_some() { Action::Hide } else { Action::Pass }
            }
            Mouse::Move(p) => {
                self.hovering = self.bubble.is_some_and(|r| r.contains(p));
                Action::Pass
            }
        }
    }
}

pub struct Config {
    /// 坐标单位：mac 是逻辑点（1.0），Windows 是物理像素（DPI / 96）。只用于距离阈值。
    pub scale: f64,
    pub double_click: Duration,
}

/// 平台层实现。除 `foreground_is_ours` 外都在后台线程调用。
pub trait Backend: Send + Sync + 'static {
    /// 在光标 `cursor` 下方显示按钮，返回按钮实际占的矩形（和鼠标事件同一坐标系）。
    fn show(&self, cursor: Pt) -> Rect;
    fn hide(&self);
    /// 前台是不是我们自己的窗口（面板 / 设置）——是就不冒按钮。
    fn foreground_is_ours(&self) -> bool;
    /// 用户点了长条的第 `index` 段：去取选中文字，弹面板并打开对应功能。
    fn clicked(&self, index: usize);
}

static BACKEND: OnceLock<(Box<dyn Backend>, Config)> = OnceLock::new();
static STATE: Mutex<Detector> = Mutex::new(Detector::new());
/// 每次显示按钮自增；自动隐藏只对自己那一次生效。
static SEQ: AtomicU64 = AtomicU64::new(0);

pub fn install(backend: impl Backend, cfg: Config) {
    let _ = BACKEND.set((Box::new(backend), cfg));
}

/// 平台钩子里调用，要快（钩子超时会被系统摘掉）。返回 true = 吞掉这次事件。
pub fn on_mouse(ev: Mouse) -> bool {
    let Some((backend, cfg)) = BACKEND.get() else { return false };
    if !matches!(ev, Mouse::LeftUp { .. } | Mouse::Move(_)) {
        // 任何按下 / 滚轮都作废还没冒出来、或正在倒计时的按钮。
        SEQ.fetch_add(1, Ordering::Relaxed);
    }
    let action = STATE.lock().unwrap().handle(ev, Instant::now(), cfg, || backend.foreground_is_ours());
    match action {
        Action::Pass => false,
        Action::Swallow => true,
        Action::Click(i) => {
            std::thread::spawn(move || {
                let (b, _) = BACKEND.get().unwrap();
                b.hide();
                b.clicked(i);
            });
            true
        }
        Action::Hide => {
            std::thread::spawn(|| BACKEND.get().unwrap().0.hide());
            false
        }
        Action::Show(pt) => {
            let seq = SEQ.fetch_add(1, Ordering::Relaxed) + 1;
            std::thread::spawn(move || {
                let (b, _) = BACKEND.get().unwrap();
                std::thread::sleep(SHOW_DELAY);
                if SEQ.load(Ordering::Relaxed) != seq {
                    return; // 期间又点了别处
                }
                let r = b.show(pt);
                {
                    let mut st = STATE.lock().unwrap();
                    st.bubble = Some(r);
                    st.hovering = false;
                }
                std::thread::sleep(AUTO_HIDE);
                loop {
                    if SEQ.load(Ordering::Relaxed) != seq {
                        return;
                    }
                    let mut st = STATE.lock().unwrap();
                    if st.hovering {
                        drop(st);
                        std::thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                    if st.bubble.take().is_some() {
                        drop(st);
                        b.hide();
                    }
                    return;
                }
            });
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CFG: Config = Config { scale: 1.0, double_click: Duration::from_millis(500) };
    fn p(x: f64, y: f64) -> Pt {
        Pt { x, y }
    }
    fn up(d: &mut Detector, pt: Pt, t: Instant) -> Action {
        d.handle(Mouse::LeftUp { pt, clicks: None }, t, &CFG, || false)
    }

    #[test]
    fn segments_split_evenly() {
        let r = Rect { x: 100.0, y: 0.0, w: 250.0, h: 30.0 };
        assert_eq!(r.segment(p(100.0, 10.0)), 0);
        assert_eq!(r.segment(p(149.0, 10.0)), 0);
        assert_eq!(r.segment(p(150.0, 10.0)), 1);
        assert_eq!(r.segment(p(260.0, 10.0)), 3);
        assert_eq!(r.segment(p(349.9, 10.0)), 4);
        let t = Instant::now();
        let mut d = Detector::new();
        d.bubble = Some(r);
        assert_eq!(d.handle(Mouse::LeftDown(p(320.0, 10.0)), t, &CFG, || false), Action::Click(4));
    }

    #[test]
    fn hover_tracking() {
        let t = Instant::now();
        let mut d = Detector::new();
        d.bubble = Some(Rect { x: 0.0, y: 0.0, w: 100.0, h: 30.0 });
        d.handle(Mouse::Move(p(50.0, 10.0)), t, &CFG, || false);
        assert!(d.hovering);
        d.handle(Mouse::Move(p(500.0, 10.0)), t, &CFG, || false);
        assert!(!d.hovering);
    }

    #[test]
    fn placement_centered_below() {
        let r = below_cursor(p(100.0, 200.0), 32.0, 32.0, 1.0);
        assert_eq!(r.x + r.w / 2.0, 100.0);
        assert_eq!(r.y, 200.0 + GAP);
        let r2 = below_cursor(p(100.0, 200.0), 64.0, 64.0, 2.0);
        assert_eq!(r2.x + r2.w / 2.0, 100.0);
        assert_eq!(r2.y, 200.0 + GAP * 2.0);
    }

    #[test]
    fn drag_shows_click_does_not() {
        let t = Instant::now();
        let mut d = Detector::new();
        d.handle(Mouse::LeftDown(p(0.0, 0.0)), t, &CFG, || false);
        assert_eq!(up(&mut d, p(2.0, 0.0), t), Action::Pass);
        d.handle(Mouse::LeftDown(p(500.0, 0.0)), t + Duration::from_secs(2), &CFG, || false);
        assert_eq!(up(&mut d, p(600.0, 0.0), t + Duration::from_secs(2)), Action::Show(p(600.0, 0.0)));
    }

    #[test]
    fn double_click_by_timing_or_count() {
        let t = Instant::now();
        let mut d = Detector::new();
        d.handle(Mouse::LeftDown(p(0.0, 0.0)), t, &CFG, || false);
        up(&mut d, p(0.0, 0.0), t);
        d.handle(Mouse::LeftDown(p(1.0, 0.0)), t, &CFG, || false);
        assert_eq!(up(&mut d, p(1.0, 0.0), t + Duration::from_millis(200)), Action::Show(p(1.0, 0.0)));

        let mut d = Detector::new();
        d.handle(Mouse::LeftDown(p(0.0, 0.0)), t, &CFG, || false);
        let a = d.handle(Mouse::LeftUp { pt: p(0.0, 0.0), clicks: Some(2) }, t, &CFG, || false);
        assert_eq!(a, Action::Show(p(0.0, 0.0)));
    }

    #[test]
    fn not_over_our_own_windows() {
        let t = Instant::now();
        let mut d = Detector::new();
        d.handle(Mouse::LeftDown(p(0.0, 0.0)), t, &CFG, || false);
        let a = d.handle(Mouse::LeftUp { pt: p(100.0, 0.0), clicks: None }, t, &CFG, || true);
        assert_eq!(a, Action::Pass);
    }

    #[test]
    fn clicking_bubble_is_swallowed_elsewhere_hides() {
        let t = Instant::now();
        let mut d = Detector::new();
        d.bubble = Some(Rect { x: 10.0, y: 10.0, w: 32.0, h: 32.0 });
        assert_eq!(d.handle(Mouse::LeftDown(p(20.0, 20.0)), t, &CFG, || false), Action::Click(1));
        assert_eq!(up(&mut d, p(20.0, 20.0), t), Action::Swallow);
        assert!(d.bubble.is_none());

        d.bubble = Some(Rect { x: 10.0, y: 10.0, w: 32.0, h: 32.0 });
        assert_eq!(d.handle(Mouse::LeftDown(p(200.0, 200.0)), t, &CFG, || false), Action::Hide);
        d.bubble = Some(Rect { x: 10.0, y: 10.0, w: 32.0, h: 32.0 });
        assert_eq!(d.handle(Mouse::Other, t, &CFG, || false), Action::Hide);
    }
}
