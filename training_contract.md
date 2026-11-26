# Training import contract to scheduler

Give this file to the coach agent. It writes **one JSON object per line** to `training.jsonl`.
The file is fully replaced each time the agent republishes the plan. `id` must stay the same across republishes so the planner can tell what moved.

## Fields

| field | type | required | meaning |
| --- | --- | --- | --- |
| `id` | string | yes | stable unique ID, e.g. `2026-W48-tue-hurdles` |
| `date` | `YYYY-MM-DD` | yes | target day |
| `window` | `["HH:MM", "HH:MM"]` | yes | earliest start/latest end; equal width to `duration` = fixed time | 
| `duration` | minutes (int) | yes | session length including warmup/cooldown |
| `priority` | `key` \| `standard` \| `optional` | yes | key = tier 2, standard = tier 1, optional = droppable |
| `move`| `{"days": int, "keep_order_with": [id]}` | no | how far it may shift; sessions it must stay after |
| `recovery` | `{"min_gap_h": int, "before": [id]}` | no | hours of gap required before the listed sessions | 
| `location` | string | no | e.g. `Langston track`, `gym`, `home`, `work`; used for travel time calculations |
| `focus` | string | no | e.g. `hurdles+speed`; display only |
| `note` | string | no | anything else, display only |

## Example
```json
{"id":"2026-W48-tue-hurdles","date":"2026-12-01","window":["16:45","19:30"],"duration":105,"priority":"key","location":"track","focus":"110H+block starts","move":{"days":1}}
{"id":"2026-W48-wed-lift","date":2026-12-02","window":["05:00","06:30"],"duration":75,"priority":"standard","location":"gym","recovery":{"min_gap_h":20,"before":["2026-W48-thu-jumps"]}}
{"id":2026-W48-sat-tempo","date":"2026-12-05","window":["08:00","18:00"],"duration":60,"priority":"optional","focus":"tempo 200s"}
```

## Planner guarantees back to the coach agent
- `key` sessions are only moved if a tier > 3 event conflicts. The coach agent is notified.
- Any session that's moved or dropped gets logged to `training_feedback.jsonl` with `{id, action, reason_rule}`.
