"""Time attribution for a CDP Profile, independent of public latency measurements."""
import math


def summarize_profile(profile):
    """Суммирует интервалы CDP samples/timeDeltas; hitCount не задаёт время."""
    nodes = {node["id"]: node for node in profile["nodes"]}
    if not nodes or len(nodes) != len(profile["nodes"]):
        raise ValueError("CPU profile node IDs are empty or duplicated")
    root = profile["nodes"][0]["id"]
    parents, order = {}, []
    pending = [(root, None)]
    while pending:
        node_id, parent = pending.pop()
        if node_id not in nodes or node_id in parents:
            raise ValueError("CPU profile is not a rooted call tree")
        parents[node_id] = parent
        order.append(node_id)
        pending.extend((child, node_id) for child in nodes[node_id].get("children", []))
    if len(order) != len(nodes):
        raise ValueError("CPU profile contains unreachable nodes")

    samples, deltas = profile["samples"], profile["timeDeltas"]
    if not samples or len(samples) != len(deltas):
        raise ValueError("CPU profile sample intervals are missing or incomplete")
    start, end = profile["startTime"], profile["endTime"]
    if any(type(t) not in (int, float) or not math.isfinite(t) for t in (start, end)) or end < start:
        raise ValueError("invalid CPU profile clock bounds")
    duration_us = end - start
    # CDP deltas are relative to the preceding *emitted* event. Reconstruct
    # absolute offsets first, then stably order the (time, frame) pairs.
    # A negative delta is a reordered sample, not negative elapsed CPU time.
    timeline, time_us, inversions = [], 0, 0
    for index, (node_id, delta) in enumerate(zip(samples, deltas)):
        if type(node_id) is not int or node_id not in nodes or type(delta) is not int:
            raise ValueError(f"invalid CPU profile sample at {index}")
        time_us += delta
        if not 0 <= time_us <= duration_us:
            raise ValueError(f"CPU sample {index} falls outside the recorded clock window")
        inversions += delta < 0
        timeline.append((time_us, node_id))
    if inversions:
        timeline.sort(key=lambda event: event[0])
    self_us = dict.fromkeys(nodes, 0)
    sampled_us = 0
    for time_us, node_id in timeline:
        self_us[node_id] += time_us - sampled_us
        sampled_us = time_us
    if sampled_us <= 0:
        raise ValueError("CPU profile contains no observed time")

    inclusive_us = self_us.copy()
    for node_id in reversed(order):
        parent = parents[node_id]
        if parent is not None:
            inclusive_us[parent] += inclusive_us[node_id]

    def top(weights):
        # Это предел объёма лога; totals охватывают все узлы и отсчёты.
        ids = sorted((node_id for node_id in nodes if weights[node_id]),
                     key=lambda node_id: (-weights[node_id], node_id))[:20]
        return [{"id": node_id, "frame": nodes[node_id]["callFrame"],
                 "selfUs": self_us[node_id], "inclusiveUs": inclusive_us[node_id]}
                for node_id in ids]

    return {"nodeCount": len(nodes), "sampleCount": len(samples),
            "outOfOrderEvents": inversions, "profileDurationUs": duration_us, "sampledTimeUs": sampled_us,
            "unattributedTailUs": duration_us - sampled_us,
            "topBySelf": top(self_us), "topByInclusive": top(inclusive_us)}
