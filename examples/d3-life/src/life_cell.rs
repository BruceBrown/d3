use super::*;

// SDL2 requires that drawing occur on the main thread. In order
// to accomodate that we'll set up a channel and give the sender
// to each machine.
#[derive(Debug, MachineImpl)]
pub enum GraphicsCmd {
    RenderRect(Rect, Color),
}
pub type GraphicsSender = machine_impl::Sender<GraphicsCmd>;
pub type GraphicsReceiver = machine_impl::Receiver<GraphicsCmd>;

// These are the commands a cell understands
#[derive(Debug, MachineImpl, Clone)]
pub enum LifeCmd {
    /// Tick instructs everyone to compute their livliness
    Tick,
    /// Tock instructs everyone to send their livliness
    Tock,
    /// Edit the state of a cell
    SetState(CellState),
    /// Notification that a neighbor just came to life
    NeighborBorn,
    /// Notification that a neighbor just died
    NeighborDied,
    /// Add a neighbor for sending neighbor notification
    AddNeighbor(LifeSender),
    /// Remove all of the neigbors
    RemoveNeighbors,
}

// These are the cell states
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CellState {
    Dead = 0,
    Alive = 1,
}
pub type LifeSender = machine_impl::Sender<LifeCmd>;

// This is mutable cell information
struct MutableLifeCell {
    id: usize,
    previous_state: CellState,
    state: CellState,
    neighbors_alive: i8,
    neighbors: Vec<LifeSender>,
    rect: Rect,
    renderer: GraphicsSender,
}
impl MutableLifeCell {
    fn new(id: usize, graphics_sender: GraphicsSender, rect: Rect) -> Self {
        Self {
            id,
            previous_state: CellState::Dead,
            state: CellState::Dead,
            neighbors_alive: 0,
            neighbors: Vec::with_capacity(8),
            rect,
            renderer: graphics_sender,
        }
    }
    // setting state is an editing function
    pub fn set_state(&mut self, state: CellState) {
        self.state = state;
        self.neighbors_alive = 0;
        self.previous_state = CellState::Dead;
        log::debug!(
            "id={}, rc=({}, {}) state={:#?}",
            self.id,
            self.id / GRID_SIZE,
            self.id % GRID_SIZE,
            state
        );
    }

    fn send_state(&self) {
        if self.state == self.previous_state {
            return;
        }
        log::debug!(
            "id={}, rc=({}, {}) previous_state={:#?} state={:#?}",
            self.id,
            self.id / GRID_SIZE,
            self.id % GRID_SIZE,
            self.previous_state,
            self.state
        );
        let cmd = if self.state == CellState::Alive {
            log::trace!("id: {} sending NeighborBorn", self.id);
            LifeCmd::NeighborBorn
        } else {
            log::trace!("id: {} sending NeighborDied", self.id);
            LifeCmd::NeighborDied
        };
        self.neighbors.iter().for_each(|s| s.send(cmd.clone()).unwrap());
        self.render(true);
    }

    fn compute_state(&mut self) {
        log::debug!(
            "id={}, rc=({}, {}) neighbors_alive={}",
            self.id,
            self.id / GRID_SIZE,
            self.id % GRID_SIZE,
            self.neighbors_alive
        );
        self.previous_state = self.state;

        self.state = match (self.state, self.neighbors_alive) {
            (CellState::Alive, x) if x < 2 => CellState::Dead,
            (CellState::Alive, 2) | (CellState::Alive, 3) => CellState::Alive,
            (CellState::Alive, x) if x > 3 => CellState::Dead,
            (CellState::Dead, 3) => CellState::Alive,
            (otherwise, _) => otherwise,
        };
        if self.previous_state != self.state {
            log::debug!(
                "id={}, rc=({}, {}) changed, state={:#?}",
                self.id,
                self.id / GRID_SIZE,
                self.id % GRID_SIZE,
                self.state
            );
        }
    }

    fn neighbor_born(&mut self) { self.neighbors_alive += 1; }

    fn neighbor_died(&mut self) { self.neighbors_alive -= 1; }

    fn render(&self, full: bool) {
        // same as it ever was...
        if full || self.state != self.previous_state {
            // the dead color must be black
            let color = match self.state {
                CellState::Alive => Color::RGB(255, 210, 0),
                CellState::Dead => Color::RGB(0, 0, 0),
            };

            // tell the graphics machine to draw
            self.renderer.send(GraphicsCmd::RenderRect(self.rect, color)).unwrap();
        }
    }
}

pub struct LifeCell {
    mutable: Mutex<MutableLifeCell>,
}
impl LifeCell {
    pub fn new(id: usize, graphics_sender: GraphicsSender, rect: Rect) -> Self {
        Self {
            mutable: Mutex::new(MutableLifeCell::new(id, graphics_sender, rect)),
        }
    }

    pub fn tick(&self) { self.mutable.lock().compute_state(); }

    pub fn tock(&self) { self.mutable.lock().send_state(); }
}

// The machine implementation
impl Machine<LifeCmd> for LifeCell {
    fn receive(&self, cmd: LifeCmd) {
        match cmd {
            LifeCmd::Tick => self.tick(),
            LifeCmd::Tock => self.tock(),
            LifeCmd::SetState(state) => self.mutable.lock().set_state(state),
            LifeCmd::NeighborBorn => self.mutable.lock().neighbor_born(),
            LifeCmd::NeighborDied => self.mutable.lock().neighbor_died(),
            LifeCmd::AddNeighbor(sender) => self.mutable.lock().neighbors.push(sender),
            LifeCmd::RemoveNeighbors => self.mutable.lock().neighbors.clear(),
        }
    }
}

// initialize cells and start the server
pub fn init_cells() -> (GraphicsReceiver, Vec<LifeSender>) {
    // configure the core and get it running
    executor::set_default_channel_capacity(10);
    executor::set_machine_count_estimate(GRID_SIZE + 10);
    executor::start_server();

    let (graphics_sender, graphics_receiver) = machine_impl::channel::<GraphicsCmd>();
    let mut cells: Vec<LifeSender> = Vec::with_capacity(GRID_SIZE * GRID_SIZE);

    // create all of the machines
    for idx in 0 .. GRID_SIZE * GRID_SIZE {
        let row = (idx / GRID_SIZE) as u32;
        let col = (idx % GRID_SIZE) as u32;
        let rect = Rect::new(
            (BORDER_SIZE + (RECT_SIZE + 1) * col) as i32,
            (BORDER_SIZE + (RECT_SIZE + 1) * row) as i32,
            RECT_SIZE,
            RECT_SIZE,
        );
        let (_, s) = executor::connect_unbounded(LifeCell::new(idx, graphics_sender.clone(), rect));
        cells.push(s);
    }
    // wait for them to all get assigned
    let target = GRID_SIZE * GRID_SIZE;
    loop {
        if target == executor::get_machine_count() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_nanos(50));
    }
    log::info!("created {} machines", executor::get_machine_count());

    // wire the neighbors together
    cells.iter().enumerate().for_each(|(idx, cell)| {
        // each cell has 8 neighbors, go find them
        let row = idx / GRID_SIZE;
        let col = idx % GRID_SIZE;
        for dr in [GRID_SIZE - 1, 0, 1].iter().cloned() {
            for dc in [GRID_SIZE - 1, 0, 1].iter().cloned() {
                sdl2::rect::Rect::new(0, 0, RECT_SIZE, RECT_SIZE);
                if dr == 0 && dc == 0 {
                    continue;
                }
                let nr = (row + dr) % GRID_SIZE;
                let nc = (col + dc) % GRID_SIZE;
                let n = nr * GRID_SIZE + nc;
                cell.send(LifeCmd::AddNeighbor(cells[n].clone())).unwrap();
            }
        }
    });
    (graphics_receiver, cells)
}

pub fn set_cells(cells: &[LifeSender], alive: &[(usize, usize)]) {
    for (row, col) in alive.iter().cloned() {
        let idx = row * GRID_SIZE + col;
        log::debug!("sending SetAlive({}) to id={} rc=({}, {})", true, idx, row, col);
        cells[idx].send(LifeCmd::SetState(CellState::Alive)).unwrap();
    }
    sync();
    // publish changes to neighbors
    cells.iter().for_each(|s| s.send(LifeCmd::Tock).unwrap());
    sync();
}

// sync ensures message processing is complete before advancing
// There is a feedback loop, which has to complete before advancing
fn sync() {
    std::thread::sleep(std::time::Duration::from_nanos(50));
    while executor::get_run_queue_len() > 0 {
        std::thread::sleep(std::time::Duration::from_nanos(50));
    }
}

// compute state, render it, then publish changes to the neighbors
pub fn tick_tock(cells: &[LifeSender]) {
    // compute state and render changes
    cells.iter().for_each(|s| s.send(LifeCmd::Tick).unwrap());
    sync();
    // publish changes to neighbors
    cells.iter().for_each(|s| s.send(LifeCmd::Tock).unwrap());
    sync();
}

// drop everything and shut down the server
pub fn finish(cells: Vec<LifeSender>) {
    cells.iter().for_each(|s| s.send(LifeCmd::RemoveNeighbors).unwrap());
    sync();
    drop(cells);
    sync();
    stop_server();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spaceship() {
        CombinedLogger::init(vec![TermLogger::new(LevelFilter::Warn, Config::default(), TerminalMode::Mixed)]).unwrap();

        let (_graphics_receiver, cells) = init_cells();
        set_cells(&cells, &[(1, 2), (2, 3), (3, 1), (3, 2), (3, 3)]);
        // next: universe.set_cells(&[(2,1), (2,3), (3,2), (3,3), (4,2)]);

        tick_tock(&cells);

        std::thread::sleep(std::time::Duration::from_millis(100));
        cells.iter().for_each(|s| s.send(LifeCmd::RemoveNeighbors).unwrap());
        sync();
        drop(cells);
        sync();
        stop_server();
    }
}
