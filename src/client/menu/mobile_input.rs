//! Wasm-only: a permanently-attached, invisible, off-screen proxy `<input>`
//! element that stands in for the login/register text fields. A `<canvas>`
//! can never trigger a mobile on-screen keyboard and has nothing for the
//! browser to paste into — only a real, DOM-focused, text-editing element
//! can do either. This module owns that one element and its event wiring;
//! it knows nothing about `LoginForm`/Bevy — callers in `account.rs` drain
//! its events each frame and decide what to do with them.
//!
//! Mirrors `crates/fc-net/src/ws.rs`'s JS-interop idiom: a `thread_local!`
//! registry, `wasm_bindgen::closure::Closure`, and `web_sys`'s typed event
//! setters (wasm32-unknown-unknown is single-threaded, so a thread-local is
//! always the same "thread").

use std::cell::RefCell;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{FocusEvent, HtmlInputElement, InputEvent, KeyboardEvent};

/// One frame's worth of proxy-sourced input, drained by `account.rs`'s
/// `drain_mobile_input` system.
#[derive(Default)]
pub(crate) struct ProxyEvents {
    /// The proxy's full current text, if it changed since the last drain —
    /// typing, paste, IME commit and autofill all land here as one value
    /// (not a per-keystroke delta; a paste can touch more than one
    /// character at once, so the DOM is the source of truth for the string
    /// while the proxy holds focus).
    pub text: Option<String>,
    /// A non-repeat Enter keydown on the proxy.
    pub submit: bool,
    /// A non-repeat Escape keydown on the proxy.
    pub escape: bool,
    /// A Tab keydown on the proxy.
    pub tab: bool,
    /// The proxy lost DOM focus for a reason this module didn't already
    /// know about (user tapped elsewhere, dismissed the on-screen
    /// keyboard, ...) — see `blur_proxy`'s doc comment for how Rust- vs
    /// DOM-initiated blurs stay in sync without needing to tell them apart.
    pub lost_focus: bool,
}

struct Proxy {
    el: HtmlInputElement,
    /// Keeps the JS event callbacks alive for the life of the page.
    _on_input: Closure<dyn FnMut(InputEvent)>,
    _on_keydown: Closure<dyn FnMut(KeyboardEvent)>,
    _on_blur: Closure<dyn FnMut(FocusEvent)>,
    pending: RefCell<ProxyEvents>,
}

thread_local! {
    /// A single proxy for the whole page's life — there's only ever one
    /// account form on screen at a time. Created lazily on first
    /// `focus_field`, then reused (never recreated) for every later focus.
    static PROXY: RefCell<Option<Proxy>> = const { RefCell::new(None) };
}

/// Longest login/password/name field — mirrors `account.rs::MAX_FIELD_LEN`
/// (kept as a plain literal here rather than importing it, so this module
/// stays free of any `account.rs` dependency; the two are asserted equal by
/// nothing more than code review, same as any other duplicated constant
/// would be — acceptable since a mismatch would only ever make the browser
/// enforce a slightly different cap than the Rust-side backstop, not a
/// correctness bug).
const MAX_FIELD_LEN: &str = "32";

fn create_proxy() -> Proxy {
    let window = web_sys::window().expect("wasm always has a window");
    let document = window.document().expect("wasm always has a document");
    let el: HtmlInputElement = document
        .create_element("input")
        .expect("creating an <input> element never fails")
        .dyn_into()
        .expect("create_element(\"input\") always yields an HtmlInputElement");

    let _ = el.set_attribute("id", "fc-text-proxy");
    let _ = el.set_attribute("autocapitalize", "off");
    let _ = el.set_attribute("autocorrect", "off");
    let _ = el.set_attribute("spellcheck", "false");
    let _ = el.set_attribute("tabindex", "-1");
    let _ = el.set_attribute("maxlength", MAX_FIELD_LEN);
    let _ = el.set_attribute("aria-hidden", "true");

    // Off-screen but real (never `display: none` — a display:none element
    // can never receive focus, so it could never open the keyboard). 1px /
    // opacity 0.01 (not literally 0) is the more conservative of the two
    // common tricks for this class of bug — some mobile Safari versions
    // have refused to raise the keyboard for a truly zero-size/opacity
    // input. font-size 16px avoids iOS Safari's well-known auto-zoom-on-
    // focus behavior for inputs under 16px.
    let style = el.style();
    let _ = style.set_property("position", "fixed");
    let _ = style.set_property("top", "0");
    let _ = style.set_property("left", "0");
    let _ = style.set_property("width", "1px");
    let _ = style.set_property("height", "1px");
    let _ = style.set_property("padding", "0");
    let _ = style.set_property("margin", "0");
    let _ = style.set_property("border", "none");
    let _ = style.set_property("outline", "none");
    let _ = style.set_property("font-size", "16px");
    let _ = style.set_property("opacity", "0.01");
    let _ = style.set_property("pointer-events", "none");
    let _ = style.set_property("z-index", "-1");

    if let Some(body) = document.body() {
        let _ = body.append_child(&el);
    }

    let on_input = {
        let el = el.clone();
        Closure::<dyn FnMut(InputEvent)>::new(move |_ev: InputEvent| {
            let value = el.value();
            PROXY.with(|p| {
                if let Some(proxy) = p.borrow().as_ref() {
                    proxy.pending.borrow_mut().text = Some(value.clone());
                }
            });
        })
    };
    el.set_oninput(Some(on_input.as_ref().unchecked_ref()));

    let on_keydown = Closure::<dyn FnMut(KeyboardEvent)>::new(move |ev: KeyboardEvent| {
        match ev.key().as_str() {
            "Enter" if !ev.repeat() => {
                ev.prevent_default();
                PROXY.with(|p| {
                    if let Some(proxy) = p.borrow().as_ref() {
                        proxy.pending.borrow_mut().submit = true;
                    }
                });
            }
            "Escape" if !ev.repeat() => {
                PROXY.with(|p| {
                    if let Some(proxy) = p.borrow().as_ref() {
                        proxy.pending.borrow_mut().escape = true;
                    }
                });
            }
            "Tab" => {
                ev.prevent_default();
                PROXY.with(|p| {
                    if let Some(proxy) = p.borrow().as_ref() {
                        proxy.pending.borrow_mut().tab = true;
                    }
                });
            }
            _ => {}
        }
    });
    el.set_onkeydown(Some(on_keydown.as_ref().unchecked_ref()));

    let on_blur = Closure::<dyn FnMut(FocusEvent)>::new(move |_ev: FocusEvent| {
        PROXY.with(|p| {
            if let Some(proxy) = p.borrow().as_ref() {
                proxy.pending.borrow_mut().lost_focus = true;
            }
        });
    });
    el.set_onblur(Some(on_blur.as_ref().unchecked_ref()));

    Proxy {
        el,
        _on_input: on_input,
        _on_keydown: on_keydown,
        _on_blur: on_blur,
        pending: RefCell::new(ProxyEvents::default()),
    }
}

/// Gives the proxy real DOM focus so the mobile keyboard opens and typing/
/// paste work — creates the element lazily on first call, otherwise just
/// re-points the existing one (no blur/refocus round-trip, since it's the
/// same element gaining a new value/type, not a focus change). `masked`
/// switches the DOM `type` between `password` (also unlocks the OS's own
/// password-manager/autofill/show-password affordances) and `text`.
pub(crate) fn focus_field(value: &str, masked: bool) {
    PROXY.with(|p| {
        let mut slot = p.borrow_mut();
        if slot.is_none() {
            *slot = Some(create_proxy());
        }
        let proxy = slot.as_ref().unwrap();
        let _ = proxy.el.set_attribute("type", if masked { "password" } else { "text" });
        proxy.el.set_value(value);
        let _ = proxy.el.focus();
    });
}

/// Drops DOM focus from the proxy. Safe/idempotent when nothing is focused
/// or the proxy was never created (e.g. the player never tapped a field on
/// this visit to the menu). Every Rust-initiated focus change (Escape,
/// submit, switching overlays, leaving the menu screen) calls this — the
/// resulting native `blur` DOM event also fires and gets queued into
/// `pending.lost_focus`, but by the time a later frame drains it, the
/// caller has already cleared its own focus state, so the drain is a
/// harmless no-op. Only a genuinely DOM-initiated blur (the user tapping
/// elsewhere, or dismissing the on-screen keyboard) is ever actually acted
/// on by a drain — no explicit origin-tracking is needed either way.
pub(crate) fn blur_proxy() {
    PROXY.with(|p| {
        if let Some(proxy) = p.borrow().as_ref() {
            let _ = proxy.el.blur();
        }
    });
}

/// Drains this frame's accumulated proxy events (typed/pasted text,
/// submit/escape/tab keydowns, DOM-initiated blur) — always clears the
/// internal buffer, so nothing is double-delivered across frames.
pub(crate) fn drain_events() -> ProxyEvents {
    PROXY.with(|p| {
        if let Some(proxy) = p.borrow().as_ref() {
            std::mem::take(&mut *proxy.pending.borrow_mut())
        } else {
            ProxyEvents::default()
        }
    })
}
