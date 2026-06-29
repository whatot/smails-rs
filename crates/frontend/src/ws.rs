use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{CloseEvent, Event, MessageEvent, WebSocket, window};
use yew::prelude::*;

use crate::api::new_message_event;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WsStatus {
    Connecting,
    Connected,
    #[default]
    Disconnected,
}

#[derive(Properties, PartialEq)]
pub struct LiveSocketProps {
    pub token: Option<String>,
    pub on_status: Callback<WsStatus>,
    pub on_new_message: Callback<()>,
}

#[function_component(LiveSocket)]
pub fn live_socket(props: &LiveSocketProps) -> Html {
    let retry = use_state(|| 0_u32);

    {
        let token = props.token.clone();
        let on_status = props.on_status.clone();
        let on_new_message = props.on_new_message.clone();
        let retry = retry.clone();

        use_effect_with((token, *retry), move |(token, retry_count)| {
            let Some(token) = token.clone() else {
                on_status.emit(WsStatus::Disconnected);
                return Box::new(|| {}) as Box<dyn FnOnce()>;
            };
            let Some(url) = ws_url(&token) else {
                on_status.emit(WsStatus::Disconnected);
                return Box::new(|| {}) as Box<dyn FnOnce()>;
            };
            let Ok(socket) = WebSocket::new(&url) else {
                on_status.emit(WsStatus::Disconnected);
                return Box::new(|| {}) as Box<dyn FnOnce()>;
            };

            on_status.emit(WsStatus::Connecting);

            let on_open_status = on_status.clone();
            let onopen = Closure::<dyn FnMut(Event)>::wrap(Box::new(move |_| {
                on_open_status.emit(WsStatus::Connected);
            }));
            socket.set_onopen(Some(onopen.as_ref().unchecked_ref()));

            let on_new = on_new_message.clone();
            let onmessage =
                Closure::<dyn FnMut(MessageEvent)>::wrap(Box::new(move |event: MessageEvent| {
                    let Some(text) = event.data().as_string() else {
                        return;
                    };
                    if text == "pong" {
                        return;
                    }
                    if new_message_event(&text) {
                        on_new.emit(());
                    }
                }));
            socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

            let on_close_status = on_status.clone();
            let close_retry = retry.clone();
            let close_attempt = *retry_count;
            let onclose = Closure::<dyn FnMut(CloseEvent)>::wrap(Box::new(move |_| {
                on_close_status.emit(WsStatus::Disconnected);
                if let Some(window) = window() {
                    let close_retry = close_retry.clone();
                    let delay = (1000_u32.saturating_mul(2_u32.saturating_pow(close_attempt)))
                        .min(30_000) as i32;
                    let callback = Closure::<dyn FnMut()>::once(move || {
                        close_retry.set(close_attempt.saturating_add(1));
                    });
                    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                        callback.as_ref().unchecked_ref(),
                        delay,
                    );
                    callback.forget();
                }
            }));
            socket.set_onclose(Some(onclose.as_ref().unchecked_ref()));

            let error_socket = socket.clone();
            let onerror = Closure::<dyn FnMut(Event)>::wrap(Box::new(move |_| {
                let _ = error_socket.close();
            }));
            socket.set_onerror(Some(onerror.as_ref().unchecked_ref()));

            let ping_socket = socket.clone();
            let ping = Closure::<dyn FnMut()>::wrap(Box::new(move || {
                if ping_socket.ready_state() == WebSocket::OPEN {
                    let _ = ping_socket.send_with_str("ping");
                }
            }));
            let ping_id = window().and_then(|window| {
                window
                    .set_interval_with_callback_and_timeout_and_arguments_0(
                        ping.as_ref().unchecked_ref(),
                        30_000,
                    )
                    .ok()
                    .map(|id| (window, id))
            });

            Box::new(move || {
                if let Some((window, id)) = ping_id {
                    window.clear_interval_with_handle(id);
                }
                socket.set_onopen(None);
                socket.set_onmessage(None);
                socket.set_onclose(None);
                socket.set_onerror(None);
                let _ = socket.close();
                drop(onopen);
                drop(onmessage);
                drop(onclose);
                drop(onerror);
                drop(ping);
            }) as Box<dyn FnOnce()>
        });
    }

    html! {}
}

fn ws_url(token: &str) -> Option<String> {
    let location = window()?.location();
    let protocol = if location.protocol().ok()? == "https:" {
        "wss:"
    } else {
        "ws:"
    };
    Some(format!(
        "{protocol}//{}/api/mailbox/connect?token={token}",
        location.host().ok()?
    ))
}
