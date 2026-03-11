---
id: TASK-0088
title: Update TASK-0067 with exact source path for digital_trigger_node
status: Done
assignee: []
created_date: '2026-03-10 12:32'
updated_date: '2026-03-10 14:15'
labels:
  - re
  - housekeeping
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The source path leak confirms the trigger engine is at:
`/workspace/monorepo/graph-io/nodes/digital_trigger_node/src/digital_trigger_node.cpp`

Update TASK-0067 with this information to aid decompilation. Search for assertion strings from this file to find the function addresses in the binary.
<!-- SECTION:DESCRIPTION:END -->
