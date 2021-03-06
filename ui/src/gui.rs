use glium::glutin::event::{Event, StartCause, WindowEvent};
use glium::glutin::event_loop::{ControlFlow, EventLoop};
use glium::glutin::{window::WindowBuilder, ContextBuilder};
use glium::Surface;
use imgui::{Context, FontSource};
use imgui_glium_renderer::Renderer;
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use send_wrapper::SendWrapper;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Instant;

use crate::clipboard_compat::*;
use crate::gridview;
use crate::input;
use crate::windows;
use crate::CONFIG;

lazy_static! {
    static ref EVENT_LOOP: SendWrapper<RefCell<Option<EventLoop<()>>>> =
        SendWrapper::new(RefCell::new(Some(EventLoop::new())));
    pub static ref DISPLAY: SendWrapper<glium::Display> = SendWrapper::new({
        let wb = WindowBuilder::new().with_title(super::TITLE.to_owned());
        let cb = ContextBuilder::new().with_vsync(true);
        glium::Display::new(wb, cb, EVENT_LOOP.borrow().as_ref().unwrap())
            .expect("Failed to initialize display")
    });
}

/// Display the main application window.
pub fn show_gui() -> ! {
    let display = &**DISPLAY;

    // Initialize runtime data.
    let mut gridview = crate::make_default_gridview(crate::DEFAULT_NDIM);
    let mut main_window = windows::MainWindow::default();
    let mut input_state = input::State::default();
    let mut events_buffer = VecDeque::new();

    // Initialize imgui.
    let mut imgui = Context::create();
    imgui.set_clipboard_backend(Box::new(ClipboardCompat));
    imgui.set_ini_filename(None);
    let mut platform = WinitPlatform::init(&mut imgui);
    let gl_window = display.gl_window();
    let window = gl_window.window();
    // Imgui DPI handling is a mess.
    platform.attach_window(imgui.io_mut(), window, HiDpiMode::Default);

    // Initialize imgui fonts.
    let font_size = CONFIG.lock().gfx.font_size as f32;
    imgui.fonts().add_font(&[FontSource::TtfData {
        data: include_bytes!("../resources/font/NotoSans-Regular.ttf"),
        size_pixels: font_size,
        config: None,
    }]);

    // Initialize imgui renderer.
    let mut renderer = Renderer::init(&mut imgui, display).expect("Failed to initialize renderer");

    // Main loop
    let mut last_frame_time = Instant::now();
    let mut next_frame_time = Instant::now();
    EVENT_LOOP
        .borrow_mut()
        .take()
        .unwrap()
        .run(move |event, _ev_loop, control_flow| {
            // Handle events.
            let mut now = Instant::now();
            let mut do_frame = false;
            match event.to_static() {
                Some(Event::NewEvents(cause)) => match cause {
                    StartCause::ResumeTimeReached {
                        start: _,
                        requested_resume,
                    } => {
                        now = requested_resume;
                        do_frame = true;
                    }
                    StartCause::Init => {
                        next_frame_time = now;
                        do_frame = true;
                    }
                    _ => (),
                },

                // The program is about to exit.
                Some(Event::LoopDestroyed) => (),

                // Queue the event to be handled next time we render
                // everything.
                Some(ev) => events_buffer.push_back(ev),

                // Ignore this event.
                None => (),
            };

            if do_frame && next_frame_time <= now {
                next_frame_time = now + CONFIG.lock().gfx.frame_duration();
                if next_frame_time < Instant::now() {
                    // Skip a frame (or several).
                    next_frame_time = Instant::now() + CONFIG.lock().gfx.frame_duration();
                }
                *control_flow = ControlFlow::WaitUntil(next_frame_time);

                // Prep imgui for event handling.
                let imgui_io = imgui.io_mut();
                platform
                    .prepare_frame(imgui_io, gl_window.window())
                    .expect("Failed to start frame");

                if let Some(delta) = now.checked_duration_since(last_frame_time) {
                    imgui_io.update_delta_time(delta);
                }
                last_frame_time = now;

                // Prep the gridview for event handling.
                let mut input_frame = input_state.frame(&gridview, &imgui_io, &platform);

                for ev in events_buffer.drain(..) {
                    // Let imgui handle events.
                    platform.handle_event(imgui_io, gl_window.window(), &ev);
                    // Handle events for the gridview.
                    input_frame.handle_event(&ev);
                    // Handle events ourself.
                    match ev {
                        Event::WindowEvent { event, .. } => match event {
                            // Handle window close event.
                            WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                            _ => (),
                        },
                        _ => (),
                    }
                }

                // Finish handling events for the gridview.
                input_frame.finish();

                // Prep imgui for rendering.
                let imgui_has_mouse = imgui_io.want_capture_mouse;
                let ui = imgui.frame();
                main_window.build(&mut windows::BuildParams {
                    ui: &ui,
                    mouse: input_state.mouse(),
                    gridview: &mut gridview,
                });
                if !imgui_has_mouse {
                    ui.set_mouse_cursor(input_state.mouse().display.cursor_icon());
                }

                let mut target = display.draw();

                // Execute commands and run the simulation.
                gridview.do_frame().expect("Unhandled exception!");

                if target.get_dimensions() != (0, 0) {
                    // Render the gridview.
                    gridview
                        .render(gridview::RenderParams {
                            target: &mut target,
                            mouse: input_state.mouse(),
                            modifiers: input_state.modifiers(),
                        })
                        .expect("Unhandled exception!");

                    // Render imgui.
                    platform.prepare_render(&ui, gl_window.window());
                    let draw_data = ui.render();
                    renderer
                        .render(&mut target, draw_data)
                        .expect("Rendering failed");
                }
                // Put it all on the screen.
                target.finish().expect("Failed to swap buffers");

                // Clean render cache.
                super::gridview::render::post_frame_clean_cache();
            }
        })
}
