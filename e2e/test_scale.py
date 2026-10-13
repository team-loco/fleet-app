import collections
import unittest

import scale


class ScaleScenario(unittest.TestCase):
    def test_counts_and_namespace_are_bounded(self):
        objects = list(scale.objects())
        counts = collections.Counter(item["kind"] for item in objects)
        self.assertEqual(counts, {
            "ConfigMap": scale.CONFIGMAPS,
            "Service": scale.SERVICES,
            "Deployment": scale.DEPLOYMENTS,
            "Event": scale.EVENTS,
            "Pod": scale.PODS,
        })
        self.assertTrue(all(item["metadata"]["namespace"] == scale.NAMESPACE for item in objects))
        names = {(item["kind"], item["metadata"]["name"]) for item in objects}
        self.assertEqual(len(names), len(objects))

    def test_only_pause_pods_consume_cluster_capacity(self):
        for item in scale.objects():
            if item["kind"] == "Deployment":
                self.assertEqual(item["spec"]["replicas"], 0)
                self.assertEqual(item["spec"]["selector"]["matchLabels"], item["spec"]["template"]["metadata"]["labels"])
            if item["kind"] == "Service":
                self.assertEqual(item["spec"]["clusterIP"], "None")
            if item["kind"] == "Pod":
                self.assertEqual(item["spec"]["containers"][0]["image"], scale.IMAGE)

    def test_every_event_refers_to_a_scale_pod(self):
        objects = list(scale.objects())
        pods = {item["metadata"]["name"] for item in objects if item["kind"] == "Pod"}
        for item in objects:
            if item["kind"] == "Event":
                self.assertIn(item["involvedObject"]["name"], pods)
                self.assertEqual(item["involvedObject"]["namespace"], scale.NAMESPACE)


if __name__ == "__main__":
    unittest.main()
