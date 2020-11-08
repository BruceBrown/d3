use std::thread;
use std::time::{Duration, Instant};

use d3::core::executor::*;
use d3::core::machine_impl::*;
use d3::d3_derive::*;

use crossbeam::channel::RecvTimeoutError;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::WindowCanvas;

use parking_lot::Mutex;
use rand::distributions::{Distribution, Uniform};
use simplelog::*;

const GRID_SIZE: usize = 128;
const RECT_SIZE: u32 = 6;
const BORDER_SIZE: u32 = 2;
const WINDOW_SIZE: u32 = BORDER_SIZE * 2 + (GRID_SIZE as u32) * (RECT_SIZE + 1);

mod life_cell;
use life_cell::*;

// Cells send a color, this translates it to a different color
struct ColorMux {
    alive: Color,
    dead: Color,
}
impl ColorMux {
    fn translate_color(&self, color: Color) -> Color {
        if color == Color::RGB(0, 0, 0) {
            self.dead
        } else {
            self.alive
        }
    }
}

// Random state generator for alive cells
struct RandomCells {
    limit: usize,
    rng: rand::rngs::OsRng,
    range: Uniform<usize>,
}
impl RandomCells {
    fn new(limit: usize, cell_count: usize) -> Self {
        Self {
            limit,
            rng: rand::rngs::OsRng::default(),
            range: Uniform::from(0 .. cell_count),
        }
    }
    // generate cells that are either alive or dead
    fn reset_cells(&mut self, cells: &[LifeSender]) {
        log::info!("regenerating");
        // give things a chance to quiet down
        thread::sleep(Duration::from_millis(50));
        let mut cell_states: Vec<CellState> = Vec::with_capacity(cells.len());
        for _ in 0 .. cells.len() {
            cell_states.push(CellState::Dead)
        }
        for _ in 0 .. self.limit {
            cell_states[self.range.sample(&mut self.rng)] = CellState::Alive;
        }
        cells
            .iter()
            .zip(cell_states.iter())
            .for_each(|(sender, state)| sender.send(LifeCmd::SetState(*state)).unwrap());
        // calling set_cells with no changes ensures we're synchronized
        set_cells(cells, Vec::<(usize, usize)>::new().as_slice());
        // give things a chance to quiet down
        thread::sleep(Duration::from_millis(50));
    }
}

// clear the canvas and generate a new set of alive and dead cells
fn reset_cells(cells: &[LifeSender], random_cells: &mut RandomCells, canvas: &mut WindowCanvas) {
    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();
    canvas.present();
    random_cells.reset_cells(cells);
}

// change the distribution of alive and dead cells
fn change_distribution(percent_pop: usize, canvas: &mut WindowCanvas) -> RandomCells {
    let title = format!("rust-d3-life demo {}x{} {}%", GRID_SIZE, GRID_SIZE, percent_pop);
    canvas.window_mut().set_title(&title).unwrap();
    let limit = (GRID_SIZE * GRID_SIZE) * percent_pop / 100;
    RandomCells::new(limit, GRID_SIZE * GRID_SIZE)
}

fn main() {
    // setup logging
    CombinedLogger::init(vec![TermLogger::new(LevelFilter::Info, Config::default(), TerminalMode::Mixed)]).unwrap();

    // setup the color mux
    let mut color_mux = ColorMux {
        alive: Color::RGB(255, 210, 0),
        dead: Color::BLACK,
    };

    // setup the random cell generator
    let mut percent_pop = 25;
    let limit = (GRID_SIZE * GRID_SIZE) * percent_pop / 100;
    let mut random_cells = RandomCells::new(limit, GRID_SIZE * GRID_SIZE);

    // init the cells
    let (graphics_receiver, cells) = init_cells();

    // init sdl2
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
    let title = format!("rust-d3-life demo {}x{} {}%", GRID_SIZE, GRID_SIZE, percent_pop);
    let window = video_subsystem
        .window(&title, WINDOW_SIZE, WINDOW_SIZE)
        .position_centered()
        .build()
        .unwrap();
    let mut canvas = window.into_canvas().software().build().unwrap();

    // clear the canvas
    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();
    canvas.present();

    // pick some cells to be alive
    random_cells.reset_cells(&cells);

    // This is the game loop
    let mut event_pump = sdl_context.event_pump().unwrap();
    let mut start = Instant::now();
    'running: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,
                Event::KeyDown {
                    keycode: Some(Keycode::Space),
                    ..
                } => {
                    // space start over again
                    reset_cells(&cells, &mut random_cells, &mut canvas);
                },
                Event::KeyDown {
                    keycode: Some(Keycode::Y), ..
                } => {
                    // y is for yellow cells
                    color_mux.alive = Color::RGB(255, 210, 0);
                    reset_cells(&cells, &mut random_cells, &mut canvas);
                },
                Event::KeyDown {
                    keycode: Some(Keycode::G), ..
                } => {
                    // g is for green cells
                    color_mux.alive = Color::GREEN;
                    reset_cells(&cells, &mut random_cells, &mut canvas);
                },
                Event::KeyDown {
                    keycode: Some(Keycode::KpPlus),
                    ..
                } => {
                    // keypad + bumps up the alive population
                    percent_pop += 1;
                    random_cells = change_distribution(percent_pop, &mut canvas);
                    reset_cells(&cells, &mut random_cells, &mut canvas);
                },
                Event::KeyDown {
                    keycode: Some(Keycode::KpMinus),
                    ..
                } => {
                    // keypad - bumps down the alive population
                    percent_pop -= 1;
                    random_cells = change_distribution(percent_pop, &mut canvas);
                    reset_cells(&cells, &mut random_cells, &mut canvas);
                },

                _ => {},
            }
        }
        // The rest of the game loop goes here...
        tick_tock(&cells);
        loop {
            if start.elapsed() >= Duration::from_millis(250) {
                start += Duration::from_millis(250);
                break;
            }
            let wait_duration = Duration::from_millis(250) - start.elapsed();
            match graphics_receiver.recv_timeout(wait_duration) {
                Ok(cmd) => match cmd {
                    GraphicsCmd::RenderRect(rect, color) => {
                        canvas.set_draw_color(color_mux.translate_color(color));
                        canvas.fill_rect(rect).unwrap();
                    },
                },
                Err(RecvTimeoutError::Timeout) => (),
                Err(RecvTimeoutError::Disconnected) => (),
            }
        }
        canvas.present();
    }
    finish(cells);
}
