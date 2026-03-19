use ratatui::{
    style::{Color, Style},
    layout::{Constraint, Layout, Rect},
};

pub fn round_score(score: f32, factor: f32) -> f32 {
    (score * factor).round() / factor
}

pub fn style_for_score(score: f32) -> Style {
    match score.ceil() as usize {
        0..=6 => Style::default().fg(Color::Black).bg(Color::Red),
        7..=8 => Style::default().fg(Color::Black).bg(Color::Yellow),
        _ => Style::default().fg(Color::Black).bg(Color::Green),
    }
}

pub fn auto_grid(area: Rect, n: usize, spacing: u16) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }

    let cols = (n as f64).sqrt().ceil() as u16;
    let rows = ((n as f64) / f64::from(cols)).ceil() as u16;

    let row_constraints: Vec<Constraint> =
        std::iter::repeat_n(Constraint::Ratio(1, rows.into()), rows as usize).collect();

    let col_constraints: Vec<Constraint> =
        std::iter::repeat_n(Constraint::Ratio(1, cols.into()), cols as usize).collect();

    let row_areas = Layout::vertical(row_constraints)
        .spacing(spacing)
        .split(area);

    let mut out = Vec::with_capacity(n);
    'outer: for r in 0..rows as usize {
        let col_areas = Layout::horizontal(col_constraints.clone())
            .spacing(spacing)
            .split(row_areas[r]);
        for &rect in col_areas.iter() {
            if out.len() == n {
                break 'outer;
            }
            out.push(rect);
        }
    }
    out
}
