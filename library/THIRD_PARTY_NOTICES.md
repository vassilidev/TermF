# Third-party notices

Some role prompts in `library/roles/` and some skills in `library/skills/` adapt short passages from the MIT-licensed works below, rewritten and shortened. Each role that does lists its sources in the `credits` key of its frontmatter, each skill in `metadata.credits`; TermF reads that key but never puts it in a prompt.

| Source | Commit | License | Files adapted | Used in |
|---|---|---|---|---|
| https://github.com/obra/superpowers | 22f0b463e7a5e579904dd6211a520321035526f7 | MIT | `skills/subagent-driven-development/task-reviewer-prompt.md` | `roles/qa.md`, `roles/secops.md`, `roles/reviewer.md` |
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

## Skills — lot A

Some skills in `library/skills/` adapt short passages from the MIT-licensed works below, rewritten and shortened. Each skill that does lists its sources in `metadata.credits` in its frontmatter.

| Source | Commit | License | Files adapted | Used in |
|---|---|---|---|---|
| https://github.com/mattpocock/skills | abeeaf95660ba9e8b7c753eb38f441ca707a8823 | MIT | `skills/engineering/tdd/SKILL.md`, `skills/engineering/tdd/tests.md`, `skills/engineering/tdd/mocking.md` | `skills/tests-from-criteria/SKILL.md` |

Copyright (c) 2026 Matt Pocock (mattpocock/skills), under the MIT License whose text is reproduced above.

## Skills — lot C

Some skills in `library/skills/` adapt short passages from the MIT-licensed work below, rewritten and shortened; each lists it in the `credits` key of its `metadata`. Everything else in these skills is written from the official pages in their `termf-sources` and restated without copying. These sources are cited by identifier and link only, because of their licences: OWASP (CC BY-SA 4.0), AWS documentation (CC BY-SA 4.0), Terraform documentation (BUSL-1.1), Redis documentation (CC BY-NC-SA 4.0, non-commercial), MongoDB documentation (site terms: no copying, non-commercial use), MySQL documentation (proprietary, Oracle).

| Source | Commit | License | Files adapted | Used in |
|---|---|---|---|---|
| https://github.com/cloudflare/security-audit-skill | c1c8a8c1471069fb0e188eeaff69b8e8db6564a8 | MIT | `skills/security-audit/SKILL.md` (operating modes; a finding names its principal, boundary and result) | `skills/security-web/SKILL.md` |

### MIT License

Copyright (c) 2025-2026 Cloudflare, Inc. (cloudflare/security-audit-skill)

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
