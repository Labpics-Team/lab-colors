import unittest

from program_cpu_profile import summarize_profile


def profile(samples, deltas):
    return {"startTime": 100, "endTime": 130,
            "nodes": [{"id": 1, "children": [2, 3], "callFrame": {}},
                      {"id": 2, "callFrame": {}}, {"id": 3, "callFrame": {}}],
            "samples": samples, "timeDeltas": deltas}


class CpuTimelineTests(unittest.TestCase):
    def test_out_of_order_events_preserve_weights_and_unattributed_tail(self):
        # In timestamp order the samples belong to frames 3@7, 2@10, 2@15.
        reference = summarize_profile(profile([3, 2, 2], [7, 3, 5]))
        self.assertEqual(reference["outOfOrderEvents"], 0)
        for samples, deltas in [([2, 3, 2], [10, -3, 8]), ([2, 2, 3], [10, 5, -8])]:
            summary = summarize_profile(profile(samples, deltas))
            self.assertEqual(summary["outOfOrderEvents"], 1)
            rows = summary["topBySelf"]
            self.assertEqual({r["id"]: r["selfUs"] for r in rows}, {2: 8, 3: 7})
            self.assertEqual(rows, reference["topBySelf"])
            self.assertEqual(summary["sampledTimeUs"], 15)
            self.assertEqual(summary["unattributedTailUs"], 15)
            self.assertEqual(summary["topByInclusive"][0]["inclusiveUs"], 15)

    def test_equal_timestamps_keep_capture_order_without_inventing_time(self):
        summary = summarize_profile(profile([3, 2, 3], [10, 0, 5]))
        self.assertEqual(summary["sampleCount"], 3)
        self.assertEqual([(r["id"], r["selfUs"]) for r in summary["topBySelf"]], [(3, 15)])

    def test_invalid_intermediate_timestamp_or_identity_is_not_repaired(self):
        for samples, deltas in [([2, 3], [-1, 4]), ([2, 3], [31, -20]),
                                ([2, 4], [3, -1]), ([2], [False]), ([2, 3], [3])]:
            with self.subTest(samples=samples, deltas=deltas), self.assertRaises(ValueError):
                summarize_profile(profile(samples, deltas))


if __name__ == "__main__":
    unittest.main()
