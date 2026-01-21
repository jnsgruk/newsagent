# Newsagent

Newsagent is an automated tool designed to assist in generating the "Tech Updates" section of newsletters. It leverages **Google Gemini** (via `rig-core`) to summarize release notes, blog posts, and technical updates into a consistent, engaging format.

## How It Works

1.  **Task Retrieval**: It connects to **Todoist** to fetch a list of tasks. Each task should contain a link to a resource (e.g., GitHub release page, documentation, blog post).
2.  **Content Extraction**: It visits the links provided in the tasks and extracts the main content using a readability tool.
3.  **Context Loading**: It creates a style guide context by "gleaning" from a specified directory of previous writings (Markdown files), ensuring consistency in tone and structure.
4.  **Generation**: Using the Gemini 3 Pro model, it processes the gathered information to generate a categorized, emoji-enhanced Markdown summary suitable for a newsletter.

## Configuration

Newsagent is configured via environment variables. You can set these in your shell or use a `.env` file in the project root.

### Required Variables

| Variable                       | Description                                                           |
| :----------------------------- | :-------------------------------------------------------------------- |
| `NEWSAGENT_GEMINI_API_KEY`     | Your Google Gemini API Key.                                           |
| `NEWSAGENT_TODOIST_API_TOKEN`  | API Token for Todoist.                                                |
| `NEWSAGENT_TODOIST_PROJECT_ID` | The ID of the Todoist project containing your newsletter items.       |
| `NEWSAGENT_GLEAN_DIR`          | Directory path containing markdown files to be used as style context. |

### Optional Variables

| Variable                            | Description                                           | Default                |
| :---------------------------------- | :---------------------------------------------------- | :--------------------- |
| `NEWSAGENT_GEMINI_MODEL`            | The Gemini model to use.                              | `gemini-3-pro-preview` |
| `NEWSAGENT_TODOIST_PROJECT_SECTION` | Specific section ID within the Todoist project.       |                        |
| `NEWSAGENT_GLEAN_FILTER`            | Glob pattern to filter files in the glean directory.  |                        |
| `NEWSAGENT_WEB_ALLOWLIST`           | Comma-separated list of allowed domains for scraping. | (All allowed)          |
| `NEWSAGENT_WEB_MAX_CHARS`           | Maximum number of characters to read from a webpage.  |                        |
| `NEWSAGENT_WEB_TIMEOUT_SECS`        | Timeout for web requests in seconds.                  |                        |
| `NEWSAGENT_DOTENV_PATH`             | Custom path to the `.env` file.                       | `.env`                 |

## Setup & Usage

1.  **Install Dependencies**: Ensure you have [Rust](https://www.rust-lang.org/) installed.
2.  **Configure Environment**: Create a `.env` file with the necessary variables.
    ```bash
    NEWSAGENT_GEMINI_API_KEY=your_key_here
    NEWSAGENT_TODOIST_API_TOKEN=your_token_here
    NEWSAGENT_TODOIST_PROJECT_ID=123456789
    NEWSAGENT_GLEAN_DIR=/path/to/archive
    ```
3.  **Run**:
    ```bash
    cargo run
    ```

The generated newsletter content will be printed to stdout.

## Development

- **Language**: Rust
- **Frameworks**: `rig-core` (LLM Agent), `tokio` (Async), `reqwest` (HTTP), `readability` (Parsing).
