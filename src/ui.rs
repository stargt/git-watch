use crate::git::format_age;
use crate::model::{DetailedStatus, RepoState, StatusKind};
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
    selected: usize,
) {
    let mut out = io::stdout();
    let _ = queue!(out, cursor::MoveTo(0, 0), Clear(ClearType::All));

    let mut first = true;
    for (i, repo) in repos.iter().enumerate() {
        if !show_clean && matches!(repo.status, StatusKind::Clean) {
            continue;
        }

        if !first && blank_line_between {
            let _ = queue!(out, Print("\r\n"));
        }
        first = false;

        let is_selected = i == selected;
        if is_selected && color {
            let _ = queue!(out, SetAttribute(Attribute::Reverse));
        }
        render_line1(&mut out, repo, width, color);
        if is_selected && color {
            let _ = queue!(out, SetAttribute(Attribute::Reset));
        } else if is_selected {
            // non-color: we already rendered, no extra reset needed
        }
        let _ = queue!(out, Print("\r\n"));
        render_line2(&mut out, repo, width, color);
        let _ = queue!(out, Print("\r\n"));
    }

    // Key hints
    let _ = queue!(out, Print("\r\n"));
    if color {
        let _ = queue!(out, SetAttribute(Attribute::Dim));
    }
    let _ = queue!(out, Print(" [↑↓] select  [Enter] detail  [q] quit"));
    if color {
        let _ = queue!(out, SetAttribute(Attribute::Reset));
    }

    let _ = out.flush();
}

pub fn render_detail(repo: &RepoState, detail: &DetailedStatus, width: usize, color: bool) {
    let mut out = io::stdout();
    let _ = queue!(out, cursor::MoveTo(0, 0), Clear(ClearType::All));

    // Repo name
    let _ = queue!(out, Print("  "), Print(&repo.name), Print("\r\n"));
    // Separator
    let sep_len = width.min(repo.name.len() + 4);
    let _ = queue!(out, Print("  "));
    for _ in 0..sep_len {
        let _ = queue!(out, Print("─"));
    }
    let _ = queue!(out, Print("\r\n"));

    if !detail.staged.is_empty() {
        if color {
            let _ = queue!(out, SetForegroundColor(Color::Blue));
        }
        let _ = queue!(out, Print("  Staged:\r\n"));
        if color {
            let _ = queue!(out, ResetColor);
        }
        for line in &detail.staged {
            let _ = queue!(out, Print("    "), Print(line), Print("\r\n"));
        }
    }

    if !detail.unstaged.is_empty() {
        if color {
            let _ = queue!(out, SetForegroundColor(Color::Yellow));
        }
        let _ = queue!(out, Print("  Unstaged:\r\n"));
        if color {
            let _ = queue!(out, ResetColor);
        }
        for line in &detail.unstaged {
            let _ = queue!(out, Print("    "), Print(line), Print("\r\n"));
        }
    }

    if !detail.untracked.is_empty() {
        if color {
            let _ = queue!(out, SetForegroundColor(Color::DarkGrey));
        }
        let _ = queue!(out, Print("  Untracked:\r\n"));
        if color {
            let _ = queue!(out, ResetColor);
        }
        for line in &detail.untracked {
            let _ = queue!(out, Print("    "), Print(line), Print("\r\n"));
        }
    }

    if detail.staged.is_empty() && detail.unstaged.is_empty() && detail.untracked.is_empty() {
        if color {
            let _ = queue!(out, SetAttribute(Attribute::Dim));
        }
        let _ = queue!(out, Print("  (clean)\r\n"));
        if color {
            let _ = queue!(out, SetAttribute(Attribute::Reset));
        }
    }

    // Key hints
    let _ = queue!(out, Print("\r\n"));
    if color {
        let _ = queue!(out, SetAttribute(Attribute::Dim));
    }
    let _ = queue!(out, Print(" [Esc] back  [r] refresh"));
    if color {
        let _ = queue!(out, SetAttribute(Attribute::Reset));
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
