//! Ordinary-key registration only. Callbacks enqueue bounded actions; the app owns all effects.
use std::{
    collections::BTreeMap,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU16, AtomicU32, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
};

use eframe::egui::{self, Key, Modifiers};

type Bindings = BTreeMap<String, Option<String>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Action {
    Visibility,
    NewChat,
    Settings,
    Session,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    ScrollUp,
    ScrollDown,
    Ask,
}

impl Action {
    pub const ALL: [Self; 11] = [
        Self::Visibility,
        Self::NewChat,
        Self::Settings,
        Self::Session,
        Self::MoveLeft,
        Self::MoveRight,
        Self::MoveUp,
        Self::MoveDown,
        Self::ScrollUp,
        Self::ScrollDown,
        Self::Ask,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Visibility => "assistant_visibility",
            Self::NewChat => "new_chat",
            Self::Settings => "settings",
            Self::Session => "session_toggle",
            Self::MoveLeft => "move_left",
            Self::MoveRight => "move_right",
            Self::MoveUp => "move_up",
            Self::MoveDown => "move_down",
            Self::ScrollUp => "scroll_up",
            Self::ScrollDown => "scroll_down",
            Self::Ask => "ask",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Visibility => "Show or hide assistant",
            Self::NewChat => "New chat",
            Self::Settings => "Open Settings",
            Self::Session => "Start or stop session",
            Self::MoveLeft => "Move window left",
            Self::MoveRight => "Move window right",
            Self::MoveUp => "Move window up",
            Self::MoveDown => "Move window down",
            Self::ScrollUp => "Scroll response up",
            Self::ScrollDown => "Scroll response down",
            Self::Ask => "Ask question",
        }
    }

    pub fn default_binding(self) -> Option<&'static str> {
        Some(match self {
            Self::Visibility => "CommandOrControl+Backslash",
            Self::NewChat => "CommandOrControl+R",
            Self::Settings => "CommandOrControl+Comma",
            Self::Session => "CommandOrControl+Shift+Backslash",
            Self::MoveLeft => "CommandOrControl+Left",
            Self::MoveRight => "CommandOrControl+Right",
            Self::MoveUp => "CommandOrControl+Up",
            Self::MoveDown => "CommandOrControl+Down",
            Self::ScrollUp => "CommandOrControl+Shift+Up",
            Self::ScrollDown => "CommandOrControl+Shift+Down",
            Self::Ask => return None,
        })
    }

    pub fn repeatable(self) -> bool {
        matches!(
            self,
            Self::MoveLeft
                | Self::MoveRight
                | Self::MoveUp
                | Self::MoveDown
                | Self::ScrollUp
                | Self::ScrollDown
        )
    }

    fn bit(self) -> u16 {
        1 << self as u8
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chord {
    pub key: Key,
    ctrl: bool,
    meta: bool,
    alt: bool,
    shift: bool,
}

impl Chord {
    pub fn parse(value: &str) -> Result<Self, String> {
        cueglint_core::validate_shortcut_binding(value).map_err(|error| error.to_string())?;
        let parts: Vec<_> = value.split('+').collect();
        let key = Key::from_name(parts.last().unwrap()).ok_or("Unsupported key")?;
        let primary = parts.contains(&"CommandOrControl");
        Ok(Self {
            key,
            ctrl: parts.contains(&"Control") || (primary && !cfg!(target_os = "macos")),
            meta: parts.contains(&"Meta") || (primary && cfg!(target_os = "macos")),
            alt: parts.contains(&"Alt"),
            shift: parts.contains(&"Shift"),
        })
    }

    pub fn record(key: Key, modifiers: Modifiers) -> Result<String, String> {
        let mut parts = Vec::new();
        let meta = modifiers.mac_cmd || (cfg!(target_os = "macos") && modifiers.command);
        let ctrl = modifiers.ctrl || (!cfg!(target_os = "macos") && modifiers.command);
        if cfg!(target_os = "macos") && meta && !ctrl || !cfg!(target_os = "macos") && ctrl && !meta
        {
            parts.push("CommandOrControl");
        } else {
            if ctrl {
                parts.push("Control");
            }
            if meta {
                parts.push("Meta");
            }
        }
        if modifiers.alt {
            parts.push("Alt");
        }
        if modifiers.shift {
            parts.push("Shift");
        }
        parts.push(key.name());
        let value = parts.join("+");
        Self::parse(&value)?;
        Ok(value)
    }

    pub fn matches(&self, key: Key, modifiers: Modifiers) -> bool {
        self.key == key
            && self.ctrl == (modifiers.ctrl || (!cfg!(target_os = "macos") && modifiers.command))
            && self.meta == (modifiers.mac_cmd || (cfg!(target_os = "macos") && modifiers.command))
            && self.alt == modifiers.alt
            && self.shift == modifiers.shift
    }

    fn is_quit(&self) -> bool {
        self.matches(Key::Q, Modifiers::COMMAND)
    }
}

pub fn binding<'a>(bindings: &'a Bindings, action: Action) -> Option<&'a str> {
    bindings
        .get(action.id())
        .map_or_else(|| action.default_binding(), |value| value.as_deref())
}

struct Dispatch {
    sender: SyncSender<Event>,
    context: OnceLock<egui::Context>,
    ids: [AtomicU32; 11],
    epochs: [AtomicU64; 11],
    registered: AtomicU16,
    pressed: AtomicU16,
    dropped: AtomicU64,
}

#[derive(Clone)]
pub struct Bridge(Arc<Dispatch>);

pub struct Event {
    action: Action,
    native_id: u32,
    epoch: u64,
}

pub fn channel() -> (Bridge, Receiver<Event>) {
    let (sender, receiver) = mpsc::sync_channel(32);
    (
        Bridge(Arc::new(Dispatch {
            sender,
            context: OnceLock::new(),
            ids: std::array::from_fn(|_| AtomicU32::new(0)),
            epochs: std::array::from_fn(|_| AtomicU64::new(0)),
            registered: AtomicU16::new(0),
            pressed: AtomicU16::new(0),
            dropped: AtomicU64::new(0),
        })),
        receiver,
    )
}

impl Bridge {
    fn native_event(&self, id: u32, pressed: bool) -> bool {
        let registered = self.0.registered.load(Ordering::Acquire);
        for action in Action::ALL {
            if registered & action.bit() != 0
                && self.0.ids[action as usize].load(Ordering::Relaxed) == id
            {
                if pressed {
                    // Windows MOD_NOREPEAT already supplies one event per press, with no release polling.
                    let was_pressed = !cfg!(target_os = "windows")
                        && self.0.pressed.fetch_or(action.bit(), Ordering::Relaxed) & action.bit()
                            != 0;
                    if !was_pressed || action.repeatable() {
                        let event = Event {
                            action,
                            native_id: id,
                            epoch: self.0.epochs[action as usize].load(Ordering::Acquire),
                        };
                        if self.0.sender.try_send(event).is_err() {
                            self.0.dropped.fetch_add(1, Ordering::Relaxed);
                        }
                        if let Some(context) = self.0.context.get() {
                            context.request_repaint_of(egui::ViewportId::ROOT);
                        }
                    }
                } else {
                    self.0.pressed.fetch_and(!action.bit(), Ordering::Relaxed);
                }
                return true;
            }
        }
        false
    }

    fn registered(&self, action: Action, id: u32) {
        self.0.ids[action as usize].store(id, Ordering::Relaxed);
        self.0.registered.fetch_or(action.bit(), Ordering::Release);
    }

    fn clear(&self, action: Action) {
        self.0
            .registered
            .fetch_and(!action.bit(), Ordering::Release);
        self.0.epochs[action as usize].fetch_add(1, Ordering::AcqRel);
        self.0.pressed.fetch_and(!action.bit(), Ordering::Relaxed);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Registered,
    Conflict(String),
    Invalid(String),
    Failed(String),
    RestartRequired(String),
    Unsupported(String),
    Disabled,
    Recording,
}

impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Registered => "Registered globally",
            Self::Conflict(_) => "Conflict",
            Self::Invalid(_) => "Configuration conflict",
            Self::Failed(_) => "Registration failed",
            Self::RestartRequired(_) => "Restart required",
            Self::Unsupported(_) => "Global unavailable",
            Self::Disabled => "Disabled",
            Self::Recording => "Paused while recording",
        }
    }
    pub fn detail(&self) -> &str {
        match self {
            Self::Conflict(value)
            | Self::Invalid(value)
            | Self::Failed(value)
            | Self::RestartRequired(value)
            | Self::Unsupported(value) => value,
            _ => "",
        }
    }
    pub fn retryable(&self) -> bool {
        matches!(self, Self::Conflict(_) | Self::Failed(_))
    }
}

trait Registrar {
    fn register(&mut self, action: Action, chord: &Chord) -> Result<u32, Status>;
    fn unregister(&mut self, action: Action, chord: &Chord) -> Result<(), Status>;
}

pub struct Manager {
    bridge: Bridge,
    receiver: Receiver<Event>,
    registrar: Box<dyn Registrar>,
    configured: Option<Bindings>,
    recording: bool,
    registered: BTreeMap<Action, Chord>,
    statuses: BTreeMap<Action, Status>,
}

impl Manager {
    pub fn disabled(context: egui::Context) -> Self {
        let (bridge, receiver) = channel();
        Self::with_registrar(
            bridge,
            receiver,
            context,
            Box::new(Unavailable(
                "Global registration is disabled in fixtures and headless tests.",
            )),
        )
    }

    pub fn native(
        bridge: Bridge,
        receiver: Receiver<Event>,
        context: egui::Context,
        fixture: bool,
    ) -> Self {
        if fixture {
            return Self::with_registrar(
                bridge,
                receiver,
                context,
                Box::new(Unavailable("Global registration is disabled in fixtures.")),
            );
        }
        let registrar = platform::registrar(&bridge);
        Self::with_registrar(bridge, receiver, context, registrar)
    }

    fn with_registrar(
        bridge: Bridge,
        receiver: Receiver<Event>,
        context: egui::Context,
        registrar: Box<dyn Registrar>,
    ) -> Self {
        let _ = bridge.0.context.set(context);
        Self {
            bridge,
            receiver,
            registrar,
            configured: None,
            recording: false,
            registered: BTreeMap::new(),
            statuses: BTreeMap::new(),
        }
    }

    pub fn status(&self, action: Action) -> Status {
        self.statuses
            .get(&action)
            .cloned()
            .unwrap_or(Status::Disabled)
    }

    pub fn reconcile(&mut self, bindings: &Bindings, recording: bool, retry: Option<Action>) {
        let retry_only = self.configured.as_ref() == Some(bindings) && self.recording == recording;
        if self.configured.as_ref() == Some(bindings)
            && self.recording == recording
            && retry.is_none()
        {
            return;
        }
        let desired: BTreeMap<_, _> = Action::ALL
            .into_iter()
            .map(|action| (action, binding(bindings, action).map(Chord::parse)))
            .collect();
        for action in Action::ALL {
            if retry_only && retry.is_some_and(|requested| requested != action) {
                continue;
            }
            if matches!(self.statuses.get(&action), Some(Status::RestartRequired(_))) {
                continue;
            }
            let parsed = &desired[&action];
            let duplicate = parsed
                .as_ref()
                .and_then(|chord| chord.as_ref().ok())
                .and_then(|chord| {
                    desired
                        .iter()
                        .find(|(other, value)| {
                            **other != action
                                && value
                                    .as_ref()
                                    .is_some_and(|value| value.as_ref().ok() == Some(chord))
                        })
                        .map(|(other, _)| *other)
                });
            let next = parsed.as_ref().and_then(|value| value.as_ref().ok());
            if !recording
                && duplicate.is_none()
                && next.is_some()
                && self.registered.get(&action) == next
                && self.statuses.get(&action) == Some(&Status::Registered)
                && retry != Some(action)
            {
                continue;
            }
            if let Some(old) = self.registered.get(&action).cloned() {
                self.bridge.clear(action);
                if let Err(error) = self.registrar.unregister(action, &old) {
                    self.statuses.insert(action, error);
                    continue;
                }
                self.registered.remove(&action);
            }
            let status = if recording {
                Status::Recording
            } else if let Some(other) = duplicate {
                Status::Invalid(format!(
                    "Also assigned to {}. Choose a different combination.",
                    other.label()
                ))
            } else {
                match parsed {
                    None => Status::Disabled,
                    Some(Err(error)) => Status::Invalid(error.clone()),
                    Some(Ok(chord)) if chord.is_quit() => {
                        Status::Invalid("This combination is reserved for Quit.".into())
                    }
                    Some(Ok(chord)) => match self.registrar.register(action, chord) {
                        Ok(id) => {
                            self.registered.insert(action, chord.clone());
                            self.bridge.registered(action, id);
                            Status::Registered
                        }
                        Err(status) => status,
                    },
                }
            };
            self.statuses.insert(action, status);
        }
        self.configured = Some(bindings.clone());
        self.recording = recording;
    }

    pub fn local_action(&self, key: Key, modifiers: Modifiers, repeat: bool) -> Option<Action> {
        let bindings = self.configured.as_ref()?;
        Action::ALL.into_iter().find(|action| {
            !self.registered.contains_key(action)
                && (!repeat || action.repeatable())
                && !matches!(self.statuses.get(action), Some(Status::Invalid(_)))
                && binding(bindings, *action)
                    .and_then(|value| Chord::parse(value).ok())
                    .is_some_and(|chord| chord.matches(key, modifiers))
        })
    }

    pub fn owns_key(&self, key: Key, modifiers: Modifiers) -> bool {
        self.registered
            .values()
            .any(|chord| chord.matches(key, modifiers))
    }

    pub fn next(&self) -> Option<Action> {
        while let Ok(event) = self.receiver.try_recv() {
            if self.bridge.0.registered.load(Ordering::Acquire) & event.action.bit() != 0
                && self.bridge.0.ids[event.action as usize].load(Ordering::Relaxed)
                    == event.native_id
                && self.bridge.0.epochs[event.action as usize].load(Ordering::Acquire)
                    == event.epoch
            {
                return Some(event.action);
            }
        }
        None
    }
    pub fn dropped(&self) -> u64 {
        self.bridge.0.dropped.swap(0, Ordering::Relaxed)
    }

    pub fn shutdown(&mut self) {
        for (action, chord) in self.registered.clone() {
            self.bridge.clear(action);
            if matches!(self.statuses.get(&action), Some(Status::RestartRequired(_))) {
                continue;
            }
            match self.registrar.unregister(action, &chord) {
                Err(error) => {
                    self.statuses.insert(action, error);
                }
                Ok(()) => {
                    self.registered.remove(&action);
                }
            }
        }
        while self.receiver.try_recv().is_ok() {}
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Unavailable(&'static str);
impl Registrar for Unavailable {
    fn register(&mut self, _: Action, _: &Chord) -> Result<u32, Status> {
        Err(Status::Unsupported(self.0.into()))
    }
    fn unregister(&mut self, _: Action, _: &Chord) -> Result<(), Status> {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};

    struct MacRegistrar {
        manager: Option<GlobalHotKeyManager>,
        thread: std::thread::ThreadId,
        next_id: u32,
        hotkeys: BTreeMap<Action, HotKey>,
    }

    pub(super) fn registrar(bridge: &Bridge) -> Box<dyn Registrar> {
        let weak = Arc::downgrade(&bridge.0);
        // global-hotkey's handler is process-wide and set once; this app has one retained native manager.
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            if let Some(dispatch) = weak.upgrade() {
                Bridge(dispatch).native_event(event.id, event.state == HotKeyState::Pressed);
            }
        }));
        Box::new(MacRegistrar {
            manager: None,
            thread: std::thread::current().id(),
            next_id: 1,
            hotkeys: BTreeMap::new(),
        })
    }

    fn hotkey(chord: &Chord) -> Result<HotKey, Status> {
        let mut parts = Vec::new();
        if chord.ctrl {
            parts.push("control".to_owned());
        }
        if chord.meta {
            parts.push("meta".to_owned());
        }
        if chord.alt {
            parts.push("alt".to_owned());
        }
        if chord.shift {
            parts.push("shift".to_owned());
        }
        let key = match chord.key {
            Key::ArrowLeft => "ArrowLeft".into(),
            Key::ArrowRight => "ArrowRight".into(),
            Key::ArrowUp => "ArrowUp".into(),
            Key::ArrowDown => "ArrowDown".into(),
            Key::Equals => "Equal".into(),
            Key::OpenBracket => "BracketLeft".into(),
            Key::CloseBracket => "BracketRight".into(),
            Key::Backtick => "Backquote".into(),
            key if key.name().len() == 1 && key.name().as_bytes()[0].is_ascii_uppercase() => {
                format!("Key{}", key.name())
            }
            key if key.name().len() == 1 && key.name().as_bytes()[0].is_ascii_digit() => {
                format!("Digit{}", key.name())
            }
            key => key.name().into(),
        };
        parts.push(key);
        parts
            .join("+")
            .parse()
            .map_err(|error: global_hotkey::hotkey::HotKeyParseError| {
                Status::Failed(error.to_string())
            })
    }

    impl Registrar for MacRegistrar {
        fn register(&mut self, action: Action, chord: &Chord) -> Result<u32, Status> {
            assert_eq!(
                self.thread,
                std::thread::current().id(),
                "shortcut registration must stay on the UI thread"
            );
            if self.manager.is_none() {
                self.manager = Some(
                    GlobalHotKeyManager::new()
                        .map_err(|error| Status::Failed(error.to_string()))?,
                );
            }
            let hotkey = self.next_hotkey(chord)?;
            self.manager.as_ref().unwrap().register(hotkey).map_err(|error| match error {
                global_hotkey::Error::AlreadyRegistered(_) => Status::Conflict("Another application or the system uses this combination. Change it or retry when it becomes available.".into()),
                error => Status::Failed(error.to_string()),
            })?;
            self.hotkeys.insert(action, hotkey);
            Ok(hotkey.id())
        }
        fn unregister(&mut self, action: Action, _: &Chord) -> Result<(), Status> {
            assert_eq!(
                self.thread,
                std::thread::current().id(),
                "shortcut cleanup must stay on the UI thread"
            );
            let Some(hotkey) = self.hotkeys.get(&action).copied() else {
                return Ok(());
            };
            if let Some(manager) = &self.manager {
                manager
                    .unregister(hotkey)
                    .map_err(|error| Status::RestartRequired(format!("Native shortcut cleanup failed: {error}. Restart Cueglint after stopping your session. This shortcut stays inactive until quit.")))?;
            }
            self.hotkeys.remove(&action);
            Ok(())
        }
    }

    impl MacRegistrar {
        fn next_hotkey(&mut self, chord: &Chord) -> Result<HotKey, Status> {
            let mut hotkey = hotkey(chord)?;
            hotkey.id = self.next_id;
            self.next_id = self.next_id.checked_add(1).ok_or_else(|| {
                Status::Failed("Shortcut registration IDs exhausted. Restart Cueglint.".into())
            })?;
            Ok(hotkey)
        }
    }

    #[test]
    fn mac_ids_are_fresh_for_the_same_chord_and_exhaust_without_reuse() {
        let mut registrar = MacRegistrar {
            manager: None,
            thread: std::thread::current().id(),
            next_id: 1,
            hotkeys: BTreeMap::new(),
        };
        let chord = Chord::parse("CommandOrControl+Backslash").unwrap();
        let first = registrar.next_hotkey(&chord).unwrap();
        let second = registrar.next_hotkey(&chord).unwrap();
        assert_eq!(first.key, second.key);
        assert_eq!(first.mods, second.mods);
        assert_ne!(first.id, second.id);
        registrar.hotkeys.insert(Action::Visibility, second);
        assert_eq!(registrar.hotkeys[&Action::Visibility].id, second.id);
        registrar.next_id = u32::MAX;
        assert!(registrar.next_hotkey(&chord).is_err());
        assert_eq!(registrar.next_id, u32::MAX);
        assert!(
            registrar.manager.is_none(),
            "this test never constructs a native manager"
        );
    }

    #[test]
    fn all_canonical_ordinary_keys_convert_without_native_registration() {
        for key in Key::ALL {
            let value = format!("CommandOrControl+{}", key.name());
            if let Ok(chord) = Chord::parse(&value) {
                hotkey(&chord).unwrap();
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

    pub(super) fn registrar(_: &Bridge) -> Box<dyn Registrar> {
        Box::new(WindowsRegistrar {
            thread: std::thread::current().id(),
            next_id: 1000,
            ids: BTreeMap::new(),
        })
    }
    struct WindowsRegistrar {
        thread: std::thread::ThreadId,
        next_id: i32,
        ids: BTreeMap<Action, i32>,
    }
    impl Registrar for WindowsRegistrar {
        fn register(&mut self, action: Action, chord: &Chord) -> Result<u32, Status> {
            assert_eq!(self.thread, std::thread::current().id());
            if chord.meta || chord.key == Key::F12 {
                return Err(Status::Unsupported("Windows reserves Windows-key combinations and F12. Choose a Control combination.".into()));
            }
            let key = virtual_key(chord.key).ok_or_else(|| {
                Status::Unsupported("This key has no Windows shortcut mapping.".into())
            })?;
            let mut modifiers = if action.repeatable() { 0 } else { MOD_NOREPEAT };
            if chord.ctrl {
                modifiers |= MOD_CONTROL;
            }
            if chord.alt {
                modifiers |= MOD_ALT;
            }
            if chord.shift {
                modifiers |= MOD_SHIFT;
            }
            if self.next_id > 0xBFFF {
                return Err(Status::Failed("Shortcut registration IDs exhausted. Restart Cueglint to register more combinations.".into()));
            }
            let id = self.next_id;
            self.next_id += 1;
            if unsafe { RegisterHotKey(std::ptr::null_mut(), id, modifiers, u32::from(key)) } == 0 {
                let error = std::io::Error::last_os_error();
                return Err(if error.raw_os_error() == Some(1409) {
                    Status::Conflict("Another application or the system uses this combination. Change it or retry when it becomes available.".into())
                } else {
                    Status::Failed(error.to_string())
                });
            }
            self.ids.insert(action, id);
            Ok(id as u32)
        }
        fn unregister(&mut self, action: Action, _: &Chord) -> Result<(), Status> {
            assert_eq!(self.thread, std::thread::current().id());
            let Some(id) = self.ids.get(&action) else {
                return Ok(());
            };
            if unsafe { UnregisterHotKey(std::ptr::null_mut(), *id) } == 0 {
                return Err(Status::Failed(std::io::Error::last_os_error().to_string()));
            }
            self.ids.remove(&action);
            Ok(())
        }
    }

    fn virtual_key(key: Key) -> Option<VIRTUAL_KEY> {
        let name = key.name();
        if name.len() == 1 && name.as_bytes()[0].is_ascii_alphanumeric() {
            return Some(u16::from(name.as_bytes()[0]));
        }
        if let Some(number) = name
            .strip_prefix('F')
            .and_then(|value| value.parse::<u16>().ok())
        {
            if (1..=20).contains(&number) {
                return Some(VK_F1 + number - 1);
            }
        }
        Some(match key {
            Key::ArrowLeft => VK_LEFT,
            Key::ArrowRight => VK_RIGHT,
            Key::ArrowUp => VK_UP,
            Key::ArrowDown => VK_DOWN,
            Key::Space => VK_SPACE,
            Key::Tab => VK_TAB,
            Key::Enter => VK_RETURN,
            Key::Backspace => VK_BACK,
            Key::Insert => VK_INSERT,
            Key::Home => VK_HOME,
            Key::End => VK_END,
            Key::PageUp => VK_PRIOR,
            Key::PageDown => VK_NEXT,
            Key::Comma => VK_OEM_COMMA,
            Key::Minus => VK_OEM_MINUS,
            Key::Period => VK_OEM_PERIOD,
            Key::Equals => VK_OEM_PLUS,
            Key::Semicolon => VK_OEM_1,
            Key::Backslash => VK_OEM_5,
            Key::Slash => VK_OEM_2,
            Key::OpenBracket => VK_OEM_4,
            Key::CloseBracket => VK_OEM_6,
            Key::Backtick => VK_OEM_3,
            Key::Quote => VK_OEM_7,
            _ => return None,
        })
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::*;
    pub(super) fn registrar(_: &Bridge) -> Box<dyn Registrar> {
        Box::new(Unavailable(
            "Linux X11 registration is not implemented in this build; native Wayland requires a GlobalShortcuts portal backend. Focused shortcuts remain available.",
        ))
    }
}

pub fn configure_native(options: &mut eframe::NativeOptions, bridge: &Bridge) {
    #[cfg(target_os = "windows")]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let bridge = bridge.clone();
        options.event_loop_builder = Some(Box::new(move |builder| {
            builder.with_msg_hook(move |message| {
                let message = unsafe {
                    &*(message.cast::<windows_sys::Win32::UI::WindowsAndMessaging::MSG>())
                };
                message.message == windows_sys::Win32::UI::WindowsAndMessaging::WM_HOTKEY
                    && bridge.native_event(message.wParam as u32, true)
            });
        }));
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (options, bridge);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    #[derive(Default)]
    struct FakeState {
        next_id: u32,
        fail_register: bool,
        fail_unregister: bool,
        restart_required: bool,
        removed: Vec<Chord>,
    }

    struct FakeRegistrar(Rc<RefCell<FakeState>>);
    impl Registrar for FakeRegistrar {
        fn register(&mut self, _: Action, _: &Chord) -> Result<u32, Status> {
            let mut state = self.0.borrow_mut();
            if state.fail_register {
                return Err(Status::Conflict("A test peer owns this combination".into()));
            }
            state.next_id += 1;
            Ok(state.next_id)
        }
        fn unregister(&mut self, _: Action, chord: &Chord) -> Result<(), Status> {
            let mut state = self.0.borrow_mut();
            state.removed.push(chord.clone());
            if state.restart_required {
                Err(Status::RestartRequired(
                    "The simulated native reference was lost. Restart required.".into(),
                ))
            } else if state.fail_unregister {
                Err(Status::Failed("Test cleanup failure".into()))
            } else {
                Ok(())
            }
        }
    }

    fn manager(context: egui::Context) -> (Manager, Rc<RefCell<FakeState>>) {
        let (bridge, receiver) = channel();
        let state = Rc::new(RefCell::new(FakeState::default()));
        (
            Manager::with_registrar(
                bridge,
                receiver,
                context,
                Box::new(FakeRegistrar(state.clone())),
            ),
            state,
        )
    }

    // Explicitly simulated registration status for app lifecycle tests, never an OS registration.
    pub(crate) fn registered_manager(context: egui::Context) -> Manager {
        let (mut manager, _) = manager(context);
        manager.reconcile(&Bindings::new(), false, None);
        manager
    }

    pub(crate) fn conflicted_manager(context: egui::Context) -> Manager {
        let (mut manager, state) = manager(context);
        state.borrow_mut().fail_register = true;
        manager.reconcile(&Bindings::new(), false, None);
        manager
    }

    fn only(action: Action, chord: &str) -> Bindings {
        let mut bindings: Bindings = Action::ALL
            .into_iter()
            .map(|action| (action.id().into(), None))
            .collect();
        bindings.insert(action.id().into(), Some(chord.into()));
        bindings
    }

    #[test]
    fn exact_modifiers_separate_session_visibility_and_ignore_one_shot_repeats() {
        let mut manager = Manager::disabled(egui::Context::default());
        manager.reconcile(&Bindings::new(), false, None);
        assert_eq!(
            manager.local_action(Key::Backslash, Modifiers::COMMAND, false),
            Some(Action::Visibility)
        );
        assert_eq!(
            manager.local_action(Key::Backslash, Modifiers::COMMAND | Modifiers::SHIFT, false),
            Some(Action::Session)
        );
        assert_eq!(
            manager.local_action(Key::Backslash, Modifiers::COMMAND | Modifiers::ALT, false),
            None
        );
        assert_eq!(manager.local_action(Key::R, Modifiers::COMMAND, true), None);
        assert_eq!(
            manager.local_action(Key::ArrowUp, Modifiers::COMMAND | Modifiers::SHIFT, true),
            Some(Action::ScrollUp)
        );
        assert_eq!(binding(&Bindings::new(), Action::Ask), None);
        assert!(Chord::record(Key::R, Modifiers::NONE).is_err());
        assert!(Chord::record(Key::Delete, Modifiers::COMMAND).is_err());
    }

    #[test]
    fn conflict_retry_avoids_duplicate_focused_dispatch_and_releases_registration() {
        let (mut manager, state) = manager(egui::Context::default());
        let bindings = only(Action::Visibility, "CommandOrControl+Backslash");
        state.borrow_mut().fail_register = true;
        manager.reconcile(&bindings, false, None);
        assert!(matches!(
            manager.status(Action::Visibility),
            Status::Conflict(_)
        ));
        assert_eq!(
            manager.local_action(Key::Backslash, Modifiers::COMMAND, false),
            Some(Action::Visibility)
        );
        state.borrow_mut().fail_register = false;
        manager.reconcile(&bindings, false, Some(Action::Visibility));
        assert_eq!(manager.status(Action::Visibility), Status::Registered);
        assert_eq!(
            manager.local_action(Key::Backslash, Modifiers::COMMAND, false),
            None
        );
        assert!(manager.bridge.native_event(1, true));
        assert_eq!(manager.next(), Some(Action::Visibility));
        manager.bridge.native_event(1, false);
        manager.bridge.native_event(1, true);
        assert_eq!(manager.next(), Some(Action::Visibility));
        manager.shutdown();
        assert!(!manager.bridge.native_event(1, true));
        assert_eq!(state.borrow().removed.len(), 1);
    }

    #[test]
    fn rebind_and_recording_invalidate_queued_events_and_queue_size_is_bounded() {
        let (mut manager, _) = manager(egui::Context::default());
        let mut bindings = only(Action::Visibility, "CommandOrControl+Backslash");
        manager.reconcile(&bindings, false, None);
        manager.bridge.native_event(1, true);
        bindings.insert(
            Action::Visibility.id().into(),
            Some("CommandOrControl+Alt+Backslash".into()),
        );
        manager.reconcile(&bindings, false, None);
        assert_eq!(manager.next(), None);
        assert!(!manager.bridge.native_event(1, true));
        manager.bridge.native_event(2, true);
        manager.reconcile(&bindings, true, None);
        assert_eq!(manager.next(), None);
        assert!(!manager.bridge.native_event(2, true));
        manager.reconcile(&bindings, false, None);
        assert!(!manager.bridge.native_event(2, true));
        assert!(manager.bridge.native_event(3, true));
        assert_eq!(manager.next(), Some(Action::Visibility));

        let bindings = only(Action::MoveRight, "CommandOrControl+Right");
        manager.reconcile(&bindings, false, None);
        for _ in 0..100 {
            manager.bridge.native_event(4, true);
        }
        assert_eq!(std::iter::from_fn(|| manager.next()).count(), 32);
        assert_eq!(manager.dropped(), 68);
    }

    #[test]
    fn failed_unregister_keeps_the_old_cleanup_identity_until_retry() {
        let (mut manager, state) = manager(egui::Context::default());
        let mut bindings = only(Action::Visibility, "CommandOrControl+Backslash");
        manager.reconcile(&bindings, false, None);
        state.borrow_mut().fail_unregister = true;
        bindings.insert(
            Action::Visibility.id().into(),
            Some("CommandOrControl+Alt+Backslash".into()),
        );
        manager.reconcile(&bindings, false, None);
        assert!(matches!(
            manager.status(Action::Visibility),
            Status::Failed(_)
        ));
        assert_eq!(
            manager.registered[&Action::Visibility],
            Chord::parse("CommandOrControl+Backslash").unwrap()
        );
        assert!(!manager.bridge.native_event(1, true));
        state.borrow_mut().fail_unregister = false;
        manager.reconcile(&bindings, false, Some(Action::Visibility));
        assert_eq!(manager.status(Action::Visibility), Status::Registered);
        assert_eq!(state.borrow().removed[0], state.borrow().removed[1]);
        manager.shutdown();
        assert!(manager.registered.is_empty());
    }

    #[test]
    fn lost_native_reference_cannot_turn_a_no_op_retry_into_success() {
        let (mut manager, state) = manager(egui::Context::default());
        let mut bindings = only(Action::Visibility, "CommandOrControl+Backslash");
        manager.reconcile(&bindings, false, None);
        state.borrow_mut().restart_required = true;
        bindings.insert(
            Action::Visibility.id().into(),
            Some("CommandOrControl+Alt+Backslash".into()),
        );
        manager.reconcile(&bindings, false, None);
        assert!(matches!(
            manager.status(Action::Visibility),
            Status::RestartRequired(_)
        ));
        assert!(!manager.status(Action::Visibility).retryable());
        assert!(!manager.bridge.native_event(1, true));
        state.borrow_mut().restart_required = false;
        manager.reconcile(&bindings, false, Some(Action::Visibility));
        manager.reconcile(&bindings, true, None);
        manager.shutdown();
        assert_eq!(
            state.borrow().removed.len(),
            1,
            "never invoke a misleading upstream no-op retry"
        );
        assert_eq!(
            state.borrow().next_id,
            1,
            "uncertain native state cannot be replaced by another registration"
        );
        assert!(manager.registered.contains_key(&Action::Visibility));
        assert!(matches!(
            manager.status(Action::Visibility),
            Status::RestartRequired(_)
        ));
    }
}
