pub mod app;
pub mod log_layer;
pub mod ui;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;
use tokio::sync::{mpsc, watch};
use voxmux_core::tui_types::{RouterState, UiCommand};

pub use app::App;
pub use log_layer::TuiLogLayer;

/// Run the TUI event loop. Blocks until the user quits.
pub async fn run(
    mut state_rx: watch::Receiver<RouterState>,
    cmd_tx: mpsc::UnboundedSender<UiCommand>,
    log_buffer: Arc<Mutex<VecDeque<String>>>,
) -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let result = run_loop(&mut terminal, &mut state_rx, &cmd_tx, &log_buffer).await;
    ratatui::restore();
    result
}

async fn run_loop(
    terminal: &mut DefaultTerminal,
    state_rx: &mut watch::Receiver<RouterState>,
    cmd_tx: &mpsc::UnboundedSender<UiCommand>,
    log_buffer: &Arc<Mutex<VecDeque<String>>>,
) -> std::io::Result<()> {
    let mut app = App::new(Arc::clone(log_buffer));

    loop {
        // Update state from watch channel
        if state_rx.has_changed().unwrap_or(false) {
            app.update_state(state_rx.borrow_and_update().clone());
        }

        terminal.draw(|frame| ui::draw(frame, &app))?;

        // Poll for events with a short timeout so we can re-render on state changes
        if event::poll(std::time::Duration::from_millis(33))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let action = app.handle_key(key);
                    match action {
                        app::AppAction::Quit => {
                            let _ = cmd_tx.send(UiCommand::Quit);
                            break;
                        }
                        app::AppAction::Command(cmd) => {
                            let _ = cmd_tx.send(cmd);
                        }
                        app::AppAction::None => {}
                    }
                }
            }
        }
    }

    Ok(())
}
