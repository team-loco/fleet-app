import unittest

import migrate


def pod(selector):
    return {"spec": {"nodeSelector": selector}}


class StalePod(unittest.TestCase):
    def test_pod_with_pre_rename_selector_is_stale(self):
        self.assertTrue(migrate.is_stale(pod({"fleet.dev/scenario": "nonexistent-node"})))

    def test_pod_with_current_selector_is_kept(self):
        self.assertFalse(migrate.is_stale(pod({"flock.dev/scenario": "nonexistent-node"})))

    def test_pod_without_selector_is_stale(self):
        self.assertTrue(migrate.is_stale({"spec": {}}))


if __name__ == "__main__":
    unittest.main()
