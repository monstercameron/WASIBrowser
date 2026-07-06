//! todo-rs: the tri-language Todo spec (examples/TODO_SPEC.md) in Rust.

use std::cell::RefCell;
use std::collections::HashMap;

use gwb::{Batch, Event, atom, ev, new_id};

const ATOM_TEXT_DECORATION: u32 = 1024;

#[derive(Default)]
struct Todo {
    label: u32,
    text: u32,
    done: bool,
}

#[derive(Default)]
struct App {
    batch: Batch,
    todos: HashMap<u32, Todo>, // keyed by item row id
    by_label: HashMap<u32, u32>,
    by_x: HashMap<u32, u32>,
    list: u32,
    status_text: u32,
    input: u32,
    add_btn: u32,
    hundred_btn: u32,
    input_value: String,
    created: u32,
    done_count: u32,
}

thread_local! {
    static APP: RefCell<App> = RefCell::new(App::default());
}

impl App {
    fn text_element(&mut self, tag: u32, text: &str) -> u32 {
        let el = new_id();
        self.batch.create_element(el, tag);
        let t = new_id();
        self.batch.create_text(t, text);
        self.batch.append_child(el, t);
        el
    }

    fn add_todo(&mut self, text: &str) {
        self.created += 1;
        let text = if text.is_empty() {
            format!("Item {}", self.created)
        } else {
            text.to_string()
        };
        let row = new_id();
        self.batch.create_element(row, atom::DIV);
        self.batch.set_style(row, atom::STYLE_DISPLAY, "flex");
        self.batch.set_style(row, atom::STYLE_GAP, "8px");
        self.batch.set_style(row, atom::STYLE_MARGIN, "0 0 6px 0");

        let label = new_id();
        self.batch.create_element(label, atom::SPAN);
        self.batch.set_style(label, atom::STYLE_CURSOR, "pointer");
        let label_text = new_id();
        self.batch.create_text(label_text, &text);
        self.batch.append_child(label, label_text);
        self.batch.append_child(row, label);

        let x = self.text_element(atom::BUTTON, "x");
        self.batch.append_child(row, x);

        self.batch.append_child(self.list, row);
        self.batch.listen(label, ev::CLICK);
        self.batch.listen(x, ev::CLICK);

        self.todos.insert(row, Todo { label, text: label_text, done: false });
        self.by_label.insert(label, row);
        self.by_x.insert(x, row);
    }

    fn update_status(&mut self) {
        let text = format!("{} items, {} done", self.todos.len(), self.done_count);
        self.batch.set_text(self.status_text, &text);
    }

    fn start(&mut self) {
        self.batch.define_atom(ATOM_TEXT_DECORATION, "text-decoration");

        let card = new_id();
        self.batch.create_element(card, atom::DIV);
        self.batch.set_style(card, atom::STYLE_PADDING, "20px");
        self.batch.set_style(card, atom::STYLE_BACKGROUND, "#26282c");
        self.batch.set_style(card, atom::STYLE_BORDER_RADIUS, "10px");
        self.batch.set_style(card, atom::STYLE_WIDTH, "420px");
        self.batch.append_child(gwb::ROOT, card);

        let heading = self.text_element(atom::H2, "Todos — Rust");
        self.batch.set_style(heading, atom::STYLE_MARGIN, "0 0 12px 0");
        self.batch.append_child(card, heading);

        let row = new_id();
        self.batch.create_element(row, atom::DIV);
        self.batch.set_style(row, atom::STYLE_DISPLAY, "flex");
        self.batch.set_style(row, atom::STYLE_GAP, "8px");
        self.batch.set_style(row, atom::STYLE_MARGIN, "0 0 14px 0");
        self.batch.append_child(card, row);

        self.input = new_id();
        self.batch.create_element(self.input, atom::INPUT);
        self.batch.set_attr(self.input, atom::ATTR_TYPE, "text");
        self.batch.set_attr(self.input, atom::ATTR_PLACEHOLDER, "What needs doing?");
        self.batch.set_style(self.input, atom::STYLE_WIDTH, "240px");
        self.batch.append_child(row, self.input);
        self.batch.listen(self.input, ev::INPUT);

        self.add_btn = self.text_element(atom::BUTTON, "Add");
        self.batch.append_child(row, self.add_btn);
        self.batch.listen(self.add_btn, ev::CLICK);

        self.hundred_btn = self.text_element(atom::BUTTON, "+100");
        self.batch.append_child(row, self.hundred_btn);
        self.batch.listen(self.hundred_btn, ev::CLICK);

        self.list = new_id();
        self.batch.create_element(self.list, atom::DIV);
        self.batch.append_child(card, self.list);

        let status = new_id();
        self.batch.create_element(status, atom::P);
        self.status_text = new_id();
        self.batch.create_text(self.status_text, "0 items, 0 done");
        self.batch.append_child(status, self.status_text);
        self.batch.set_style(status, atom::STYLE_MARGIN, "10px 0 0 0");
        self.batch.set_style(status, atom::STYLE_FONT_SIZE, "12px");
        self.batch.set_style(status, atom::STYLE_COLOR, "#9a9fa6");
        self.batch.append_child(card, status);
    }

    fn on_event(&mut self, e: &Event) -> u32 {
        match e.kind {
            ev::INPUT if e.listener == self.input => {
                self.input_value = e.text.clone();
            }
            ev::CLICK if e.listener == self.add_btn => {
                let value = std::mem::take(&mut self.input_value);
                self.add_todo(&value);
                self.batch.set_attr(self.input, atom::ATTR_VALUE, "");
                self.update_status();
            }
            ev::CLICK if e.listener == self.hundred_btn => {
                let start = std::time::Instant::now();
                for _ in 0..100 {
                    self.add_todo("");
                }
                self.update_status();
                gwb::log_line(
                    gwb::LOG_INFO,
                    &format!("+100 encoded in {:.2}ms (guest)", start.elapsed().as_secs_f64() * 1000.0),
                );
            }
            ev::CLICK => {
                if let Some(&row) = self.by_label.get(&e.listener) {
                    let todo = self.todos.get_mut(&row).unwrap();
                    todo.done = !todo.done;
                    let label = todo.label;
                    if todo.done {
                        self.done_count += 1;
                        self.batch.set_style(label, atom::STYLE_COLOR, "#9a9fa6");
                        self.batch.set_style(label, ATOM_TEXT_DECORATION, "line-through");
                    } else {
                        self.done_count -= 1;
                        self.batch.set_style(label, atom::STYLE_COLOR, "#e8e8e8");
                        self.batch.remove_style(label, ATOM_TEXT_DECORATION);
                    }
                    self.update_status();
                } else if let Some(&row) = self.by_x.get(&e.listener) {
                    if let Some(todo) = self.todos.remove(&row) {
                        if todo.done {
                            self.done_count -= 1;
                        }
                        self.by_label.remove(&todo.label);
                    }
                    self.by_x.remove(&e.listener);
                    self.batch.remove(row);
                    self.update_status();
                }
            }
            _ => {}
        }
        0
    }
}

// ---------------------------------------------------------------- exports

#[unsafe(no_mangle)]
extern "C" fn gwb_abi_version() -> u32 {
    1 << 16
}

#[unsafe(no_mangle)]
extern "C" fn gwb_start(_w: f32, _h: f32, _scale: f32, _flags: u32) {
    gwb::register_event_region();
    APP.with_borrow_mut(|app| {
        app.start();
        app.batch.submit();
    });
}

#[unsafe(no_mangle)]
extern "C" fn gwb_events(count: u32) -> u32 {
    APP.with_borrow_mut(|app| {
        let ret = gwb::decode_events(count, |e| app.on_event(e));
        app.batch.submit();
        ret
    })
}

#[unsafe(no_mangle)]
extern "C" fn gwb_frame(_dt_ms: f32) {}
