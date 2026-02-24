use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use voxmux_core::tui_types::{InputState, OutputState, RouterState};
use voxmux_tui::app::{App, Tab};
use voxmux_tui::ui;

fn buffer_text(buf: &ratatui::buffer::Buffer) -> String {
    let area = buf.area();
    let mut text = String::new();
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            text.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
        }
        text.push('\n');
    }
    text
}

#[test]
fn test_full_draw_cycle() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let logs = Arc::new(Mutex::new(VecDeque::new()));
    {
        let mut buf = logs.lock().unwrap();
        buf.push_back("[INFO] test: startup".to_string());
    }

    let mut app = App::new(Arc::clone(&logs));
    app.update_state(RouterState {
        inputs: vec![InputState {
            id: "mic1".into(),
            device_name: "Test Mic".into(),
            enabled: true,
            volume: 0.75,
            muted: false,
            peak_level: 0.4,
        }],
        output: OutputState {
            device_name: "Test Speakers".into(),
            play_mixed_input: true,
        },
        latest_recognitions: vec!["hello world".to_string()],
        is_running: true,
    });

    // Draw all 4 tabs — no panics
    for tab in &[Tab::Dashboard, Tab::Inputs, Tab::Outputs, Tab::Logs] {
        app.tab = *tab;
        terminal
            .draw(|frame| ui::draw(frame, &app))
            .unwrap();
    }
}

#[test]
fn test_state_watch_updates_render() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(Arc::new(Mutex::new(VecDeque::new())));
    app.tab = Tab::Inputs;

    // Initial render: no inputs
    terminal
        .draw(|frame| ui::draw(frame, &app))
        .unwrap();
    let text = buffer_text(terminal.backend().buffer());
    assert!(!text.contains("NewDevice"), "should not contain NewDevice yet");

    // Simulate watch update: add 2 inputs
    app.update_state(RouterState {
        inputs: vec![
            InputState {
                id: "new1".into(),
                device_name: "NewDevice1".into(),
                enabled: true,
                volume: 0.5,
                ..Default::default()
            },
            InputState {
                id: "new2".into(),
                device_name: "NewDevice2".into(),
                enabled: true,
                volume: 0.8,
                ..Default::default()
            },
        ],
        ..Default::default()
    });

    // Re-render should show updated inputs
    terminal
        .draw(|frame| ui::draw(frame, &app))
        .unwrap();
    let text = buffer_text(terminal.backend().buffer());
    assert!(text.contains("NewDevice1"), "expected NewDevice1:\n{}", text);
    assert!(text.contains("NewDevice2"), "expected NewDevice2:\n{}", text);
}
