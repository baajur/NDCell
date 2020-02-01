use imgui::*;
use num::ToPrimitive;
use ref_thread_local::RefThreadLocal;

use crate::automaton::NdSimulate;
use crate::ui::State;

ref_thread_local! {
    pub static managed VISIBLE: bool = false;
}

#[derive(Default)]
pub struct WindowState {
    pub running: bool,
    jump_to_gen: isize,
}

/// Builds the main window.
pub fn build(state: &mut State, ui: &imgui::Ui) {
    if *VISIBLE.borrow() {
        Window::new(&ImString::new("Simulation")).build(&ui, || {
            let mut width = ui.window_content_region_width();
            if width < 100.0 {
                width = 200.0;
            }
            if ui.button(im_str!("Step 1 generation"), [width, 40.0]) {
                state.step_no_cache_clear(&1.into(), true);
            };
            ui.spacing();
            ui.spacing();
            ui.separator();
            ui.spacing();
            ui.spacing();
            let old_sim_step_size = &state.step_size;
            let mut sim_step_size = old_sim_step_size.to_i32().unwrap();
            ui.input_int(im_str!("Sim step"), &mut sim_step_size)
                .step(16)
                .step_fast(256)
                .build();
            if sim_step_size <= 0 {
                sim_step_size = 1;
            }
            if old_sim_step_size.to_i32().unwrap() != sim_step_size {
                state.step_size = sim_step_size.into();
            }
            if ui.button(
                &ImString::new(format!("Step {} generations", sim_step_size)),
                [width, 40.0],
            ) {
                state.step_step_size(true);
            }
            ui.spacing();
            ui.spacing();
            {
                if ui.button(
                    if state.input_state.is_running {
                        im_str!("Stop")
                    } else {
                        im_str!("Start")
                    },
                    [width, 60.0],
                ) {
                    state.toggle_running();
                }
            }
            ui.spacing();
            ui.spacing();
            ui.separator();
            ui.spacing();
            ui.spacing();
            let jump_to_gen = &mut state.gui.simulation.jump_to_gen;
            let mut jump_to_gen_i32 = *jump_to_gen as i32;
            ui.input_int(im_str!("Jump to"), &mut jump_to_gen_i32)
                .step(16)
                .step_fast(256)
                .build();
            *jump_to_gen = jump_to_gen_i32 as isize;
            if *jump_to_gen <= state.grid_view.get_generation_count().to_isize().unwrap() {
                *jump_to_gen = state.grid_view.get_generation_count().to_isize().unwrap();
            }
            if ui.button(
                &ImString::new(format!("Jump to generation {}", *jump_to_gen)),
                [width, 40.0],
            ) {
                if state.grid_view.get_generation_count().to_isize().unwrap() < *jump_to_gen {
                    let tmp_step_size =
                        *jump_to_gen - state.grid_view.get_generation_count().to_isize().unwrap();
                    state.step(&tmp_step_size.into(), true);
                }
            }
            ui.spacing();
            ui.spacing();
            ui.separator();
            ui.spacing();
            ui.spacing();
            let button_width = (width - 20.0) / 2.0;
            if ui.button(im_str!("Undo"), [button_width, 60.0]) {
                state.undo();
            }
            ui.same_line(button_width + 20.0);
            if ui.button(im_str!("Redo"), [button_width, 60.0]) {
                state.redo();
            }
            ui.spacing();
            ui.spacing();
            if ui.button(im_str!("Reset"), [width, 40.0]) {
                state.stop_running();
                state.reset();
            }
        })
    }
}
