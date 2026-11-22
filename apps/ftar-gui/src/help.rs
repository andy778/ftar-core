//! Built-in help: the user guide (`docs/USER-GUIDE.md`, embedded as plain text)
//! shown in a window from the Help menu. The parsing is plain text handling, so
//! it is testable without a window.

use eframe::egui;

use crate::app::App;

/// The user guide, compiled into the binary as text (no binary assets).
const GUIDE: &str = include_str!("../../../docs/USER-GUIDE.md");
/// Title of the guide section about the program and its licence.
pub const ABOUT: &str = "About ftar";
/// Title of the intro shown before the first `## ` heading.
const OVERVIEW: &str = "Overview";

/// One block of a topic. Continuation lines are joined to the block they follow.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// A paragraph, or an empty string for a vertical gap.
    Text(String),
    Bullet(String),
}

/// A help page: a `## ` section of the guide.
#[derive(Debug, Clone, PartialEq)]
pub struct Topic {
    pub title: String,
    pub blocks: Vec<Block>,
}

/// Help window state: whether it is shown, which topic it shows, and the parsed guide.
#[derive(Debug, Clone)]
pub struct HelpState {
    pub open: bool,
    pub topic: usize,
    pub topics: Vec<Topic>,
}

impl Default for HelpState {
    fn default() -> Self {
        Self {
            open: false,
            topic: 0,
            topics: parse(GUIDE),
        }
    }
}

/// Split the guide into topics. The `# ` title is dropped, the text before the first
/// `## ` becomes the overview, `- ` lines become bullets and indented or wrapped
/// lines join the block before them. Never panics.
pub fn parse(text: &str) -> Vec<Topic> {
    let mut topics = vec![Topic {
        title: OVERVIEW.to_string(),
        blocks: Vec::new(),
    }];
    for line in text.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            topics.push(Topic {
                title: title.trim().to_string(),
                blocks: Vec::new(),
            });
            continue;
        }
        if line.starts_with("# ") {
            continue;
        }
        let Some(topic) = topics.last_mut() else {
            continue;
        };
        let blocks = &mut topic.blocks;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            blocks.push(Block::Text(String::new()));
        } else if let Some(item) = trimmed.strip_prefix("- ") {
            blocks.push(Block::Bullet(item.to_string()));
        } else {
            match blocks.last_mut() {
                Some(Block::Bullet(s)) | Some(Block::Text(s)) if !s.is_empty() => {
                    s.push(' ');
                    s.push_str(trimmed);
                }
                _ => blocks.push(Block::Text(trimmed.to_string())),
            }
        }
    }
    topics
}

impl App {
    /// Show the help window on `topic` (a title from the guide), or on the first page.
    pub fn show_help(&mut self, topic: Option<&str>) {
        self.help.open = true;
        self.help.topic = topic
            .and_then(|t| self.help.topics.iter().position(|x| x.title == t))
            .unwrap_or(0);
    }

    pub(crate) fn help_window(&mut self, ctx: &egui::Context) {
        if !self.help.open {
            return;
        }
        let mut open = true;
        let mut current = self.help.topic;
        // Taken out so the closure can borrow the rest of the app mutably.
        let topics = std::mem::take(&mut self.help.topics);
        egui::Window::new("ftar help")
            .open(&mut open)
            .default_size([760.0, 520.0])
            .show(ctx, |ui| {
                ui.label(format!("ftar {}", env!("CARGO_PKG_VERSION")));
                ui.separator();
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(170.0);
                        for (i, t) in topics.iter().enumerate() {
                            if ui.selectable_label(current == i, &t.title).clicked() {
                                current = i;
                            }
                        }
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if let Some(t) = topics.get(current) {
                                for b in &t.blocks {
                                    match b {
                                        Block::Text(s) if s.is_empty() => ui.add_space(6.0),
                                        Block::Text(s) => {
                                            ui.label(s);
                                        }
                                        Block::Bullet(s) => {
                                            ui.label(format!("\u{2022} {s}"));
                                        }
                                    }
                                }
                            }
                        });
                });
            });
        self.help.topic = current;
        self.help.topics = topics;
        self.help.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guide_has_overview_and_about_topics() {
        let topics = parse(GUIDE);
        assert_eq!(topics.first().map(|t| t.title.as_str()), Some(OVERVIEW));
        assert!(topics.iter().any(|t| t.title == ABOUT));
        assert!(topics.len() > 5);
    }

    #[test]
    fn bullets_and_wrapped_lines_are_joined() {
        let topics =
            parse("intro\nmore\n\n## Part\n- first\n  continued\n- second\n\nplain\nwrapped\n");
        assert_eq!(topics.len(), 2);
        assert_eq!(
            topics[0].blocks,
            vec![Block::Text("intro more".into()), Block::Text(String::new())]
        );
        assert_eq!(
            topics[1].blocks,
            vec![
                Block::Bullet("first continued".into()),
                Block::Bullet("second".into()),
                Block::Text(String::new()),
                Block::Text("plain wrapped".into()),
            ]
        );
    }

    #[test]
    fn empty_and_odd_input_do_not_panic() {
        assert_eq!(parse("").len(), 1);
        assert_eq!(parse("# \n## \n- \n   \n").len(), 2);
    }
}
