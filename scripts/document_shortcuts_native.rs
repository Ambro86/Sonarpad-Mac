use std::cell::{Cell, RefCell};
use std::rc::Rc;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType};
use objc2_foundation::{NSPoint, NSString};
use wxdragon::prelude::*;
use wxdragon::event::KeyboardEvent;
use wxdragon::timer::Timer;
#[path = "DOCUMENT_DECK_PATH"] mod document_deck;
mod documents {
    pub const ID_NEXT_DOCUMENT: i32 = 2014;
    pub const ID_PREVIOUS_DOCUMENT: i32 = 2015;
}
struct ShortcutActions { document: Rc<dyn Fn(i32)> }
fn command_shortcut_down(key: &KeyboardEvent) -> bool { key.cmd_down() }
fn append_podcast_log(message: &str) { println!("{message}"); }
// PRODUCTION_NATIVE_FALLBACK
fn keyboard(event: WindowEventData, actions: &ShortcutActions) {
    if let WindowEventData::Keyboard(key_event) = &event {
        println!("key={:?} unicode={:?} cmd={} shift={}", key_event.get_key_code(), key_event.get_unicode_key(), key_event.cmd_down(), key_event.shift_down());
        // PRODUCTION_HANDLER
    }
    event.skip(true);
}
fn main() {
    wxdragon::main(|_app| {
        let frame = Frame::builder().with_title("Sonarpad document shortcut test").with_size(Size::new(600, 300)).build();
        let menu = Menu::builder().build();
        menu.append(documents::ID_NEXT_DOCUMENT, "Next\tNEXT_ACCELERATOR", "", ItemKind::Normal);
        menu.append(documents::ID_PREVIOUS_DOCUMENT, "Previous\tPREVIOUS_ACCELERATOR", "", ItemKind::Normal);
        frame.set_menu_bar(MenuBar::builder().append(menu, "Window").build());
        let panel = Panel::builder(&frame).build();
        let editor = TextCtrl::builder(&panel).with_style(TextCtrlStyle::MultiLine).build();
        let deck = Rc::new(RefCell::new(document_deck::DocumentDeck::<String>::default()));
        deck.borrow_mut().items = vec!["first".into(), "second".into(), "third".into()];
        editor.set_value("first");
        let calls = Rc::new(Cell::new(0));
        let deck_action = deck.clone();
        let calls_action = calls.clone();
        let action: Rc<dyn Fn(i32)> = Rc::new(move |id| {
            if !matches!(id, documents::ID_NEXT_DOCUMENT | documents::ID_PREVIOUS_DOCUMENT) { return; }
            calls_action.set(calls_action.get() + 1);
            let mut deck = deck_action.borrow_mut();
            deck.step(id == documents::ID_PREVIOUS_DOCUMENT);
            editor.set_value(&deck.items[deck.active]);
            println!("document command={id} active={}", deck.active);
        });
        let menu_action = action.clone();
        frame.on_menu(move |event| menu_action(event.get_id()));
        let editor_action = action.clone();
        editor.on_key_down(move |event| keyboard(event, &ShortcutActions {document: editor_action.clone()}));
        frame.on_key_down(move |event| keyboard(event, &ShortcutActions {document: action.clone()}));
        frame.show(true);
        editor.set_focus();
        let timer = Timer::new(&frame);
        let phase = Cell::new(0usize);
        let failures = Cell::new(0);
        timer.on_tick(move |_| {
            let key = |name: &str| std::env::var(format!("SHORTCUT_TEST_{name}_KEY")).unwrap().parse::<u16>().unwrap();
            let shift = |name: &str| std::env::var(format!("SHORTCUT_TEST_{name}_SHIFT")).unwrap() == "1";
            let cases = [("next", "<", key("NEXT"), shift("NEXT"), 1),
                         ("previous", ">", key("PREVIOUS"), shift("PREVIOUS"), 2),
                         ("native character fallback next", "<", 10, false, 1),
                         ("native character fallback previous", ">", 10, true, 2)];
            let step = phase.get();
            phase.set(step + 1);
            let index = step / 2;
            if index >= cases.len() { std::process::exit(if failures.get() == 0 {0} else {1}); }
            let (name, character, key, shift, expected) = cases[index];
            if step % 2 == 1 {
                let passed = calls.get() == 1 && deck.borrow().active == expected;
                println!("{name}: {} calls={} document={}", if passed {"PASS"} else {"FAIL"}, calls.get(), deck.borrow().active);
                if !passed { failures.set(failures.get() + 1); }
                return;
            }
            calls.set(0);
            deck.borrow_mut().active = 0;
            editor.set_value("first");
            editor.set_focus();
            let app = NSApplication::sharedApplication(MainThreadMarker::new().unwrap());
            let window = app.keyWindow().expect("Test must have a native key window");
            let mut flags = NSEventModifierFlags::Command;
            if shift { flags |= NSEventModifierFlags::Shift; }
            let text = NSString::from_str(character);
            let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                NSEventType::KeyDown, NSPoint::new(0.0, 0.0), flags, 0.0, window.windowNumber(), None, &text, &text, false, key).unwrap();
            println!("Sending {name}");
            app.postEvent_atStart(&event, false);
        });
        timer.start(500, false);
        std::mem::forget(timer);
    });
}
