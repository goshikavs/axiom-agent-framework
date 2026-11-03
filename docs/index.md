---
layout: home
title: Goshikq Docs
titleTemplate: false
description: Documentation for Goshikq, an open-source AI agent framework with a Rust runtime for durable execution, multi-agent orchestration, and verified outcomes.
hero:
  name: Goshikq
  text: AI agent framework with verified outcomes
  tagline: Rust runtime, durable execution, multi-agent orchestration, Python and TypeScript SDKs, and Goshikq Studio.
  actions:
    - theme: brand
      text: Quickstart
      link: /getting-started/quickstart
    - theme: alt
      text: Architecture
      link: /ARCHITECTURE
    - theme: alt
      text: GitHub
      link: https://github.com/goshikavs/goshikq-agent-framework
features:
  - title: Durable execution
    details: Checkpoints, action journals, leases, and crash recovery live in the Rust runtime—not only in the SDK process.
  - title: Verified outcomes
    details: Goal predicates run against authoritative state. A model saying the work is done is not verification.
  - title: Multi-agent orchestration
    details: Parent and child runs, shared context, and independent goal checks. Remote SUCCESS is not parent VERIFIED.
---

Start with [installation](/guides/installation) and the [workflow builder](/guides/workflow-builder). Source repository: [goshikavs/goshikq-agent-framework](https://github.com/goshikavs/goshikq-agent-framework).
