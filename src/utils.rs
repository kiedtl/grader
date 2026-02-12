use ratatui::{
    style::{Color, Style},
};

pub fn style_for_score(score: f32) -> Style {
    match score.ceil() as usize {
        0..=6 => Style::default().fg(Color::Black).bg(Color::Red),
        7..=8 => Style::default().fg(Color::Black).bg(Color::Yellow),
        _ => Style::default().fg(Color::Black).bg(Color::Green),
    }
}
