# Third-party notices

Some role prompts in `library/roles/` adapt short passages from the MIT-licensed works below, rewritten and shortened. Each role that does lists its sources in the `credits` key of its frontmatter; TermF reads that key but never puts it in a prompt.

| Source | Commit | License | Files adapted | Used in |
|---|---|---|---|---|
| https://github.com/obra/superpowers | 22f0b463e7a5e579904dd6211a520321035526f7 | MIT | `skills/subagent-driven-development/task-reviewer-prompt.md` | `roles/qa.md`, `roles/secops.md` |
| https://github.com/garrytan/gstack | dd611038ca41f81fab9fa129b670852c17954b44 | MIT | `qa/SKILL.md`, `crew/docs/captain.md` (neither is listed in its `NOTICE.md` as derived from an Apache-2.0 work) | `roles/qa.md`, `roles/cto.md` |
| https://github.com/wshobson/agents | 91ff75307f5668916f3eb77d5ba556266c961376 | MIT | `plugins/agent-teams/agents/team-lead.md` | `roles/lead.md`, `roles/cto.md` |
| https://github.com/FoundationAgents/MetaGPT | 11cdf466d042aece04fc6cfd13b28e1a70341b1f | MIT | `metagpt/prompts/di/team_leader.py` | `roles/lead.md`, `roles/cto.md` |
| https://github.com/msitarzewski/agency-agents | 8ae6164602c95ca78828b148b9052460ee426885 | MIT | `testing/testing-reality-checker.md` | `roles/qa.md` |
| https://github.com/bmad-code-org/BMAD-METHOD | e435a22574fe55722841086e5566420178b98df3 | MIT (with a trademark notice) | `skills/method/bmad-agent-po/customize.toml`, `skills/method/bmad-agent-architect/customize.toml` | `roles/po.md`, `roles/architect.md` |

The BMAD-METHOD license reserves its names as trademarks of BMad Code, LLC; TermF uses none of them for a role, a workflow or a feature, and none of its personas.

## MIT License

Copyright (c) 2025 Jesse Vincent (obra/superpowers)
Copyright (c) 2026 Garry Tan (garrytan/gstack)
Copyright (c) 2024 Seth Hobson (wshobson/agents)
Copyright (c) 2024 Chenglin Wu (FoundationAgents/MetaGPT)
Copyright (c) 2025 AgentLand Contributors (msitarzewski/agency-agents)
Copyright (c) 2025 BMad Code, LLC (bmad-code-org/BMAD-METHOD)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
