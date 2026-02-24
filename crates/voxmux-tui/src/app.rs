use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent};
use voxmux_core::tui_types::{RouterState, UiCommand};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Dashboard,
    Inputs,
    Outputs,
    Logs,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppAction {
    None,
    Quit,
    Command(UiCommand),
}

pub struct App {
    pub tab: Tab,
    pub state: RouterState,
    pub selected_input: usize,
    pub should_quit: bool,
    pub logs: Arc<Mutex<VecDeque<String>>>,
    pub log_scroll: usize,
    pub log_auto_scroll: bool,
}

impl App {
    pub fn new(logs: Arc<Mutex<VecDeque<String>>>) -> Self {
        Self {
            tab: Tab::Dashboard,
            state: RouterState::default(),
            selected_input: 0,
            should_quit: false,
            logs,
            log_scroll: 0,
            log_auto_scroll: true,
        }
    }

    pub fn update_state(&mut self, new_state: RouterState) {
        self.state = new_state;
        // Clamp selected_input to valid range
        if !self.state.inputs.is_empty() && self.selected_input >= self.state.inputs.len() {
            self.selected_input = self.state.inputs.len() - 1;
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        // Global keys
        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                return AppAction::Quit;
            }
            KeyCode::Char('1') => {
                self.tab = Tab::Dashboard;
                return AppAction::None;
            }
            KeyCode::Char('2') => {
                self.tab = Tab::Inputs;
                return AppAction::None;
            }
            KeyCode::Char('3') => {
                self.tab = Tab::Outputs;
                return AppAction::None;
            }
            KeyCode::Char('4') => {
                self.tab = Tab::Logs;
                return AppAction::None;
            }
            _ => {}
        }

        // Tab-specific keys
        match self.tab {
            Tab::Inputs => self.handle_inputs_key(key),
            Tab::Outputs => self.handle_outputs_key(key),
            Tab::Logs => self.handle_logs_key(key),
            Tab::Dashboard => AppAction::None,
        }
    }

    fn handle_inputs_key(&mut self, key: KeyEvent) -> AppAction {
        if self.state.inputs.is_empty() {
            return AppAction::None;
        }

        match key.code {
            KeyCode::Up => {
                if self.selected_input > 0 {
                    self.selected_input -= 1;
                }
                AppAction::None
            }
            KeyCode::Down => {
                if self.selected_input + 1 < self.state.inputs.len() {
                    self.selected_input += 1;
                }
                AppAction::None
            }
            KeyCode::Right => {
                let input = &self.state.inputs[self.selected_input];
                let new_vol = (input.volume + 0.05).min(1.0);
                AppAction::Command(UiCommand::SetVolume {
                    input_id: input.id.clone(),
                    volume: new_vol,
                })
            }
            KeyCode::Left => {
                let input = &self.state.inputs[self.selected_input];
                let new_vol = (input.volume - 0.05).max(0.0);
                AppAction::Command(UiCommand::SetVolume {
                    input_id: input.id.clone(),
                    volume: new_vol,
                })
            }
            KeyCode::Char('m') => {
                let input = &self.state.inputs[self.selected_input];
                AppAction::Command(UiCommand::SetMuted {
                    input_id: input.id.clone(),
                    muted: !input.muted,
                })
            }
            KeyCode::Char('e') => {
                let input = &self.state.inputs[self.selected_input];
                AppAction::Command(UiCommand::SetEnabled {
                    input_id: input.id.clone(),
                    enabled: !input.enabled,
                })
            }
            _ => AppAction::None,
        }
    }

    fn handle_outputs_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Char(' ') => AppAction::Command(UiCommand::SetPlayMixedInput(
                !self.state.output.play_mixed_input,
            )),
            _ => AppAction::None,
        }
    }

    fn handle_logs_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Up => {
                self.log_scroll = self.log_scroll.saturating_add(1);
                self.log_auto_scroll = false;
                AppAction::None
            }
            KeyCode::Down => {
                self.log_scroll = self.log_scroll.saturating_sub(1);
                AppAction::None
            }
            KeyCode::Char('G') => {
                self.log_scroll = 0;
                self.log_auto_scroll = true;
                AppAction::None
            }
            _ => AppAction::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use voxmux_core::tui_types::InputState;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn make_app() -> App {
        App::new(Arc::new(Mutex::new(VecDeque::new())))
    }

    fn make_app_with_inputs(inputs: Vec<InputState>) -> App {
        let mut app = make_app();
        app.update_state(RouterState {
            inputs,
            ..Default::default()
        });
        app
    }

    // ── Step 3: App struct + state ─────────────────────────────

    #[test]
    fn test_app_initial_state() {
        let app = make_app();
        assert_eq!(app.tab, Tab::Dashboard);
        assert_eq!(app.selected_input, 0);
        assert!(!app.should_quit);
        assert_eq!(app.log_scroll, 0);
        assert!(app.log_auto_scroll);
    }

    #[test]
    fn test_app_tab_switching() {
        let mut app = make_app();
        app.handle_key(key(KeyCode::Char('2')));
        assert_eq!(app.tab, Tab::Inputs);
        app.handle_key(key(KeyCode::Char('3')));
        assert_eq!(app.tab, Tab::Outputs);
        app.handle_key(key(KeyCode::Char('4')));
        assert_eq!(app.tab, Tab::Logs);
        app.handle_key(key(KeyCode::Char('1')));
        assert_eq!(app.tab, Tab::Dashboard);
    }

    #[test]
    fn test_app_state_update() {
        let mut app = make_app();
        let state = RouterState {
            inputs: vec![
                InputState {
                    id: "a".into(),
                    ..Default::default()
                },
                InputState {
                    id: "b".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        app.update_state(state);
        assert_eq!(app.state.inputs.len(), 2);
    }

    #[test]
    fn test_app_log_auto_scroll() {
        let mut app = make_app();
        app.tab = Tab::Logs;

        // Scroll up → auto_scroll = false
        app.handle_key(key(KeyCode::Up));
        assert!(!app.log_auto_scroll);
        assert_eq!(app.log_scroll, 1);

        // Press G → auto_scroll = true, scroll = 0
        app.handle_key(key(KeyCode::Char('G')));
        assert!(app.log_auto_scroll);
        assert_eq!(app.log_scroll, 0);
    }

    // ── Step 4: Key event handling ─────────────────────────────

    #[test]
    fn test_app_volume_up() {
        let mut app = make_app_with_inputs(vec![InputState {
            id: "mic1".into(),
            volume: 0.5,
            ..Default::default()
        }]);
        app.tab = Tab::Inputs;
        let action = app.handle_key(key(KeyCode::Right));
        assert_eq!(
            action,
            AppAction::Command(UiCommand::SetVolume {
                input_id: "mic1".into(),
                volume: 0.55,
            })
        );
    }

    #[test]
    fn test_app_volume_down() {
        let mut app = make_app_with_inputs(vec![InputState {
            id: "mic1".into(),
            volume: 0.5,
            ..Default::default()
        }]);
        app.tab = Tab::Inputs;
        let action = app.handle_key(key(KeyCode::Left));
        match action {
            AppAction::Command(UiCommand::SetVolume { input_id, volume }) => {
                assert_eq!(input_id, "mic1");
                assert!((volume - 0.45).abs() < 1e-5, "expected ~0.45, got {}", volume);
            }
            other => panic!("expected SetVolume command, got {:?}", other),
        }
    }

    #[test]
    fn test_app_volume_clamp() {
        // At max
        let mut app = make_app_with_inputs(vec![InputState {
            id: "mic1".into(),
            volume: 1.0,
            ..Default::default()
        }]);
        app.tab = Tab::Inputs;
        let action = app.handle_key(key(KeyCode::Right));
        assert_eq!(
            action,
            AppAction::Command(UiCommand::SetVolume {
                input_id: "mic1".into(),
                volume: 1.0,
            })
        );

        // At min
        let mut app = make_app_with_inputs(vec![InputState {
            id: "mic1".into(),
            volume: 0.0,
            ..Default::default()
        }]);
        app.tab = Tab::Inputs;
        let action = app.handle_key(key(KeyCode::Left));
        assert_eq!(
            action,
            AppAction::Command(UiCommand::SetVolume {
                input_id: "mic1".into(),
                volume: 0.0,
            })
        );
    }

    #[test]
    fn test_app_mute_toggle() {
        let mut app = make_app_with_inputs(vec![InputState {
            id: "mic1".into(),
            muted: false,
            ..Default::default()
        }]);
        app.tab = Tab::Inputs;
        let action = app.handle_key(key(KeyCode::Char('m')));
        assert_eq!(
            action,
            AppAction::Command(UiCommand::SetMuted {
                input_id: "mic1".into(),
                muted: true,
            })
        );
    }

    #[test]
    fn test_app_enable_toggle() {
        let mut app = make_app_with_inputs(vec![InputState {
            id: "mic1".into(),
            enabled: true,
            ..Default::default()
        }]);
        app.tab = Tab::Inputs;
        let action = app.handle_key(key(KeyCode::Char('e')));
        assert_eq!(
            action,
            AppAction::Command(UiCommand::SetEnabled {
                input_id: "mic1".into(),
                enabled: false,
            })
        );
    }

    #[test]
    fn test_app_play_mixed_toggle() {
        let mut app = make_app();
        app.state.output.play_mixed_input = true;
        app.tab = Tab::Outputs;
        let action = app.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(
            action,
            AppAction::Command(UiCommand::SetPlayMixedInput(false))
        );
    }

    #[test]
    fn test_app_quit() {
        let mut app = make_app();
        let action = app.handle_key(key(KeyCode::Char('q')));
        assert_eq!(action, AppAction::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn test_app_log_scroll() {
        let logs = Arc::new(Mutex::new(VecDeque::new()));
        {
            let mut buf = logs.lock().unwrap();
            for i in 0..20 {
                buf.push_back(format!("log line {}", i));
            }
        }
        let mut app = App::new(logs);
        app.tab = Tab::Logs;

        // Up → scroll increases
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.log_scroll, 1);
        assert!(!app.log_auto_scroll);

        // Down → scroll decreases
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.log_scroll, 0);

        // G → scroll to bottom, auto_scroll on
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.log_scroll, 2);
        app.handle_key(key(KeyCode::Char('G')));
        assert_eq!(app.log_scroll, 0);
        assert!(app.log_auto_scroll);
    }
}
