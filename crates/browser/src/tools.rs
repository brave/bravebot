//! The tools the MCP server offers, and the extension method each one calls.
//!
//! The list is fixed here rather than asked of the extension. A person vouches for a server's tool
//! list once and BraveBot records a digest of it, so a list that changed with whether the extension
//! was connected would be put to them again every time it did. Each tool calls the extension method
//! of the same name with the tool's arguments as its parameters, and which of these the extension
//! answers is its own business: a tool it does not answer fails like any other call.
//!
//! Every argument is one a person can judge from the question BraveBot asks before a call, which
//! shows the arguments as the planner wrote them. A page is named by its URL for that reason: a
//! tab id is a number that says nothing about which page it is.

use serde_json::{Value, json};

/// One tool: its name, which is also the extension method it calls, what it does, and its schema.
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    schema: fn() -> Value,
}

impl Tool {
    /// The tool as `tools/list` describes it.
    pub fn listed(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": (self.schema)(),
        })
    }
}

/// Every tool, in the order `tools/list` gives them.
pub const TOOLS: &[Tool] = &[
    Tool {
        name: "list_tabs",
        description: "List the tabs open in Brave: each tab's id, window id, title and URL.",
        schema: || json!({"type": "object", "properties": {}}),
    },
    Tool {
        name: "read_page",
        description: "Read the text of the open tab at this URL, as list_tabs gave it.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "search_history",
        description: "Search Brave's browsing history by words in a page's title or URL.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 100},
                },
                "required": ["query"],
            })
        },
    },
    Tool {
        name: "search_bookmarks",
        description: "Search Brave's bookmarks by words in a bookmark's title or URL.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"query": {"type": "string"}},
                "required": ["query"],
            })
        },
    },
];

/// The tool of this name, if there is one.
pub fn named(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|tool| tool.name == name)
}

/// The whole list, as the `tools/list` result.
pub fn list() -> Value {
    json!({"tools": TOOLS.iter().map(Tool::listed).collect::<Vec<_>>()})
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name is looked up exactly, so a near miss calls nothing.
    #[test]
    fn a_tool_is_found_by_its_exact_name() {
        assert_eq!(named("list_tabs").map(|tool| tool.name), Some("list_tabs"));
        assert!(named("list_tab").is_none());
        assert!(named("List_Tabs").is_none());
    }

    /// The question before a call shows its arguments, so a page is asked for by the URL a person
    /// can read there rather than by a tab id that names nothing they can check.
    #[test]
    fn a_page_is_asked_for_by_its_url() {
        let schema = named("read_page").unwrap().listed()["inputSchema"].clone();
        assert_eq!(schema["required"], json!(["url"]));
        assert_eq!(schema["properties"]["url"]["type"], "string");
    }

    /// Every schema is an object schema, which is what an MCP client expects of `inputSchema`.
    #[test]
    fn every_tool_takes_an_object() {
        for tool in TOOLS {
            assert_eq!(
                tool.listed()["inputSchema"]["type"],
                "object",
                "{}",
                tool.name
            );
        }
    }
}
