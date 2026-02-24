use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{App, Tab};

pub fn draw(frame: &mut Frame, app: &App) {
    let [tabs_area, main_area] =
        Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(frame.area());

    draw_tabs(frame, app, tabs_area);

    match app.tab {
        Tab::Dashboard => draw_dashboard(frame, app, main_area),
        Tab::Inputs => draw_inputs(frame, app, main_area),
        Tab::Outputs => draw_outputs(frame, app, main_area),
        Tab::Logs => draw_logs(frame, app, main_area),
    }
}

fn draw_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let titles = vec!["1:Dashboard", "2:Inputs", "3:Outputs", "4:Logs"];
    let selected = match app.tab {
        Tab::Dashboard => 0,
        Tab::Inputs => 1,
        Tab::Outputs => 2,
        Tab::Logs => 3,
    };
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("voxmux"))
        .select(selected)
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, area);
}

fn draw_dashboard(frame: &mut Frame, app: &App, area: Rect) {
    if app.state.inputs.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Dashboard");
        let para = Paragraph::new("No inputs configured").block(block);
        frame.render_widget(para, area);
        return;
    }

    let constraints: Vec<Constraint> = app
        .state
        .inputs
        .iter()
        .map(|_| Constraint::Length(2))
        .chain(std::iter::once(Constraint::Fill(1)))
        .collect();

    let areas = Layout::vertical(constraints).split(area);

    for (i, input) in app.state.inputs.iter().enumerate() {
        let label = format!(
            "{} {}",
            input.id,
            if input.muted { "[M]" } else { "" }
        );
        let ratio = input.peak_level.clamp(0.0, 1.0) as f64;
        let gauge = Gauge::default()
            .block(Block::default().title(label))
            .gauge_style(Style::default().fg(if input.muted { Color::DarkGray } else { Color::Green }))
            .ratio(ratio);
        frame.render_widget(gauge, areas[i]);
    }

    // Remaining area: recent recognitions
    let last = areas.len() - 1;
    let recog_items: Vec<ListItem> = app
        .state
        .latest_recognitions
        .iter()
        .rev()
        .take(10)
        .map(|s| ListItem::new(s.as_str()))
        .collect();
    let recog_list = List::new(recog_items)
        .block(Block::default().borders(Borders::ALL).title("Recent ASR"));
    frame.render_widget(recog_list, areas[last]);
}

fn draw_inputs(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .state
        .inputs
        .iter()
        .enumerate()
        .map(|(i, input)| {
            let marker = if i == app.selected_input { ">" } else { " " };
            let mute_str = if input.muted { " [MUTED]" } else { "" };
            let enabled_str = if input.enabled { "" } else { " (disabled)" };
            let line = Line::from(vec![
                Span::raw(format!("{} ", marker)),
                Span::styled(
                    &input.device_name,
                    if i == app.selected_input {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                ),
                Span::raw(format!(
                    "  vol:{:.0}%{}{}",
                    input.volume * 100.0,
                    mute_str,
                    enabled_str,
                )),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Inputs (Up/Down=select, Left/Right=vol, m=mute, e=enable)"),
    );
    frame.render_widget(list, area);
}

fn draw_outputs(frame: &mut Frame, app: &App, area: Rect) {
    let play_str = if app.state.output.play_mixed_input {
        "ON"
    } else {
        "OFF"
    };
    let text = format!(
        "Output device: {}\nPlay mixed input: {} (Space to toggle)",
        app.state.output.device_name, play_str,
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Output");
    let para = Paragraph::new(text).block(block);
    frame.render_widget(para, area);
}

fn draw_logs(frame: &mut Frame, app: &App, area: Rect) {
    let logs = app.logs.lock().unwrap();
    let total = logs.len();

    let visible_height = area.height.saturating_sub(2) as usize; // account for borders
    let scroll = app.log_scroll.min(total.saturating_sub(visible_height));
    let end = total.saturating_sub(scroll);
    let start = end.saturating_sub(visible_height);

    let items: Vec<ListItem> = logs
        .iter()
        .skip(start)
        .take(end - start)
        .map(|s| ListItem::new(s.as_str()))
        .collect();

    let title = if app.log_auto_scroll {
        "Logs (auto-scroll)"
    } else {
        "Logs (Up/Down=scroll, G=bottom)"
    };
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(list, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use ratatui::buffer::Buffer;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use voxmux_core::tui_types::{InputState, RouterState};

    fn buffer_text(buf: &Buffer) -> String {
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
    fn test_dashboard_renders_vu_meters() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(Arc::new(Mutex::new(VecDeque::new())));
        app.update_state(RouterState {
            inputs: vec![
                InputState {
                    id: "mic1".into(),
                    device_name: "USB Mic".into(),
                    enabled: true,
                    volume: 0.8,
                    muted: false,
                    peak_level: 0.6,
                },
                InputState {
                    id: "mic2".into(),
                    device_name: "Line In".into(),
                    enabled: true,
                    volume: 0.5,
                    muted: false,
                    peak_level: 0.3,
                },
            ],
            ..Default::default()
        });
        app.tab = Tab::Dashboard;

        terminal
            .draw(|frame| draw(frame, &app))
            .unwrap();

        let text = buffer_text(terminal.backend().buffer());
        // Gauge renders block chars for the filled portion
        assert!(
            text.contains("mic1") && text.contains("mic2"),
            "expected both input ids in dashboard, got:\n{}",
            text,
        );
    }

    #[test]
    fn test_inputs_tab_renders_device_list() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(Arc::new(Mutex::new(VecDeque::new())));
        app.update_state(RouterState {
            inputs: vec![
                InputState {
                    id: "a".into(),
                    device_name: "DeviceAlpha".into(),
                    ..Default::default()
                },
                InputState {
                    id: "b".into(),
                    device_name: "DeviceBeta".into(),
                    ..Default::default()
                },
                InputState {
                    id: "c".into(),
                    device_name: "DeviceGamma".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        app.tab = Tab::Inputs;

        terminal
            .draw(|frame| draw(frame, &app))
            .unwrap();

        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("DeviceAlpha"), "missing DeviceAlpha:\n{}", text);
        assert!(text.contains("DeviceBeta"), "missing DeviceBeta:\n{}", text);
        assert!(text.contains("DeviceGamma"), "missing DeviceGamma:\n{}", text);
    }

    #[test]
    fn test_logs_tab_renders_log_lines() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let logs = Arc::new(Mutex::new(VecDeque::new()));
        {
            let mut buf = logs.lock().unwrap();
            for i in 0..10 {
                buf.push_back(format!("[INFO] test: log message {}", i));
            }
        }

        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(Arc::clone(&logs));
        app.tab = Tab::Logs;

        terminal
            .draw(|frame| draw(frame, &app))
            .unwrap();

        let text = buffer_text(terminal.backend().buffer());
        assert!(
            text.contains("log message"),
            "expected log text in output:\n{}",
            text,
        );
    }
}
