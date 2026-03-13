use crate::git::format_age;
use crate::model::{RepoState, StatusKind};
use crossterm::{
    cursor,
    execute, queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{self, Clear, ClearType},
};
use std::io::{self, Write};

pub fn enter_ui() -> io::Result<()> {
    terminal::enable_raw_mode()?;
    execute!(io::stdout(), terminal::EnterAlternateScreen, cursor::Hide)
}

pub fn exit_ui() -> io::Result<()> {
    execute!(io::stdout(), cursor::Show, terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()
}

pub fn render(
    repos: &[RepoState],
    width: usize,
    color: bool,
    show_clean: bool,
    blank_line_between: bool,
) {
    let mut out = io::stdout();
    let _ = queue!(out, cursor::MoveTo(0, 0), Clear(ClearType::All));

    let mut first = true;
    for repo in repos {
        if !show_clean && matches!(repo.status, StatusKind::Clean) {
            continue;
        }

        if !first && blank_line_between {
            let _ = queue!(out, Print("\r\n"));
        }
        first = false;

        render_line1(&mut out, repo, width, color);
        let _ = queue!(out, Print("\r\n"));
        render_line2(&mut out, repo, width, color);
        let _ = queue!(out, Print("\r\n"));
    }

    let _ = out.flush();
}

fn render_line1(out: &mut impl Write, repo: &RepoState, width: usize, color: bool) {
    let (sym1, col1, sym2, col2) = match repo.status {
        StatusKind::StagedOnly => ("●", Color::Blue, "·", Color::DarkGrey),
        StatusKind::UnstagedOnly => ("·", Color::DarkGrey, "●", Color::Yellow),
        StatusKind::Both => ("●", Color::Blue, "●", Color::Yellow),
        StatusKind::Clean => ("·", Color::DarkGrey, "·", Color::DarkGrey),
        StatusKind::Error => ("✖", Color::Red, "✖", Color::Red),
    };

    if color {
        let _ = queue!(
            out,
            SetForegroundColor(col1),
            Print(sym1),
            SetForegroundColor(col2),
            Print(sym2),
            ResetColor,
        );
    } else {
        let _ = queue!(out, Print(sym1), Print(sym2));
    }

    let _ = queue!(out, Print(" "));

    let max_name = if width > 3 { width - 3 } else { width };
    let name = trim_str(&repo.name, max_name);
    let _ = queue!(out, Print(name));
}

fn render_line2(out: &mut impl Write, repo: &RepoState, width: usize, color: bool) {
    let _ = queue!(out, Print("  "));

    if matches!(repo.status, StatusKind::Error) {
        let msg = "repository unavailable";
        let age_str = "--";
        let available = if width > 6 { width - 6 } else { 0 };
        let msg_trimmed = trim_str(msg, available.saturating_sub(age_str.len() + 2));

        if color {
            let _ = queue!(out, SetAttribute(Attribute::Dim));
        }
        let _ = queue!(out, Print(&msg_trimmed));
        let padding = available.saturating_sub(msg_trimmed.len() + age_str.len());
        for _ in 0..padding {
            let _ = queue!(out, Print(" "));
        }
        if color {
            let _ = queue!(out, SetAttribute(Attribute::Reset));
        }
        let _ = queue!(out, Print(age_str));
        return;
    }

    let age_str = match repo.commit_timestamp {
        Some(ts) => format_age(ts),
        None => "--".to_string(),
    };

    // Available space: width - 2 (indent) - 2 (gap before age) - age_len
    let content_width = width.saturating_sub(2);
    let summary_max = content_width.saturating_sub(age_str.len() + 2);
    let summary = trim_str(&repo.commit_summary, summary_max);

    if color {
        let _ = queue!(out, SetAttribute(Attribute::Dim));
    }
    let _ = queue!(out, Print(&summary));
    if color {
        let _ = queue!(out, SetAttribute(Attribute::Reset));
    }

    let padding = content_width.saturating_sub(summary.len() + age_str.len());
    for _ in 0..padding {
        let _ = queue!(out, Print(" "));
    }
    let _ = queue!(out, Print(&age_str));
}

fn trim_str(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else if max <= 3 {
        chars[..max].iter().collect()
    } else {
        let mut result: String = chars[..max - 3].iter().collect();
        result.push_str("...");
        result
    }
}
