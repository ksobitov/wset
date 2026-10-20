//! Reading the keyboard repeat settings the compositor actually uses.
//!
//! Every Wayland client is told them through wl_keyboard.repeat_info, so
//! this works the same on every compositor.

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_keyboard, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};

/// Auto-repeat as the compositor reports it; a rate of 0 means it's off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Repeat {
    /// Milliseconds before a held key starts repeating.
    pub delay: i32,
    /// Repeats per second.
    pub rate: i32,
}

#[derive(Default)]
struct State {
    keyboard: Option<wl_keyboard::WlKeyboard>,
    repeat: Option<Repeat>,
}

/// Returns None when the seat has no keyboard.
pub fn repeat() -> Result<Option<Repeat>, String> {
    let conn = Connection::connect_to_env()
        .map_err(|e| format!("can't connect to the Wayland display: {e}"))?;
    let (globals, mut queue) =
        registry_queue_init::<State>(&conn).map_err(|e| format!("Wayland: {e}"))?;
    let qh = queue.handle();
    // repeat_info arrived in version 4.
    let _seat: wl_seat::WlSeat =
        globals.bind(&qh, 4..=9, ()).map_err(|_| "the compositor has no wl_seat 4")?;

    let mut state = State::default();
    // The first roundtrip gets the seat's capabilities and with them the
    // keyboard, the second its repeat_info.
    for _ in 0..2 {
        queue.roundtrip(&mut state).map_err(|e| format!("Wayland: {e}"))?;
    }
    Ok(state.repeat)
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities: WEnum::Value(caps) } = event
            && caps.contains(wl_seat::Capability::Keyboard)
            && state.keyboard.is_none()
        {
            state.keyboard = Some(seat.get_keyboard(qh, ()));
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::RepeatInfo { rate, delay } = event {
            state.repeat = Some(Repeat { delay, rate });
        }
    }
}
