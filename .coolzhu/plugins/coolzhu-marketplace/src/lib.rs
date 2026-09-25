//! Agent Template Marketplace — publish, search, and install agent templates.
//! B6: Community-driven agent template sharing.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct AgentTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub model: String,
    pub allowed_tools: Vec<String>,
    pub skills: Vec<String>,
    pub prompt_template: String,
    pub version: String,
    pub author: String,
    pub downloads: u64,
}

#[derive(Debug, Clone)]
pub struct TemplateMarket {
    templates: HashMap<String, AgentTemplate>,
}

impl TemplateMarket {
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }

    pub fn publish(&mut self, t: AgentTemplate) {
        self.templates.insert(t.id.clone(), t);
    }

    pub fn search(&self, query: &str) -> Vec<&AgentTemplate> {
        let lower = query.to_lowercase();
        self.templates
            .values()
            .filter(|t| {
                t.name.to_lowercase().contains(&lower)
                    || t.description.to_lowercase().contains(&lower)
                    || t.category.to_lowercase().contains(&lower)
            })
            .collect()
    }

    pub fn by_category(&self, cat: &str) -> Vec<&AgentTemplate> {
        self.templates
            .values()
            .filter(|t| t.category == cat)
            .collect()
    }

    pub fn popular(&self, top: usize) -> Vec<&AgentTemplate> {
        let mut v: Vec<&AgentTemplate> = self.templates.values().collect();
        v.sort_by_key(|t| -(t.downloads as i64));
        v.truncate(top);
        v
    }

    pub fn install(&mut self, id: &str) -> Option<AgentTemplate> {
        self.templates.get_mut(id).map(|t| {
            t.downloads += 1;
            t.clone()
        })
    }
}

pub fn render_markdown(template: &AgentTemplate) -> String {
    let tools = template
        .allowed_tools
        .iter()
        .map(|t| format!("- {t}"))
        .collect::<Vec<_>>()
        .join("\n");
    let skills = template
        .skills
        .iter()
        .map(|s| format!("- {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("# {}\n\n{}\n\n**Category:** {}\n**Model:** {}\n**Author:** {}\n**Version:** {}\n**Downloads:** {}\n\n## Tools\n\n{tools}\n\n## Skills\n\n{skills}\n\n## Prompt\n\n```\n{}\n```\n",
        template.name, template.description, template.category, template.model,
        template.author, template.version, template.downloads, template.prompt_template)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str, cat: &str, dl: u64) -> AgentTemplate {
        AgentTemplate {
            id: id.into(),
            name: id.into(),
            description: "desc".into(),
            category: cat.into(),
            model: "m".into(),
            allowed_tools: vec![],
            skills: vec![],
            prompt_template: "".into(),
            version: "1".into(),
            author: "a".into(),
            downloads: dl,
        }
    }

    #[test]
    fn search_by_category() {
        let mut m = TemplateMarket::new();
        m.publish(sample("t1", "quality", 0));
        m.publish(sample("t2", "debug", 0));
        assert_eq!(m.by_category("quality").len(), 1);
    }

    #[test]
    fn popular_sorts_correctly() {
        let mut m = TemplateMarket::new();
        m.publish(sample("a", "t", 10));
        m.publish(sample("b", "t", 100));
        assert_eq!(m.popular(1)[0].downloads, 100);
    }

    #[test]
    fn install_increments() {
        let mut m = TemplateMarket::new();
        m.publish(sample("t", "t", 0));
        m.install("t");
        assert_eq!(m.install("t").unwrap().downloads, 2);
    }
}
