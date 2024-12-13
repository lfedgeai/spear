#!/usr/bin/env python3
import argparse
import base64
import logging
import os
import sys

import spear.client as client
import spear.hostcalls.tools as tools
import spear.hostcalls.transform as tf
import spear.utils.io as io

logging.basicConfig(
    level=logging.DEBUG,  # Set the desired logging level
    # Customize the log format
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    handlers=[logging.StreamHandler(stream=sys.stderr)],  # Log to stderr
)

logger = logging.getLogger(__name__)
logger.setLevel(logging.INFO)

agent = client.HostAgent()

def handle(params):
    """
    handle the request
    """
    logger.debug("Handling request: %s", params)
    test("text-embedding-ada-002")
    test("bge-large-en-v1.5")


def test(model):
    """
    test the model
    """
    logger.info("Testing model: %s", model)
    resp = agent.exec_request(
        "transform",
        tf.TransformRequest(
            input_types=[tf.TransformType.TEXT],
            output_types=[tf.TransformType.VECTOR],
            operations=[tf.TransformOperation.EMBEDDINGS],
            params={
                "model": model,
                "input": "hi",
            },
        ),
    )

    if isinstance(resp, client.JsonRpcOkResp):
        resp = tf.TransformResponse.schema().load(resp.result)
        # base64 decode the response string
        data = resp.results[0].data
        logger.info("Response Len: %s", len(data))
    elif isinstance(resp, client.JsonRpcErrorResp):
        raise Exception(resp)

    agent.stop()
    return "done"


if __name__ == "__main__":
    agent.register_handler("handle", handle)
    agent.run()
