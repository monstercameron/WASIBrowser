//! gwb — the low-level Rust binding for the GWB ABI (docs/ABI.md).
//! Guest-side, wasm32-wasip1. Mirror of sdk/gwb (Go); binding #2.

pub const ROOT: u32 = 1;

// Well-known atoms (mirror of renderer/src/abi.rs).
pub mod atom {
    pub const DIV: u32 = 1;
    pub const SPAN: u32 = 2;
    pub const P: u32 = 3;
    pub const H1: u32 = 4;
    pub const H2: u32 = 5;
    pub const H3: u32 = 6;
    pub const BUTTON: u32 = 7;
    pub const INPUT: u32 = 8;

    pub const ATTR_CLASS: u32 = 100;
    pub const ATTR_ID: u32 = 101;
    pub const ATTR_STYLE: u32 = 102;
    pub const ATTR_VALUE: u32 = 104;
    pub const ATTR_TYPE: u32 = 105;
    pub const ATTR_PLACEHOLDER: u32 = 106;

    pub const STYLE_DISPLAY: u32 = 200;
    pub const STYLE_COLOR: u32 = 201;
    pub const STYLE_BACKGROUND: u32 = 202;
    pub const STYLE_WIDTH: u32 = 203;
    pub const STYLE_HEIGHT: u32 = 204;
    pub const STYLE_MARGIN: u32 = 205;
    pub const STYLE_PADDING: u32 = 206;
    pub const STYLE_FONT_SIZE: u32 = 208;
    pub const STYLE_GAP: u32 = 210;
    pub const STYLE_BORDER_RADIUS: u32 = 214;
    pub const STYLE_CURSOR: u32 = 215;
}

pub mod ev {
    pub const POINTER_DOWN: u16 = 1;
    pub const POINTER_UP: u16 = 2;
    pub const POINTER_MOVE: u16 = 3;
    pub const CLICK: u16 = 4;
    pub const DBLCLICK: u16 = 5;
    pub const POINTER_ENTER: u16 = 6;
    pub const POINTER_LEAVE: u16 = 7;
    pub const WHEEL: u16 = 8;
    pub const KEY_DOWN: u16 = 9;
    pub const KEY_UP: u16 = 10;
    pub const TEXT_INPUT: u16 = 11;
    pub const INPUT: u16 = 12;
    pub const FOCUS: u16 = 15;
    pub const BLUR: u16 = 16;
    pub const SCROLL: u16 = 17;
    pub const WINDOW_RESIZE: u16 = 18;
    pub const THEME_CHANGE: u16 = 19;
    pub const OBSERVED_LAYOUT: u16 = 32;
}

pub const LOG_DEBUG: u32 = 0;
pub const LOG_INFO: u32 = 1;
pub const LOG_WARN: u32 = 2;
pub const LOG_ERROR: u32 = 3;

const NO_STR: u32 = 0xFFFF_FFFF;

#[link(wasm_import_module = "gwb")]
unsafe extern "C" {
    fn submit(ptr: *const u8, len: u32) -> u32;
    fn event_region(ptr: *const u8, len: u32);
    fn log(level: u32, ptr: *const u8, len: u32);
    fn request_frame();
}

pub fn log_line(level: u32, msg: &str) {
    unsafe { log(level, msg.as_ptr(), msg.len() as u32) }
}

pub fn request_animation_frame() {
    unsafe { request_frame() }
}

// ---------------------------------------------------------------- ids + region

const EVENT_BUF_SIZE: usize = 8192;

struct EventBuf(std::cell::UnsafeCell<[u8; EVENT_BUF_SIZE]>);
// Safety: wasm guests are single-threaded; the host writes only while the
// guest is suspended in gwb_events.
unsafe impl Sync for EventBuf {}
static EVENT_BUF: EventBuf = EventBuf(std::cell::UnsafeCell::new([0; EVENT_BUF_SIZE]));

static NEXT_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1); // 1 is ROOT

pub fn new_id() -> u32 {
    NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}

/// Must be called once inside gwb_start.
pub fn register_event_region() {
    unsafe { event_region(EVENT_BUF.0.get() as *const u8, EVENT_BUF_SIZE as u32) }
}

// ---------------------------------------------------------------- batch

#[derive(Default)]
pub struct Batch {
    ops: Vec<u8>,
    heap: Vec<u8>,
    count: u32,
}

impl Batch {
    fn op(&mut self, code: u8, a: u16, b: u32, c: u32, d: u32) {
        self.ops.push(code);
        self.ops.push(0);
        self.ops.extend_from_slice(&a.to_le_bytes());
        self.ops.extend_from_slice(&b.to_le_bytes());
        self.ops.extend_from_slice(&c.to_le_bytes());
        self.ops.extend_from_slice(&d.to_le_bytes());
        self.count += 1;
    }

    fn str(&mut self, s: &str) -> u32 {
        let off = self.heap.len() as u32;
        self.heap.extend_from_slice(&(s.len() as u32).to_le_bytes());
        self.heap.extend_from_slice(s.as_bytes());
        while self.heap.len() % 4 != 0 {
            self.heap.push(0);
        }
        off
    }

    pub fn create_element(&mut self, id: u32, tag: u32) { self.op(1, 0, id, tag, NO_STR) }
    pub fn create_text(&mut self, id: u32, text: &str) { let s = self.str(text); self.op(2, 0, id, 0, s) }
    pub fn set_attr(&mut self, id: u32, name: u32, value: &str) { let s = self.str(value); self.op(3, 0, id, name, s) }
    pub fn remove_attr(&mut self, id: u32, name: u32) { self.op(4, 0, id, name, NO_STR) }
    pub fn set_text(&mut self, id: u32, text: &str) { let s = self.str(text); self.op(5, 0, id, 0, s) }
    pub fn set_style(&mut self, id: u32, prop: u32, value: &str) { let s = self.str(value); self.op(6, 0, id, prop, s) }
    pub fn remove_style(&mut self, id: u32, prop: u32) { self.op(7, 0, id, prop, NO_STR) }
    pub fn append_child(&mut self, parent: u32, child: u32) { self.op(8, 0, parent, child, NO_STR) }
    pub fn insert_before(&mut self, parent: u32, child: u32, before: u32) { self.op(9, 0, parent, child, before) }
    pub fn remove(&mut self, id: u32) { self.op(10, 0, id, 0, NO_STR) }
    pub fn replace_with(&mut self, old: u32, new: u32) { self.op(11, 0, old, new, NO_STR) }
    pub fn clear(&mut self, id: u32) { self.op(12, 0, id, 0, NO_STR) }
    pub fn set_inner_html(&mut self, id: u32, html: &str) { let s = self.str(html); self.op(13, 0, id, 0, s) }
    pub fn define_atom(&mut self, atom: u32, name: &str) { let s = self.str(name); self.op(14, 0, atom, 0, s) }
    pub fn listen(&mut self, id: u32, kind: u16) { self.op(15, kind, id, 0, NO_STR) }
    pub fn unlisten(&mut self, id: u32, kind: u16) { self.op(16, kind, id, 0, NO_STR) }
    pub fn observe(&mut self, id: u32, what: u32) { self.op(17, 0, id, what, NO_STR) }
    pub fn unobserve(&mut self, id: u32, what: u32) { self.op(18, 0, id, what, NO_STR) }
    pub fn focus(&mut self, id: u32) { self.op(19, 0, id, 0, NO_STR) }

    /// Encode + submit, then reset for reuse. Returns host status (0 = ok).
    pub fn submit(&mut self) -> u32 {
        if self.count == 0 {
            return 0;
        }
        let heap_off = 16 + self.ops.len() as u32;
        let mut buf = Vec::with_capacity(16 + self.ops.len() + self.heap.len());
        buf.extend_from_slice(b"GWB1");
        buf.extend_from_slice(&self.count.to_le_bytes());
        buf.extend_from_slice(&heap_off.to_le_bytes());
        buf.extend_from_slice(&(self.heap.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.ops);
        buf.extend_from_slice(&self.heap);
        let status = unsafe { submit(buf.as_ptr(), buf.len() as u32) };
        self.ops.clear();
        self.heap.clear();
        self.count = 0;
        status
    }
}

// ---------------------------------------------------------------- events

/// A decoded event record (see docs/ABI.md for per-kind fields).
#[derive(Default, Clone, Debug)]
pub struct Event {
    pub kind: u16,
    pub flags: u16,
    pub target: u32,
    pub listener: u32,
    pub time_ms: f64,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub dx: f32,
    pub dy: f32,
    pub scale: f32,
    pub buttons: u16,
    pub mods: u16,
    pub pressed: bool,
    pub dark: bool,
    pub text: String,
}

/// Decode `count` records from the event region, invoking `f` per record.
/// Returns the OR of all `f` return values (Ret* flags).
pub fn decode_events(count: u32, mut f: impl FnMut(&Event) -> u32) -> u32 {
    let buf: &[u8; EVENT_BUF_SIZE] = unsafe { &*EVENT_BUF.0.get() };
    let mut ret = 0u32;
    let mut off = 0usize;
    let u16at = |r: &[u8], o: usize| u16::from_le_bytes(r[o..o + 2].try_into().unwrap());
    let u32at = |r: &[u8], o: usize| u32::from_le_bytes(r[o..o + 4].try_into().unwrap());
    let f32at = |r: &[u8], o: usize| f32::from_le_bytes(r[o..o + 4].try_into().unwrap());
    for _ in 0..count {
        if off + 40 > buf.len() {
            break;
        }
        let r = &buf[off..];
        let mut e = Event {
            kind: u16at(r, 0),
            flags: u16at(r, 2),
            target: u32at(r, 4),
            listener: u32at(r, 8),
            time_ms: f64::from_le_bytes(r[12..20].try_into().unwrap()),
            ..Default::default()
        };
        match e.kind {
            ev::POINTER_DOWN | ev::POINTER_UP | ev::POINTER_MOVE | ev::CLICK | ev::DBLCLICK
            | ev::POINTER_ENTER | ev::POINTER_LEAVE => {
                e.x = f32at(r, 20);
                e.y = f32at(r, 24);
                e.buttons = u16at(r, 28);
                e.mods = u16at(r, 30);
            }
            ev::WHEEL => {
                e.dx = f32at(r, 20);
                e.dy = f32at(r, 24);
                e.mods = u16at(r, 28);
            }
            ev::KEY_DOWN | ev::KEY_UP | ev::TEXT_INPUT => {
                e.mods = u16at(r, 22);
                e.pressed = r[24] != 0;
            }
            ev::SCROLL => {
                e.x = f32at(r, 20);
                e.y = f32at(r, 24);
            }
            ev::WINDOW_RESIZE => {
                e.w = f32at(r, 20);
                e.h = f32at(r, 24);
                e.scale = f32at(r, 28);
            }
            ev::THEME_CHANGE => e.dark = u32at(r, 20) != 0,
            ev::OBSERVED_LAYOUT => {
                e.x = f32at(r, 20);
                e.y = f32at(r, 24);
                e.w = f32at(r, 28);
                e.h = f32at(r, 32);
            }
            _ => {}
        }
        let str_len = u32at(r, 36) as usize;
        let mut next = off + 40;
        if str_len > 0 && next + str_len <= buf.len() {
            e.text = String::from_utf8_lossy(&buf[next..next + str_len]).into_owned();
            next += str_len;
            next = (next + 3) & !3;
        }
        off = next;
        ret |= f(&e);
    }
    ret
}
