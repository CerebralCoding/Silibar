use std::collections::BTreeMap;

use colored::Colorize;
use serde::Deserialize;

use silibar::system_info::{Fan, SystemMessage};
use zellij_tile::prelude::*;

const DEFAULT_METRICS_URL: &str = "http://127.0.0.1:9090/json";
const SAMPLE_INTERVAL_SECONDS: f64 = 1.0;
const COMPACT_TERMINAL_MAX_COLS: usize = 120;

#[derive(Default)]
struct State {
    stats: SystemMessage,
    metrics_url: String,
    metrics_only: bool,
    session_name: String,
    mode_name: String,
    base_mode_name: String,
    active_tab_name: String,
    active_tab_position: usize,
    tabs: Vec<TabInfo>,
    tab_click_ranges: Vec<(usize, usize, u32)>,
    simplified_ui: bool,
    seen_selectable_pane: bool,
    permission_granted: bool,
    permission_denied: bool,
    request_in_flight: bool,
    sample_received: bool,
    last_error: Option<String>,
}

register_plugin!(State);

#[derive(Default, Deserialize)]
struct MacmonSnapshot {
    temp: Option<MacmonTemp>,
    memory: Option<MacmonMemory>,
    fans: Option<Vec<Fan>>,
    cpu_usage_pct: Option<f32>,
    gpu_usage: Option<(u32, f32)>,
    cpu_power: Option<f32>,
    gpu_power: Option<f32>,
    ane_power: Option<f32>,
    all_power: Option<f32>,
    sys_power: Option<f32>,
    soc: Option<MacmonSoc>,
}

#[derive(Default, Deserialize)]
struct MacmonTemp {
    cpu_temp_avg: Option<f32>,
    gpu_temp_avg: Option<f32>,
}

#[derive(Default, Deserialize)]
struct MacmonMemory {
    ram_total: u64,
    ram_usage: u64,
    swap_total: u64,
    swap_usage: u64,
}

#[derive(Default, Deserialize)]
struct MacmonSoc {
    chip_name: String,
}

impl From<MacmonSnapshot> for SystemMessage {
    fn from(snapshot: MacmonSnapshot) -> Self {
        let temp = snapshot.temp.unwrap_or_default();
        let memory = snapshot.memory.unwrap_or_default();
        let (gpu_frequency_mhz, gpu_usage) = snapshot.gpu_usage.unwrap_or_default();

        Self {
            cpu_usage: ratio_to_pct(snapshot.cpu_usage_pct.unwrap_or_default()),
            mem_used: memory.ram_usage,
            mem_total: memory.ram_total,
            swap_used: memory.swap_usage,
            swap_total: memory.swap_total,
            gpu_usage: ratio_to_pct(gpu_usage),
            gpu_frequency_mhz,
            cpu_power_watts: snapshot.cpu_power.unwrap_or_default(),
            gpu_power_watts: snapshot.gpu_power.unwrap_or_default(),
            ane_power_watts: snapshot.ane_power.unwrap_or_default(),
            total_power_watts: snapshot.all_power.unwrap_or_default(),
            system_power_watts: snapshot.sys_power.unwrap_or_default(),
            cpu_temp_celsius: temp.cpu_temp_avg.unwrap_or_default(),
            gpu_temp_celsius: temp.gpu_temp_avg.unwrap_or_default(),
            fans: snapshot.fans.unwrap_or_default(),
            chip_name: snapshot.soc.map(|soc| soc.chip_name).unwrap_or_default(),
        }
    }
}

fn ratio_to_pct(value: f32) -> f32 {
    (value * 100.0).clamp(0.0, 100.0)
}

fn strip_ansi_codes(s: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;]*[mK]").unwrap();
    re.replace_all(s, "").to_string()
}

fn metric_value(text: &str) -> colored::ColoredString {
    text.bright_green().on_black()
}

fn temperature_value(celsius: f32) -> String {
    if celsius.is_finite() && celsius > 0.0 {
        format!("{celsius:>3.0}C")
    } else {
        " --C".to_owned()
    }
}

fn metric_segment(label: &str, value: &str, compact: bool) -> String {
    if compact {
        return format!("{}", metric_value(value));
    }

    format!(
        "{}{}{}",
        label.white().on_black().bold(),
        " ".on_black(),
        metric_value(value)
    )
}

fn active_tab_ribbon(text: &str) -> String {
    serialize_ribbon(&Text::new(text).selected())
}

fn inactive_tab_ribbon(text: &str) -> String {
    serialize_ribbon(&Text::new(text))
}

fn tab_ribbon_width(label: &str, simplified_ui: bool) -> usize {
    label.chars().count() + 2 + if simplified_ui { 0 } else { 2 }
}

fn tab_at_column(ranges: &[(usize, usize, u32)], column: usize) -> Option<u32> {
    ranges
        .iter()
        .find(|(start, end, _)| column >= *start && column < *end)
        .map(|(_, _, tab_index)| *tab_index)
}

fn tab_bar_title(session_name: &str) -> String {
    format!(" Zellij ({session_name}) ")
}

fn status_value(text: &str, cols: usize) -> String {
    if cols < 12 {
        return String::new();
    }

    format!("{}", text.bright_yellow().on_black().bold())
}

fn visible_width(s: &str) -> usize {
    strip_ansi_codes(s).chars().count()
}

fn mode_name(mode: InputMode) -> String {
    match mode {
        InputMode::EnterSearch => "ENTER SEARCH".to_string(),
        InputMode::RenameTab => "RENAME TAB".to_string(),
        InputMode::RenamePane => "RENAME PANE".to_string(),
        other => format!("{:?}", other).to_uppercase(),
    }
}

fn join_left_right_with_width(left: &str, left_width: usize, right: &str, cols: usize) -> String {
    let right_width = visible_width(right);
    format!(
        "{}{}{}",
        left,
        " ".repeat(cols.saturating_sub(left_width + right_width))
            .on_black(),
        right
    )
}

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        colored::control::set_override(true);
        set_selectable(false);
        self.metrics_url = configuration
            .get("metrics_url")
            .cloned()
            .unwrap_or_else(|| DEFAULT_METRICS_URL.to_string());
        self.metrics_only = configuration
            .get("render_mode")
            .is_some_and(|mode| mode == "metrics");

        request_permission(&[
            PermissionType::WebAccess,
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
        ]);

        subscribe(&[
            EventType::PermissionRequestResult,
            EventType::WebRequestResult,
            EventType::ModeUpdate,
            EventType::TabUpdate,
            EventType::Mouse,
            EventType::PaneUpdate,
            EventType::PaneClosed,
            EventType::Timer,
        ]);
        set_timeout(0.2);
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;

        match event {
            Event::PermissionRequestResult(status) => match status {
                PermissionStatus::Granted => {
                    eprintln!("Permission granted");
                    self.permission_granted = true;
                    self.permission_denied = false;
                    self.request_sample();
                    set_timeout(SAMPLE_INTERVAL_SECONDS);
                    should_render = true;
                }
                PermissionStatus::Denied => {
                    eprintln!("Permission denied");
                    self.permission_denied = true;
                    should_render = true;
                }
            },
            Event::WebRequestResult(status, _headers, body, _context) => {
                self.request_in_flight = false;
                if status == 200 {
                    match serde_json::from_slice::<MacmonSnapshot>(&body) {
                        Ok(snapshot) => {
                            self.stats = snapshot.into();
                            self.sample_received = true;
                            self.last_error = None;
                        }
                        Err(err) => {
                            eprintln!("Failed to parse macmon response: {err}");
                            self.last_error = Some("macmon: bad data".to_string());
                        }
                    }
                } else {
                    self.last_error = Some(format!("macmon: http {status}"));
                }
                should_render = true;
            }
            Event::ModeUpdate(mode_info) => {
                self.simplified_ui = mode_info.capabilities.arrow_fonts;
                self.mode_name = mode_name(mode_info.mode);
                self.base_mode_name = mode_info
                    .base_mode
                    .map(mode_name)
                    .unwrap_or_else(|| self.mode_name.clone());
                self.session_name = mode_info.session_name.unwrap_or_default();
                should_render = true;
            }
            Event::TabUpdate(tabs) => {
                if let Some(active_tab) = tabs.iter().find(|tab| tab.active) {
                    self.active_tab_name = active_tab.name.clone();
                    self.active_tab_position = active_tab.position + 1;
                    should_render = true;
                }

                let selectable_panes: usize = tabs
                    .iter()
                    .map(|tab| {
                        tab.selectable_tiled_panes_count + tab.selectable_floating_panes_count
                    })
                    .sum();

                if selectable_panes > 0 {
                    self.seen_selectable_pane = true;
                } else if self.seen_selectable_pane {
                    quit_zellij();
                }
                self.tabs = tabs;
            }
            Event::Mouse(Mouse::LeftClick(_, column)) => {
                if let Some(tab_index) = tab_at_column(&self.tab_click_ranges, column) {
                    switch_tab_to(tab_index);
                }
            }
            Event::PaneUpdate(_) => {}
            Event::PaneClosed(_) => {
                should_render = true;
            }
            Event::Timer(_) => {
                if self.permission_granted {
                    self.request_sample();
                }
                set_timeout(SAMPLE_INTERVAL_SECONDS);
                should_render = true;
            }
            _ => {}
        }

        should_render
    }
    fn render(&mut self, rows: usize, cols: usize) {
        self.tab_click_ranges.clear();
        if cols == 0 {
            return;
        }

        if self.metrics_only {
            self.render_metrics_only(rows, cols);
            return;
        }

        let bar = self.combined_bar_text(cols);

        if rows >= 2 {
            println!("{}", bar);
            print!("{}", " ".repeat(cols).on_black());
        } else {
            print!("{}", bar);
        }
    }
}

impl State {
    fn request_sample(&mut self) {
        if self.request_in_flight {
            return;
        }

        let mut context = BTreeMap::new();
        context.insert("request".to_string(), "macmon-sample".to_string());
        web_request(
            &self.metrics_url,
            HttpVerb::Get,
            BTreeMap::new(),
            Vec::new(),
            context,
        );
        self.request_in_flight = true;
    }

    fn render_metrics_only(&self, rows: usize, cols: usize) {
        if let Some(status) = self.status_text(cols) {
            self.render_right_aligned(&status, rows, cols);
            return;
        }

        let metrics = self.metrics_text(cols);
        self.render_right_aligned(&metrics, rows, cols);
    }

    fn render_right_aligned(&self, text: &str, rows: usize, cols: usize) {
        if cols == 0 {
            return;
        }

        let output = if visible_width(text) > cols {
            let truncated: String = strip_ansi_codes(text).chars().take(cols).collect();
            truncated.white().on_black().to_string()
        } else {
            text.to_string()
        };

        let metrics = output;
        let metrics_width = visible_width(&metrics);

        if metrics_width >= cols {
            print!("{}", metrics);
            return;
        }

        let line = format!(
            "{}{}",
            " ".repeat(cols.saturating_sub(metrics_width)).on_black(),
            metrics
        );

        if rows >= 2 {
            println!("{}", line);
            print!("{}", " ".repeat(cols).on_black());
        } else {
            print!("{}", line);
        }
    }

    fn status_text(&self, cols: usize) -> Option<String> {
        if self.permission_denied {
            return Some(status_value("macmon: web access denied", cols));
        }

        if !self.permission_granted {
            return Some(status_value("macmon: allow web access", cols));
        }

        if !self.sample_received {
            if let Some(error) = &self.last_error {
                return Some(status_value(error, cols));
            }
            return Some(status_value("macmon: starting", cols));
        }

        None
    }

    fn tab_bar_text(&mut self, cols: usize, available_cols: usize) -> (String, usize) {
        self.tab_click_ranges.clear();
        let mut tabs = self.tabs.clone();
        tabs.sort_by_key(|tab| tab.position);
        if tabs.is_empty() {
            tabs.push(TabInfo {
                name: self.active_tab_name.clone(),
                position: self.active_tab_position.max(1) - 1,
                active: true,
                ..Default::default()
            });
        }
        let labels: Vec<String> = tabs
            .iter()
            .map(|tab| {
                let label = if tab.name.is_empty() {
                    format!("Tab #{}", tab.position + 1)
                } else {
                    tab.name.clone()
                };
                if tab_ribbon_width(&label, self.simplified_ui) > available_cols {
                    format!("#{}", tab.position + 1)
                } else {
                    label
                }
            })
            .collect();
        let widths: Vec<usize> = labels
            .iter()
            .map(|label| tab_ribbon_width(label, self.simplified_ui))
            .collect();
        let active = tabs.iter().position(|tab| tab.active).unwrap_or(0);
        if widths[active] > available_cols {
            return (String::new(), 0);
        }

        let session_name = if self.session_name.is_empty() {
            "macOS"
        } else {
            &self.session_name
        };
        let title = tab_bar_title(session_name);
        let mut text = String::new();
        let mut width = 0;
        if cols > COMPACT_TERMINAL_MAX_COLS
            && title.chars().count() + widths.iter().sum::<usize>() <= available_cols
        {
            width = title.chars().count();
            text = title.white().on_black().bold().to_string();
        } else if cols <= COMPACT_TERMINAL_MAX_COLS && widths[active] < available_cols {
            width = 1;
            text = " ".on_black().to_string();
        }

        let mut start = active;
        let mut end = active + 1;
        let mut used = width + widths[active];
        while start > 0 && used + widths[start - 1] <= available_cols {
            start -= 1;
            used += widths[start];
        }
        while end < tabs.len() && used + widths[end] <= available_cols {
            used += widths[end];
            end += 1;
        }

        for index in start..end {
            let tab = &tabs[index];
            let label = &labels[index];
            text.push_str(&if tab.active {
                active_tab_ribbon(label)
            } else {
                inactive_tab_ribbon(label)
            });
            let end_column = width + widths[index];
            self.tab_click_ranges
                .push((width, end_column, (tab.position + 1) as u32));
            width = end_column;
        }

        (text, width)
    }

    fn combined_bar_text(&mut self, cols: usize) -> String {
        let active_position = self
            .tabs
            .iter()
            .find(|tab| tab.active)
            .map(|tab| tab.position + 1)
            .unwrap_or(self.active_tab_position.max(1));
        let minimum_tab_width =
            tab_ribbon_width(&format!("#{active_position}"), self.simplified_ui);
        let right_padding = usize::from(cols > minimum_tab_width);
        let content_cols = cols.saturating_sub(right_padding);
        let mut metrics = self.metrics_text(cols);
        let metrics_budget = content_cols.saturating_sub(minimum_tab_width + 1);
        if visible_width(&metrics) > metrics_budget {
            metrics = self.metrics_text(metrics_budget);
        }
        let available_cols =
            content_cols.saturating_sub(visible_width(&metrics) + usize::from(!metrics.is_empty()));
        let (top_left, top_left_width) = self.tab_bar_text(cols, available_cols);

        let mut bar = join_left_right_with_width(&top_left, top_left_width, &metrics, content_cols);
        if right_padding > 0 {
            bar.push_str(&" ".on_black().to_string());
        }
        bar
    }

    fn metrics_text(&self, cols: usize) -> String {
        let regular_segments = self.metric_segments(false);
        let regular_separator = format!(
            "{}{}{}",
            " ".on_black(),
            "|".bright_black().on_black(),
            " ".on_black()
        );
        let regular = regular_segments.join(&regular_separator);

        if cols > COMPACT_TERMINAL_MAX_COLS && visible_width(&regular) <= cols {
            return regular;
        }

        let compact_segments = self.metric_segments(true);
        let compact_separator = format!(
            "{}{}{}",
            " ".on_black(),
            "|".bright_black().on_black(),
            " ".on_black()
        );
        fit_metric_segments(compact_segments, &compact_separator, 3, cols)
    }

    fn metric_segments(&self, compact: bool) -> Vec<String> {
        let mut segments: Vec<String> = Vec::new();

        let cpu_pct = self.stats.cpu_usage as f64;
        let cpu_value = format!("{cpu_pct:>3.0}%");
        segments.push(metric_segment("CPU:", &cpu_value, compact));

        let gpu_pct = self.stats.gpu_usage as f64;
        let gpu_value = format!("{gpu_pct:>3.0}%");
        segments.push(metric_segment("GPU:", &gpu_value, compact));

        let mem_used_gb = self.stats.mem_used as f64 / 1024.0 / 1024.0 / 1024.0;
        let mem_total_gb = self.stats.mem_total as f64 / 1024.0 / 1024.0 / 1024.0;
        let memory_value = format!("{mem_used_gb:>4.1}/{mem_total_gb:.0}G");
        segments.push(metric_segment("RAM:", &memory_value, compact));

        let total_power = if self.stats.system_power_watts > 0.0 {
            self.stats.system_power_watts
        } else {
            self.stats.total_power_watts
        };
        let power_value = format!("{total_power:>4.1}W");
        segments.push(metric_segment("PWR:", &power_value, compact));

        let temperatures = format!(
            "{}/{}",
            temperature_value(self.stats.cpu_temp_celsius),
            temperature_value(self.stats.gpu_temp_celsius)
        );
        segments.push(metric_segment("TEMP:", &temperatures, compact));

        segments
    }
}

fn fit_metric_segments(
    segments: Vec<String>,
    separator: &str,
    separator_width: usize,
    cols: usize,
) -> String {
    let mut fitted: Vec<String> = Vec::new();
    let mut used_cols = 0;

    for segment in segments {
        let segment_visible = visible_width(&segment);
        let space_needed = if fitted.is_empty() {
            segment_visible
        } else {
            separator_width + segment_visible
        };

        if used_cols + space_needed <= cols {
            used_cols += space_needed;
            fitted.push(segment);
        } else {
            break;
        }
    }

    if fitted.is_empty() {
        return String::new();
    }

    fitted.join(separator)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics_state() -> State {
        colored::control::set_override(true);
        State {
            stats: SystemMessage {
                cpu_usage: 42.0,
                gpu_usage: 73.0,
                mem_used: 8 * 1024 * 1024 * 1024,
                mem_total: 16 * 1024 * 1024 * 1024,
                system_power_watts: 12.3,
                cpu_temp_celsius: 55.0,
                gpu_temp_celsius: 62.0,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn tabs_state(count: usize, active: usize) -> State {
        State {
            tabs: (0..count)
                .map(|position| TabInfo {
                    position,
                    name: format!("Project {}", position + 1),
                    active: position == active,
                    ..Default::default()
                })
                .collect(),
            ..metrics_state()
        }
    }

    #[test]
    fn compact_bar_hides_the_title_and_keeps_full_tab_names() {
        let mut state = tabs_state(2, 1);
        let compact = state.combined_bar_text(120);
        assert!(
            strip_ansi_codes(&compact)
                .starts_with(&format!(" {}", inactive_tab_ribbon("Project 1")))
        );
        assert!(compact.contains(&active_tab_ribbon("Project 2")));
        assert!(!compact.contains("Zellij"));
        assert!(!compact.contains("CPU:"));
        assert_eq!(state.tab_click_ranges.first().unwrap().0, 1);
        assert_eq!(tab_at_column(&state.tab_click_ranges, 0), None);

        let desktop = state.combined_bar_text(121);
        assert!(strip_ansi_codes(&desktop).starts_with(" Zellij (macOS) "));
        assert!(desktop.contains(&inactive_tab_ribbon("Project 1")));
        assert!(desktop.contains(&active_tab_ribbon("Project 2")));
        assert!(strip_ansi_codes(&desktop).contains("CPU:"));
    }

    #[test]
    fn four_mobile_tabs_keep_complete_ribbons_and_matching_click_targets() {
        for cols in [97, 110, 120] {
            for simplified_ui in [false, true] {
                let mut state = tabs_state(4, 3);
                state.simplified_ui = simplified_ui;
                state.tabs.reverse();
                let bar = state.combined_bar_text(cols);
                assert_eq!(state.tab_click_ranges.len(), 4);
                assert_eq!(state.tab_click_ranges.first().unwrap().0, 1);
                assert_eq!(tab_at_column(&state.tab_click_ranges, 0), None);
                assert!(!bar.contains("Zellij"));
                let mut rendered = strip_ansi_codes(&bar);
                let ribbon_width = if simplified_ui { 11 } else { 13 };
                for (index, &(start, end, tab)) in state.tab_click_ranges.iter().enumerate() {
                    let label = format!("Project {}", index + 1);
                    let ribbon = if index == 3 {
                        active_tab_ribbon(&label)
                    } else {
                        inactive_tab_ribbon(&label)
                    };
                    assert!(bar.contains(&ribbon));
                    assert_eq!(tab, (index + 1) as u32);
                    assert_eq!(end - start, ribbon_width);
                    assert_eq!(tab_at_column(&state.tab_click_ranges, start), Some(tab));
                    assert_eq!(tab_at_column(&state.tab_click_ranges, end - 1), Some(tab));
                    rendered = rendered.replace(&ribbon, &" ".repeat(ribbon_width));
                }
                assert!(!rendered.contains('\u{1b}'));
                assert_eq!(rendered.chars().count(), cols);
                assert!(
                    rendered
                        .ends_with(&format!("{} ", strip_ansi_codes(&state.metrics_text(cols))))
                );
                let last_end = state.tab_click_ranges.last().unwrap().1;
                for column in last_end..cols {
                    assert_eq!(tab_at_column(&state.tab_click_ranges, column), None);
                }
            }
        }
    }

    #[test]
    fn overflowing_tabs_keep_the_active_tab_visible_without_cutting_ribbons() {
        for (cols, numbered) in [(40, true), (60, false), (96, false), (121, false)] {
            for active in [0, 5, 11] {
                for simplified_ui in [false, true] {
                    let mut state = tabs_state(12, active);
                    state.simplified_ui = simplified_ui;
                    let bar = state.combined_bar_text(cols);
                    assert!(
                        state
                            .tab_click_ranges
                            .iter()
                            .any(|&(_, _, tab)| tab == (active + 1) as u32)
                    );
                    let mut rendered = strip_ansi_codes(&bar);
                    for &(start, end, index) in &state.tab_click_ranges {
                        let tab = &state.tabs[index as usize - 1];
                        let label = if numbered {
                            format!("#{index}")
                        } else {
                            tab.name.clone()
                        };
                        let ribbon = if tab.active {
                            active_tab_ribbon(&label)
                        } else {
                            inactive_tab_ribbon(&label)
                        };
                        assert!(bar.contains(&ribbon));
                        assert!(start < end && end <= cols);
                        assert_eq!(tab_at_column(&state.tab_click_ranges, start), Some(index));
                        assert_eq!(tab_at_column(&state.tab_click_ranges, end - 1), Some(index));
                        rendered = rendered.replace(&ribbon, &" ".repeat(end - start));
                    }
                    assert!(!rendered.contains('\u{1b}'));
                    assert_eq!(rendered.chars().count(), cols);
                    assert_eq!(
                        bar.matches("\u{1b}Pzribbon;").count(),
                        state.tab_click_ranges.len()
                    );
                }
            }
        }
    }

    #[test]
    fn resizing_clears_hidden_tab_click_targets() {
        let mut state = tabs_state(12, 11);
        state.combined_bar_text(160);
        assert!(!state.tab_click_ranges.is_empty());

        let bar = state.combined_bar_text(7);
        assert_eq!(strip_ansi_codes(&bar), active_tab_ribbon("#12"));
        assert_eq!(state.tab_click_ranges, [(0, 7, 12)]);

        assert_eq!(state.combined_bar_text(3), "   ".on_black().to_string());
        assert!(state.tab_click_ranges.is_empty());
        assert_eq!(strip_ansi_codes(&state.combined_bar_text(0)), "");
    }

    #[test]
    fn oversized_names_fall_back_to_a_complete_numbered_tab() {
        let mut state = tabs_state(1, 0);
        state.session_name = "session".repeat(30);
        state.tabs[0].name = "project".repeat(30);
        let bar = state.combined_bar_text(121);
        assert!(bar.starts_with(&active_tab_ribbon("#1")));
        assert!(
            strip_ansi_codes(&bar)
                .ends_with(&format!("{} ", strip_ansi_codes(&state.metrics_text(121))))
        );
        assert_eq!(state.tab_click_ranges, [(0, 6, 1)]);
    }

    #[test]
    fn wide_metrics_are_labelled_and_put_gpu_before_ram() {
        let metrics = strip_ansi_codes(&metrics_state().metrics_text(121));
        assert_eq!(
            metrics,
            "CPU:  42% | GPU:  73% | RAM:  8.0/16G | PWR: 12.3W | TEMP:  55C/ 62C"
        );
    }

    #[test]
    fn narrow_metrics_automatically_compact() {
        let metrics = strip_ansi_codes(&metrics_state().metrics_text(96));
        assert_eq!(metrics, " 42% |  73% |  8.0/16G | 12.3W |  55C/ 62C");
    }

    #[test]
    fn metrics_switch_at_the_mobile_terminal_breakpoint() {
        let state = metrics_state();
        let regular = strip_ansi_codes(&state.metrics_text(usize::MAX));

        assert_eq!(strip_ansi_codes(&state.metrics_text(121)), regular);
        assert_eq!(
            strip_ansi_codes(&state.metrics_text(120)),
            " 42% |  73% |  8.0/16G | 12.3W |  55C/ 62C"
        );
    }

    #[test]
    fn compact_metrics_fit_whole_segments() {
        let state = metrics_state();
        assert_eq!(strip_ansi_codes(&state.metrics_text(3)), "");
        assert_eq!(strip_ansi_codes(&state.metrics_text(4)), " 42%");
        assert_eq!(strip_ansi_codes(&state.metrics_text(10)), " 42%");
        assert_eq!(strip_ansi_codes(&state.metrics_text(11)), " 42% |  73%");
        assert_eq!(
            strip_ansi_codes(&state.metrics_text(41)),
            " 42% |  73% |  8.0/16G | 12.3W"
        );
        assert_eq!(
            strip_ansi_codes(&state.metrics_text(42)),
            " 42% |  73% |  8.0/16G | 12.3W |  55C/ 62C"
        );
    }

    #[test]
    fn combined_bar_uses_regular_metrics_on_desktop_widths() {
        let mut state = metrics_state();
        let regular = strip_ansi_codes(&state.metrics_text(usize::MAX));
        let bar = strip_ansi_codes(&state.combined_bar_text(160));

        assert!(bar.ends_with(&format!("{regular} ")));
    }

    #[test]
    fn combined_bar_uses_compact_metrics_on_mobile_widths() {
        let bar = strip_ansi_codes(&metrics_state().combined_bar_text(96));

        assert!(bar.ends_with(" 42% |  73% |  8.0/16G | 12.3W |  55C/ 62C "));
        assert!(!bar.contains("CPU:"));
    }

    #[test]
    fn compact_metrics_do_not_move_when_values_gain_digits() {
        let mut low = metrics_state();
        low.stats.cpu_usage = 9.0;
        low.stats.gpu_usage = 9.0;
        low.stats.mem_used = 9 * 1024 * 1024 * 1024;
        low.stats.system_power_watts = 9.9;
        low.stats.cpu_temp_celsius = 9.0;
        low.stats.gpu_temp_celsius = 9.0;

        let mut high = metrics_state();
        high.stats.cpu_usage = 100.0;
        high.stats.gpu_usage = 100.0;
        high.stats.mem_used = 10 * 1024 * 1024 * 1024;
        high.stats.system_power_watts = 10.0;
        high.stats.cpu_temp_celsius = 100.0;
        high.stats.gpu_temp_celsius = 100.0;

        let low_metrics = strip_ansi_codes(&low.metrics_text(96));
        let high_metrics = strip_ansi_codes(&high.metrics_text(96));

        assert_eq!(low_metrics.chars().count(), high_metrics.chars().count());
        assert_eq!(low_metrics, "  9% |   9% |  9.0/16G |  9.9W |   9C/  9C");
        assert_eq!(high_metrics, "100% | 100% | 10.0/16G | 10.0W | 100C/100C");
    }

    #[test]
    fn macmon_temperatures_render_independently_when_sensors_are_missing() {
        colored::control::set_override(true);
        for (json, expected) in [
            (
                r#"{"temp":{"cpu_temp_avg":55.0,"gpu_temp_avg":62.0}}"#,
                " |  55C/ 62C",
            ),
            (r#"{"temp":{"cpu_temp_avg":55.0}}"#, " |  55C/ --C"),
            (
                r#"{"temp":{"cpu_temp_avg":null,"gpu_temp_avg":62.0}}"#,
                " |  --C/ 62C",
            ),
            (r#"{}"#, " |  --C/ --C"),
        ] {
            let snapshot: MacmonSnapshot = serde_json::from_str(json).unwrap();
            let state = State {
                stats: snapshot.into(),
                ..Default::default()
            };
            assert!(strip_ansi_codes(&state.metrics_text(96)).ends_with(expected));
        }
    }

    #[test]
    fn unavailable_temperatures_keep_their_columns() {
        for temperature in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let mut state = metrics_state();
            state.stats.cpu_temp_celsius = temperature;
            state.stats.gpu_temp_celsius = temperature;
            assert_eq!(
                strip_ansi_codes(&state.metrics_text(96)),
                " 42% |  73% |  8.0/16G | 12.3W |  --C/ --C"
            );
        }
    }

    #[test]
    fn tab_section_width_does_not_change_the_metrics_format() {
        let mut short_tab = metrics_state();
        let mut long_tab = metrics_state();
        long_tab.active_tab_name =
            "a very long tab name that consumes the available gap".to_string();

        let desktop_metrics = format!("{} ", strip_ansi_codes(&short_tab.metrics_text(160)));
        let mobile_metrics = format!("{} ", strip_ansi_codes(&short_tab.metrics_text(96)));

        assert!(strip_ansi_codes(&short_tab.combined_bar_text(160)).ends_with(&desktop_metrics));
        assert!(strip_ansi_codes(&long_tab.combined_bar_text(160)).ends_with(&desktop_metrics));
        assert!(strip_ansi_codes(&short_tab.combined_bar_text(96)).ends_with(&mobile_metrics));
        assert!(strip_ansi_codes(&long_tab.combined_bar_text(96)).ends_with(&mobile_metrics));
    }

    #[test]
    fn active_tab_uses_zellij_selected_ribbon() {
        let ribbon = active_tab_ribbon("Tab #1");

        assert_eq!(ribbon, serialize_ribbon(&Text::new("Tab #1").selected()));
        assert!(ribbon.starts_with("\u{1b}Pzribbon;x"));
    }

    #[test]
    fn inactive_tab_uses_zellij_unselected_ribbon() {
        let ribbon = inactive_tab_ribbon("Tab #2");

        assert_eq!(ribbon, serialize_ribbon(&Text::new("Tab #2")));
        assert!(ribbon.starts_with("\u{1b}Pzribbon;"));
        assert!(!ribbon.starts_with("\u{1b}Pzribbon;x"));
    }

    #[test]
    fn mouse_tab_ranges_include_both_ribbon_separators() {
        assert_eq!(tab_ribbon_width("Tab #1", false), 10);
        assert_eq!(tab_ribbon_width("Tab #1", true), 8);

        let ranges = [(15, 25, 1), (25, 35, 2)];
        assert_eq!(tab_at_column(&ranges, 14), None);
        assert_eq!(tab_at_column(&ranges, 15), Some(1));
        assert_eq!(tab_at_column(&ranges, 24), Some(1));
        assert_eq!(tab_at_column(&ranges, 25), Some(2));
        assert_eq!(tab_at_column(&ranges, 34), Some(2));
        assert_eq!(tab_at_column(&ranges, 35), None);
    }

    #[test]
    fn session_title_keeps_builtin_leading_padding() {
        assert_eq!(tab_bar_title("macOS"), " Zellij (macOS) ");
    }
}
