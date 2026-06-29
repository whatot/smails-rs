use smails_core::{MailboxCreated, MessageDetail, MessageSummary};
use wasm_bindgen_futures::spawn_local;
use web_sys::window;
use yew::prelude::*;

use crate::{
    api::{self, ApiError, ApiResponse},
    components::{AgentDocs, ApiDocs, Faq, Hero, Inbox, Shell},
    pages::{ApiPage, McpPage, OtpPage},
    storage,
    ws::{LiveSocket, WsStatus},
};

#[derive(Clone, Default, PartialEq)]
struct State {
    address: String,
    token: Option<String>,
    messages: Vec<MessageSummary>,
    selected_id: Option<String>,
    detail: Option<MessageDetail>,
    loaded: bool,
    notice: Option<String>,
    error: Option<String>,
    server_version: Option<String>,
    ws_status: WsStatus,
}

#[function_component(App)]
pub fn app() -> Html {
    match current_route() {
        Route::Home => html! { <HomePage /> },
        Route::Mcp => html! { <McpPage /> },
        Route::Api => html! { <ApiPage /> },
        Route::Otp => html! { <OtpPage /> },
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Route {
    Home,
    Mcp,
    Api,
    Otp,
}

#[function_component(HomePage)]
fn home_page() -> Html {
    let state = use_state(State::default);

    {
        let state = state.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                initialize(state).await;
            });
            || {}
        });
    }

    let refresh = {
        let state = state.clone();
        Callback::from(move |_| refresh_messages(state.clone()))
    };
    let new_address = {
        let state = state.clone();
        Callback::from(move |_| create_new_mailbox(state.clone()))
    };
    let copy_address = {
        let state = state.clone();
        Callback::from(move |_| {
            if let Some(clipboard) = window().map(|window| window.navigator().clipboard()) {
                let _ = clipboard.write_text(&state.address);
                set_notice(&state, "Address copied.");
            }
        })
    };
    let open_message = {
        let state = state.clone();
        Callback::from(move |id: String| open_message(state.clone(), id))
    };
    let delete_message = {
        let state = state.clone();
        Callback::from(move |id: String| delete_message(state.clone(), id))
    };
    let back = {
        let state = state.clone();
        Callback::from(move |_| {
            let mut next = (*state).clone();
            next.selected_id = None;
            next.detail = None;
            state.set(next);
        })
    };
    let ws_status = {
        let state = state.clone();
        Callback::from(move |status| {
            let mut next = (*state).clone();
            next.ws_status = status;
            state.set(next);
        })
    };
    let ws_new_message = {
        let state = state.clone();
        Callback::from(move |_| refresh_messages(state.clone()))
    };

    html! {
        <Shell>
            <LiveSocket token={state.token.clone()} on_status={ws_status} on_new_message={ws_new_message} />
            <Hero />
            <Inbox
                address={state.address.clone()}
                messages={state.messages.clone()}
                selected_id={state.selected_id.clone()}
                detail={state.detail.clone()}
                loaded={state.loaded}
                ws_status={state.ws_status}
                notice={state.notice.clone()}
                error={state.error.clone()}
                on_new={new_address}
                on_refresh={refresh}
                on_copy={copy_address}
                on_open={open_message}
                on_delete={delete_message}
                on_back={back}
            />
            <AgentDocs />
            <ApiDocs />
            <Faq />
        </Shell>
    }
}

fn current_route() -> Route {
    let path = window()
        .and_then(|window| window.location().pathname().ok())
        .unwrap_or_else(|| "/".to_owned());
    match path.as_str() {
        "/mcp" => Route::Mcp,
        "/email-api" => Route::Api,
        "/otp" => Route::Otp,
        _ => Route::Home,
    }
}

async fn initialize(state: UseStateHandle<State>) {
    let token = storage::token();
    let address = storage::address();
    if let (Some(token), Some(address)) = (token, address) {
        update(&state, |next| {
            next.token = Some(token.clone());
            next.address = address;
        });
        match api::list_messages(&token).await {
            Ok(response) => {
                apply_messages(&state, response);
                update(&state, |next| next.loaded = true);
                return;
            }
            Err(err) if matches!(err.status, Some(401 | 403)) => {
                storage::clear();
            }
            Err(err) => {
                update(&state, |next| {
                    next.error = Some(err.message);
                    next.loaded = true;
                });
                return;
            }
        }
    }

    match api::create_mailbox().await {
        Ok(response) => {
            apply_mailbox(&state, response);
            update(&state, |next| next.loaded = true);
        }
        Err(err) => {
            update(&state, |next| {
                next.error = Some(format!("Failed to initialize: {}", err.message));
                next.loaded = true;
            });
        }
    }
}

fn refresh_messages(state: UseStateHandle<State>) {
    let Some(token) = state.token.clone() else {
        return;
    };
    spawn_local(async move {
        match api::list_messages(&token).await {
            Ok(response) => apply_messages(&state, response),
            Err(err) => set_error(&state, err),
        }
    });
}

fn create_new_mailbox(state: UseStateHandle<State>) {
    spawn_local(async move {
        match api::create_mailbox().await {
            Ok(response) => {
                apply_mailbox(&state, response);
                update(&state, |next| {
                    next.messages.clear();
                    next.selected_id = None;
                    next.detail = None;
                    next.notice = Some("New mailbox created.".to_owned());
                });
            }
            Err(err) => set_error(&state, err),
        }
    });
}

fn open_message(state: UseStateHandle<State>, id: String) {
    let Some(token) = state.token.clone() else {
        return;
    };
    update(&state, |next| {
        next.selected_id = Some(id.clone());
        next.detail = None;
        if let Some(message) = next.messages.iter_mut().find(|message| message.id == id) {
            message.read = 1;
        }
    });
    spawn_local(async move {
        match api::get_message(&token, &id).await {
            Ok(response) => {
                let ApiResponse { data, version } = response;
                update(&state, |next| {
                    apply_version(next, version);
                    if next.selected_id.as_deref() == Some(data.id.as_str()) {
                        next.detail = Some(data);
                    }
                });
            }
            Err(err) => set_error(&state, err),
        }
    });
}

fn delete_message(state: UseStateHandle<State>, id: String) {
    let Some(token) = state.token.clone() else {
        return;
    };
    spawn_local(async move {
        match api::delete_message(&token, &id).await {
            Ok(response) => update(&state, |next| {
                apply_version(next, response.version);
                next.messages.retain(|message| message.id != id);
                if next.selected_id.as_deref() == Some(id.as_str()) {
                    next.selected_id = None;
                    next.detail = None;
                }
                next.notice = Some("Message deleted.".to_owned());
            }),
            Err(err) => set_error(&state, err),
        }
    });
}

fn apply_mailbox(state: &UseStateHandle<State>, response: ApiResponse<MailboxCreated>) {
    let ApiResponse { data, version } = response;
    storage::save(&data.address, &data.token);
    update(state, |next| {
        apply_version(next, version);
        next.address = data.address;
        next.token = Some(data.token);
        next.error = None;
    });
}

fn apply_messages(state: &UseStateHandle<State>, response: ApiResponse<Vec<MessageSummary>>) {
    update(state, |next| {
        apply_version(next, response.version);
        next.messages = response.data;
        next.error = None;
    });
}

fn apply_version(state: &mut State, version: Option<String>) {
    let Some(version) = version else {
        return;
    };
    match &state.server_version {
        None => state.server_version = Some(version),
        Some(current) if current != &version => {
            state.server_version = Some(version);
            state.notice = Some("A new version is available. Refresh when convenient.".to_owned());
        }
        _ => {}
    }
}

fn set_notice(state: &UseStateHandle<State>, message: &str) {
    update(state, |next| {
        next.notice = Some(message.to_owned());
        next.error = None;
    });
}

fn set_error(state: &UseStateHandle<State>, err: ApiError) {
    update(state, |next| {
        next.error = Some(err.message);
    });
}

fn update(state: &UseStateHandle<State>, f: impl FnOnce(&mut State)) {
    let mut next = (**state).clone();
    f(&mut next);
    state.set(next);
}
