//! The menu bar panel as macOS wants one.
//!
//! The window tao makes is an ordinary window: showing it activates the
//! app, and activating the app switches to whatever space the desk is on,
//! so a click on the menu bar icon over a full-screen app threw the person
//! out of it. A menu bar panel is an `NSPanel` with the non-activating
//! style: it takes key without bringing the app forward, joins every
//! space, sits at the pop-up menu level so it shows over full-screen apps,
//! and stays out of Mission Control. The window is adopted into that class
//! in place, the way menu bar apps built on Tauri do it, since `NSPanel`
//! adds behaviour and no storage.

use objc2::{define_class, msg_send, runtime::AnyObject, ClassType, MainThreadOnly};
use objc2_app_kit::NSPanel;
use objc2_foundation::{NSPoint, NSRect, NSSize};

define_class!(
    // SAFETY: NSPanel has no subclassing requirements beyond the main
    // thread, and the struct has no Drop and no ivars.
    #[unsafe(super(NSPanel))]
    #[thread_kind = MainThreadOnly]
    #[name = "PrincipiaMenuPanel"]
    struct MenuPanel;

    impl MenuPanel {
        /// A borderless panel refuses key status by default; this one takes
        /// it so the card's buttons and keys work without the app coming forward.
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            true
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool {
            false
        }
    }
);

/// `NSWindowStyleMaskNonactivatingPanel`.
const NONACTIVATING_PANEL: usize = 1 << 7;
/// `NSPopUpMenuWindowLevel`: above full-screen apps and the menu bar's own menus.
const POP_UP_MENU_LEVEL: isize = 101;
/// `NSWindowAnimationBehaviorNone`: the card animates itself.
const ANIMATION_NONE: isize = 2;
const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
const TRANSIENT: usize = 1 << 3;
const IGNORES_CYCLE: usize = 1 << 6;
const FULL_SCREEN_AUXILIARY: usize = 1 << 8;

/// Make the window the panel, once. Safe to call again.
///
/// # Safety
/// `ns_window` must be a live `NSWindow`, and this must run on the main thread.
pub unsafe fn adopt(ns_window: *mut AnyObject) {
    let window = &*ns_window;
    if window.class() != MenuPanel::class() {
        AnyObject::set_class(window, MenuPanel::class());
    }
    let mask: usize = msg_send![window, styleMask];
    let _: () = msg_send![window, setStyleMask: mask | NONACTIVATING_PANEL];
    let _: () = msg_send![window, setLevel: POP_UP_MENU_LEVEL];
    let _: () = msg_send![
        window,
        setCollectionBehavior: CAN_JOIN_ALL_SPACES | TRANSIENT | IGNORES_CYCLE | FULL_SCREEN_AUXILIARY
    ];
    let _: () = msg_send![window, setHidesOnDeactivate: false];
    let _: () = msg_send![window, setFloatingPanel: true];
    let _: () = msg_send![window, setBecomesKeyOnlyIfNeeded: false];
    let _: () = msg_send![window, setAnimationBehavior: ANIMATION_NONE];
}

/// Place the panel by its top-left corner, in points from the top-left of
/// the main display (the coordinates the tray icon reports).
///
/// # Safety
/// As for [`adopt`].
pub unsafe fn place(ns_window: *mut AnyObject, x: f64, y: f64, width: f64, height: f64) {
    let window = &*ns_window;
    // Cocoa measures from the bottom of the first screen; flip against it.
    let screens: *mut AnyObject = msg_send![objc2::class!(NSScreen), screens];
    let count: usize = msg_send![screens, count];
    let main_height = if count > 0 {
        let first: *mut AnyObject = msg_send![screens, objectAtIndex: 0usize];
        let frame: NSRect = msg_send![first, frame];
        frame.size.height
    } else {
        0.0
    };
    let _: () = msg_send![window, setContentSize: NSSize::new(width, height)];
    let _: () = msg_send![window, setFrameTopLeftPoint: NSPoint::new(x, main_height - y)];
}

/// Place the panel and show it as key without activating the app.
///
/// # Safety
/// As for [`adopt`].
pub unsafe fn present(ns_window: *mut AnyObject, x: f64, y: f64, width: f64, height: f64) {
    place(ns_window, x, y, width, height);
    let _: () = msg_send![ns_window, makeKeyAndOrderFront: std::ptr::null::<AnyObject>()];
}
