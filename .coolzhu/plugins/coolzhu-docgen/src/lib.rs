//! Document Generator — render docs in Markdown, HTML, JSON, YAML + CHANGELOG.
//!
//! Produces structured documentation from code and commit history.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocFormat {
    Markdown,
    Html,
    Json,
    Yaml,
}

#[derive(Debug, Clone)]
struct Section {
    title: String,
    level: usize,
    content: String,
}

#[derive(Debug, Clone)]
pub struct DocGenerator {
    title: String,
    sections: Vec<Section>,
    format: DocFormat,
}

impl DocGenerator {
    pub fn new(title: &str, format: DocFormat) -> Self {
        Self {
            title: title.into(),
            sections: vec![],
            format,
        }
    }

    pub fn add_section(&mut self, title: &str, level: usize, content: &str) {
        self.sections.push(Section {
            title: title.into(),
            level,
            content: content.into(),
        });
    }

    pub fn render(&self) -> String {
        match self.format {
            DocFormat::Markdown => self.render_md(),
            DocFormat::Html => self.render_html(),
            DocFormat::Json => self.render_json(),
            DocFormat::Yaml => self.render_yml(),
        }
    }

    fn render_md(&self) -> String {
        let mut out = format!("# {}\n\n", self.title);
        for s in &self.sections {
            out.push_str(&format!(
                "{} {}\n\n{}\n\n",
                "#".repeat(s.level + 1),
                s.title,
                s.content
            ));
        }
        out
    }

    fn render_html(&self) -> String {
        let mut out = format!(
            "<!DOCTYPE html>\n<html>\n<head><title>{}</title></head>\n<body>\n<h1>{}</h1>\n",
            self.title, self.title
        );
        for s in &self.sections {
            out.push_str(&format!(
                "<h{}>{}</h{}>\n<p>{}</p>\n",
                s.level + 1,
                s.title,
                s.level + 1,
                s.content
            ));
        }
        out.push_str("</body>\n</html>\n");
        out
    }

    fn render_json(&self) -> String {
        let mut out = format!("{{\n  \"title\": \"{}\",\n  \"sections\": [\n", self.title);
        let len = self.sections.len();
        for (i, s) in self.sections.iter().enumerate() {
            let comma = if i + 1 < len { "," } else { "" };
            out.push_str(&format!(
                "    {{ \"title\": \"{}\", \"level\": {}, \"content\": \"{}\" }}{comma}\n",
                s.title, s.level, s.content
            ));
        }
        out.push_str("  ]\n}\n");
        out
    }

    fn render_yml(&self) -> String {
        let mut out = format!("title: {}\nsections:\n", self.title);
        for s in &self.sections {
            out.push_str(&format!(
                "  - title: {}\n    level: {}\n    content: |\n",
                s.title, s.level
            ));
            for l in s.content.lines() {
                out.push_str(&format!("      {l}\n"));
            }
        }
        out
    }

    /// Generate CHANGELOG from conventional commits.
    pub fn changelog(commits: &[(&str, &str)], version: &str) -> String {
        let mut feats = vec![];
        let mut fixes = vec![];
        let mut chores = vec![];
        for (msg, _) in commits {
            if msg.starts_with("feat") {
                feats.push(msg);
            } else if msg.starts_with("fix") {
                fixes.push(msg);
            } else {
                chores.push(msg);
            }
        }
        let mut out = format!("# Changelog\n\n## {version}\n\n");
        if !feats.is_empty() {
            out.push_str("### Features\n");
            for f in &feats {
                out.push_str(&format!("- {f}\n"));
            }
        }
        if !fixes.is_empty() {
            out.push_str("### Bug Fixes\n");
            for f in &fixes {
                out.push_str(&format!("- {f}\n"));
            }
        }
        if !chores.is_empty() {
            out.push_str("### Chores\n");
            for c in &chores {
                out.push_str(&format!("- {c}\n"));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_basic() {
        let mut g = DocGenerator::new("API", DocFormat::Markdown);
        g.add_section("Auth", 1, "Use tokens.");
        let md = g.render();
        assert!(md.contains("# API"));
        assert!(md.contains("## Auth"));
    }

    #[test]
    fn html_basic() {
        let mut g = DocGenerator::new("Guide", DocFormat::Html);
        g.add_section("Intro", 1, "Welcome.");
        assert!(g.render().contains("<!DOCTYPE html>"));
    }

    #[test]
    fn changelog_categorizes() {
        let cl = DocGenerator::changelog(
            &[
                ("feat: login", "a"),
                ("fix: crash", "b"),
                ("chore: deps", "c"),
            ],
            "v1.0",
        );
        assert!(cl.contains("### Features"));
        assert!(cl.contains("### Bug Fixes"));
        assert!(cl.contains("### Chores"));
    }
}
