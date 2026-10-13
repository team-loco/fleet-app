#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

NAMESPACE = "fleet-scale"
CONFIGMAPS = 3000
SERVICES = 1500
DEPLOYMENTS = 400
EVENTS = 2000
PODS = 200
IMAGE = os.environ["FLEET_SCALE_IMAGE"]
PAYLOAD_BYTES = 1024
BATCH_SIZE = 100


def object_for(kind, index, api_version="v1", **fields):
    return dict(
        apiVersion=api_version,
        kind=kind,
        metadata=dict(name=f"scale-{kind.lower()}-{index:04}", namespace=NAMESPACE),
        **fields,
    )


def objects():
    for index in range(CONFIGMAPS):
        yield object_for("ConfigMap", index, data={"payload": "x" * PAYLOAD_BYTES})
    for index in range(SERVICES):
        yield object_for("Service", index, spec={"clusterIP": "None", "ports": [{"port": 80}]})
    for index in range(DEPLOYMENTS):
        selector = {"scale-deployment": str(index)}
        spec = dict(
            replicas=0,
            selector={"matchLabels": selector},
            template={"metadata": {"labels": selector}, "spec": {"containers": [{"name": "pause", "image": IMAGE}]}},
        )
        yield object_for("Deployment", index, "apps/v1", spec=spec)
    for index in range(EVENTS):
        yield object_for(
            "Event", index,
            involvedObject={"apiVersion": "v1", "kind": "Pod", "namespace": NAMESPACE, "name": f"scale-pod-{index % PODS:04}"},
            reason="ScaleProbe", message="Fleet scale scenario event", type="Normal", count=1,
        )
    for index in range(PODS):
        yield object_for("Pod", index, spec={
            "terminationGracePeriodSeconds": 0,
            "containers": [{"name": "pause", "image": IMAGE, "resources": {"requests": {"cpu": "1m", "memory": "1Mi"}}}],
        })


if __name__ == "__main__":
    items = list(objects())
    if len(sys.argv) == 1:
        print(json.dumps(dict(apiVersion="v1", kind="List", items=items)))
    else:
        output = Path(sys.argv[1])
        for start in range(0, len(items), BATCH_SIZE):
            manifest = dict(apiVersion="v1", kind="List", items=items[start:start + BATCH_SIZE])
            (output / f"batch-{start:05}.json").write_text(json.dumps(manifest))
