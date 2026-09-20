use serde_json::Value;

use super::ToolMeta;

pub const META: &[ToolMeta] = &[
    ToolMeta {
        name: "linear.issues",
        desc: "list Linear issues, newest first; optional team filter via linear.teams id",
        args: "{\"team_id\":\"\",\"limit\":20}",
        mutating: false,
    },
    ToolMeta {
        name: "linear.issue",
        desc: "read one Linear issue with description and comments",
        args: "{\"id\":\"ENG-123\"}",
        mutating: false,
    },
    ToolMeta {
        name: "linear.create_issue",
        desc: "create a Linear issue in a team",
        args: "{\"team_id\":\"...\",\"title\":\"...\",\"description\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "linear.comment",
        desc: "comment on a Linear issue",
        args: "{\"issue_id\":\"...\",\"body\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "linear.teams",
        desc: "list Linear teams with ids",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "slack.channels",
        desc: "list Slack channels, DMs and group messages with ids",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "slack.history",
        desc: "read recent messages from a Slack channel",
        args: "{\"channel\":\"C123\",\"limit\":30}",
        mutating: false,
    },
    ToolMeta {
        name: "slack.replies",
        desc: "read a Slack thread",
        args: "{\"channel\":\"C123\",\"ts\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "slack.send",
        desc: "send a Slack message; optional thread_ts to reply in a thread",
        args: "{\"channel\":\"C123\",\"text\":\"...\",\"thread_ts\":\"\"}",
        mutating: true,
    },
    ToolMeta {
        name: "slack.users",
        desc: "list Slack workspace users with ids",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "notion.search",
        desc: "search Notion pages and databases the integration can access",
        args: "{\"query\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "notion.query_db",
        desc: "query a Notion database; use notion.search to find its id",
        args: "{\"db_id\":\"...\",\"page_size\":20}",
        mutating: false,
    },
    ToolMeta {
        name: "notion.get_page",
        desc: "read one Notion page's properties",
        args: "{\"page_id\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "notion.create_page",
        desc: "create a Notion page under a parent page",
        args: "{\"parent_id\":\"...\",\"title\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "notion.read_blocks",
        desc: "read a Notion page or block's children as text",
        args: "{\"block_id\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "notion.append_text",
        desc: "append a paragraph to a Notion page or block",
        args: "{\"block_id\":\"...\",\"text\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "figma.me",
        desc: "who the Figma token belongs to",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "figma.file",
        desc: "Figma file name and page list from its key",
        args: "{\"key\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "figma.comments",
        desc: "read comments on a Figma file",
        args: "{\"key\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "figma.comment",
        desc: "comment on a Figma file",
        args: "{\"key\":\"...\",\"message\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "discord.guilds",
        desc: "list Discord servers the bot is in",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "discord.channels",
        desc: "list text channels of a Discord server",
        args: "{\"guild_id\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "discord.history",
        desc: "read recent messages from a Discord channel",
        args: "{\"channel_id\":\"...\",\"limit\":30}",
        mutating: false,
    },
    ToolMeta {
        name: "discord.send",
        desc: "send a message to a Discord channel",
        args: "{\"channel_id\":\"...\",\"content\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "telegram.me",
        desc: "which Telegram bot the token belongs to",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "telegram.updates",
        desc: "recent Telegram messages sent to the bot",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "telegram.send",
        desc: "send a Telegram message to a chat id from telegram.updates",
        args: "{\"chat_id\":\"...\",\"text\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "todoist.tasks",
        desc: "list Todoist tasks, optional filter like \"today\" or \"p1\"",
        args: "{\"filter\":\"\"}",
        mutating: false,
    },
    ToolMeta {
        name: "todoist.create_task",
        desc: "create a Todoist task; priority 1-4, 4 is highest",
        args: "{\"content\":\"...\",\"description\":\"\",\"priority\":1}",
        mutating: true,
    },
    ToolMeta {
        name: "todoist.close_task",
        desc: "complete a Todoist task",
        args: "{\"id\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "todoist.delete_task",
        desc: "delete a Todoist task",
        args: "{\"id\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "gitlab.projects",
        desc: "list your GitLab projects with ids",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "gitlab.issues",
        desc: "list a GitLab project's issues",
        args: "{\"project_id\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "gitlab.merge_requests",
        desc: "list a GitLab project's merge requests",
        args: "{\"project_id\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "gitlab.pipelines",
        desc: "list a GitLab project's recent pipelines",
        args: "{\"project_id\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "gitlab.create_issue",
        desc: "create a GitLab issue",
        args: "{\"project_id\":\"...\",\"title\":\"...\",\"description\":\"\"}",
        mutating: true,
    },
    ToolMeta {
        name: "ha.states",
        desc: "list Home Assistant entities with states",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "ha.state",
        desc: "read one Home Assistant entity",
        args: "{\"entity_id\":\"light.kitchen\"}",
        mutating: false,
    },
    ToolMeta {
        name: "ha.turn",
        desc: "call a Home Assistant service, e.g. domain light, service turn_on",
        args: "{\"domain\":\"light\",\"service\":\"turn_on\",\"entity_id\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "ha.config",
        desc: "read the Home Assistant instance config",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "trello.boards",
        desc: "list your Trello boards with ids",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "trello.lists",
        desc: "list a Trello board's lists",
        args: "{\"board_id\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "trello.cards",
        desc: "list a Trello list's cards",
        args: "{\"list_id\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "trello.create_card",
        desc: "create a Trello card",
        args: "{\"list_id\":\"...\",\"name\":\"...\",\"desc\":\"\"}",
        mutating: true,
    },
    ToolMeta {
        name: "trello.comment",
        desc: "comment on a Trello card",
        args: "{\"card_id\":\"...\",\"text\":\"...\"}",
        mutating: true,
    },
];

fn s<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .ok_or(format!("missing {key}"))
}

fn opt<'a>(args: &'a Value, key: &str) -> &'a str {
    args.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

fn num(args: &Value, key: &str, def: u64) -> u64 {
    args.get(key).and_then(|v| v.as_u64()).unwrap_or(def)
}

fn out(v: Result<Value, String>) -> Result<String, String> {
    v.map(|v| v.to_string())
}

pub async fn exec(name: &str, args: &Value) -> Result<String, String> {
    match name {
        "linear.issues" => {
            out(crate::mcp::linear::issues(opt(args, "team_id"), num(args, "limit", 20)).await)
        }
        "linear.issue" => out(crate::mcp::linear::get_issue(s(args, "id")?).await),
        "linear.create_issue" => out(crate::mcp::linear::create_issue(
            s(args, "team_id")?,
            s(args, "title")?,
            opt(args, "description"),
        )
        .await),
        "linear.comment" => {
            out(crate::mcp::linear::comment_issue(s(args, "issue_id")?, s(args, "body")?).await)
        }
        "linear.teams" => out(crate::mcp::linear::teams().await),
        "slack.channels" => out(crate::mcp::slack::channels().await),
        "slack.history" => {
            out(crate::mcp::slack::history(s(args, "channel")?, num(args, "limit", 30)).await)
        }
        "slack.replies" => {
            out(crate::mcp::slack::replies(s(args, "channel")?, s(args, "ts")?).await)
        }
        "slack.send" => out(crate::mcp::slack::send(
            s(args, "channel")?,
            s(args, "text")?,
            opt(args, "thread_ts"),
        )
        .await),
        "slack.users" => out(crate::mcp::slack::users().await),
        "notion.search" => out(crate::mcp::notion::search(opt(args, "query")).await),
        "notion.query_db" => {
            out(crate::mcp::notion::query_db(s(args, "db_id")?, num(args, "page_size", 20)).await)
        }
        "notion.get_page" => out(crate::mcp::notion::get_page(s(args, "page_id")?).await),
        "notion.create_page" => {
            out(crate::mcp::notion::create_page(s(args, "parent_id")?, s(args, "title")?).await)
        }
        "notion.read_blocks" => out(crate::mcp::notion::read_blocks(s(args, "block_id")?).await),
        "notion.append_text" => {
            out(crate::mcp::notion::append_text(s(args, "block_id")?, s(args, "text")?).await)
        }
        "figma.me" => out(crate::mcp::figma::me().await),
        "figma.file" => out(crate::mcp::figma::file_meta(s(args, "key")?).await),
        "figma.comments" => out(crate::mcp::figma::comments(s(args, "key")?).await),
        "figma.comment" => {
            out(crate::mcp::figma::post_comment(s(args, "key")?, s(args, "message")?).await)
        }
        "discord.guilds" => out(crate::mcp::discord::guilds().await),
        "discord.channels" => out(crate::mcp::discord::channels(s(args, "guild_id")?).await),
        "discord.history" => {
            out(crate::mcp::discord::history(s(args, "channel_id")?, num(args, "limit", 30)).await)
        }
        "discord.send" => {
            out(crate::mcp::discord::send(s(args, "channel_id")?, s(args, "content")?).await)
        }
        "telegram.me" => out(crate::mcp::telegram::me().await),
        "telegram.updates" => out(crate::mcp::telegram::updates().await),
        "telegram.send" => {
            out(crate::mcp::telegram::send(s(args, "chat_id")?, s(args, "text")?).await)
        }
        "todoist.tasks" => out(crate::mcp::todoist::tasks(opt(args, "filter")).await),
        "todoist.create_task" => out(crate::mcp::todoist::create_task(
            s(args, "content")?,
            opt(args, "description"),
            num(args, "priority", 1) as u8,
        )
        .await),
        "todoist.close_task" => {
            crate::mcp::todoist::close_task(s(args, "id")?).await?;
            Ok("closed".into())
        }
        "todoist.delete_task" => {
            crate::mcp::todoist::delete_task(s(args, "id")?).await?;
            Ok("deleted".into())
        }
        "gitlab.projects" => out(crate::mcp::gitlab::projects().await),
        "gitlab.issues" => out(crate::mcp::gitlab::issues(s(args, "project_id")?).await),
        "gitlab.merge_requests" => {
            out(crate::mcp::gitlab::merge_requests(s(args, "project_id")?).await)
        }
        "gitlab.pipelines" => out(crate::mcp::gitlab::pipelines(s(args, "project_id")?).await),
        "gitlab.create_issue" => out(crate::mcp::gitlab::create_issue(
            s(args, "project_id")?,
            s(args, "title")?,
            opt(args, "description"),
        )
        .await),
        "ha.states" => out(crate::mcp::ha::states().await),
        "ha.state" => out(crate::mcp::ha::get_state(s(args, "entity_id")?).await),
        "ha.turn" => out(crate::mcp::ha::turn(
            s(args, "domain")?,
            s(args, "service")?,
            s(args, "entity_id")?,
        )
        .await),
        "ha.config" => out(crate::mcp::ha::config().await),
        "trello.boards" => out(crate::mcp::trello::boards().await),
        "trello.lists" => out(crate::mcp::trello::lists(s(args, "board_id")?).await),
        "trello.cards" => out(crate::mcp::trello::cards(s(args, "list_id")?).await),
        "trello.create_card" => out(crate::mcp::trello::create_card(
            s(args, "list_id")?,
            s(args, "name")?,
            opt(args, "desc"),
        )
        .await),
        "trello.comment" => {
            out(crate::mcp::trello::comment_card(s(args, "card_id")?, s(args, "text")?).await)
        }
        _ => Err("unknown connector tool".into()),
    }
}
