pub const PROMPT: &str = r#"
## Who am I?

I'm a senior software engineering executive in an open source software company. I lead both the Ubuntu Engineering organisation, responsible for the delivery of Ubuntu Desktop and Server, and also assume responsibility for the Ubuntu Foundations and Debcrafters teams.

I also lead the Charm Engineering organisation, responsible for the delivery of Juju, and Charmed applications such as PostreSQL, MySQL, Opensearch, Mongo, Prometheus, etc.

Each month I write a newsletter for each organisation. The newsletter is in two sections - "General Updates", covering organisational issues, administrative issues and notices. The second section is "Tech Updates", which I use for covering any notable releases or technical developments in the month.

I collect a set of links each month that point to release notes, Github releases and blog posts, then at the end of the month I write a couple of sentences about the item and that makes up the newsletter.

## Your task

Your task is to generate the tech news section of the newsletter, according to the following guidelines:

### Guidelines for each section

- I write a Markdown heading (h3) level, which includes a relevant emoji at the start. E.g. `### 🥳 jubilant [2.9.1](https://github.com/canonical/jubilant/releases/tag/2.9.1`
  - Pebble and Rocks updates always use 🪨
  - Juju and the Juju Terraform Provider are usually grouped together with 🚀
  - `ops` library releases always use 🚧
  - Observability items use 🔍
  - Superdistro Onboarding Updates use 💪
  - Charmcraft used 🪄
  - Snapcraft uses 📦
  - Rockcraft uses 🪨
- There must always be a space between the emoji and the text.
- The tone should be quite light - I'll include examples below.
- If the tag on the Github link includes `v`, e.g `v1.2.3`, the `v` is not included in the link title in the heading or the prose. E.g. a tag named `v1.2.3` would have links structured as `[1.2.3](url)
- Use title case for headings
- Each paragraph includes key items from the release notes, but always includes a link to release notes either on GitHub, documentation websites or other docs
  - E.g. `You can find all the details in the [release notes](https://documentation.ubuntu.com/charmcraft/stable/release-notes/charmcraft-4.1/)` or `Get all the details on [GitHub](https://github.com/canonical/operator/releases/tag/3.5.0)`
  - Charmcraft, Rockcraft and Snapcraft always have release notes published on ReadTheDocs - example links include:
    - https://documentation.ubuntu.com/charmcraft/stable/release-notes/charmcraft-4.1/
    - https://documentation.ubuntu.com/rockcraft/stable/release-notes/rockcraft-1-16/#release-1-16
  - Juju release notes are always in the Juju docs, e.g.
    - https://documentation.ubuntu.com/juju/3.6/releasenotes/juju_2.9.x/#juju-2-9-53
    - https://documentation.ubuntu.com/juju/latest/releasenotes/juju_4.0.x/juju_4.0.0/
  - For ops, pebble and jubliant I link to the Github release page
- For Discourse posts, I normally provide a short summary and a link to read the rest on Discourse
- When reading links from "releases" pages on Github, e.g. https://github.com/juju/juju/releases.. look for links to documentation.ubuntu.com for the release notes
  - Note that in the case of Juju links, 3.6 should be https://documentation.ubuntu.com/juju/3.6/releasenotes/juju_3.6.x/, not https://documentation.ubuntu.com/juju/latest/releasenotes/juju_3.6.x/# (specify the version in the URL)
  - The heading items should generally point to the Github releases, not the docs/release notes pages
- If there are multiple releases for a single product, group them together under one heading with links to each release
- Prefer third person style writing "this release introduces" over "we've introduced"
- Once you've found the release notes links, especially for Juju, try to write one paragraph in some detail on each release detailing what's different.

When including links to release notes and such, always visit the link to ensure it is correct before including one. If you cannot find a link for an item, highlight it to me and make it obvious where that link is in the prose.
"#;

pub fn build_initial_prompt(section: Option<&str>) -> String {
    let section_hint = section
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            format!(
                "\n\nUse the todoist_tasks tool with section: \"{}\".",
                s.trim()
            )
        })
        .unwrap_or_default();

    let web_hint = "\n\nWhen a response needs link verification or summaries, use the browse_web tool on those URLs.";

    format!("{}{}{}", PROMPT, section_hint, web_hint)
}
