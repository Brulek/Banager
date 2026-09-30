//! Whether tao's application delegate answers `applicationShouldTerminate:`.
//! It must not: `quit::guard_quitting` adds that method to the delegate's
//! class, and leaves the class as it is -- the guard off -- when the class
//! answers it already, itself or through a superclass
//! (`quit::add_should_terminate`). So a tao that starts to answer it turns
//! the question before quitting off; this fails first, for it to be looked
//! at.
//!
//! tao makes its event loop, and sets its delegate, only on the main
//! thread, so this is a test with its own `main` (`harness = false` in
//! Cargo.toml), which runs there. It makes the event loop and never runs
//! it: no window, nothing on screen.

#[cfg(target_os = "macos")]
fn main() {
    use objc2::runtime::AnyObject;
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use tauri_runtime_wry::tao::event_loop::EventLoop;

    let _event_loop = EventLoop::<()>::new();
    let mtm = MainThreadMarker::new().expect("a harness = false test runs on the main thread");
    let delegate = NSApplication::sharedApplication(mtm)
        .delegate()
        .expect("tao sets the application delegate as it makes its event loop");
    let class = AsRef::<AnyObject>::as_ref(&*delegate).class();
    assert!(
        class
            .instance_method(objc2::sel!(applicationShouldTerminate:))
            .is_none(),
        "tao's delegate class {:?} answers applicationShouldTerminate: now, so Banager's \
         question before quitting stays off (quit::add_should_terminate): look at what tao \
         does there",
        class.name()
    );
    // In libtest's words, so that it is counted with the rest.
    println!("\nrunning 1 test");
    println!(
        "test tao_delegate ({:?} does not answer applicationShouldTerminate:) ... ok",
        class.name()
    );
    println!("\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n");
}

#[cfg(not(target_os = "macos"))]
fn main() {}
