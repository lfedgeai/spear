#!/usr/bin/env python3
import argparse
import logging
import sys
import os
import base64

import spear.client as client
import spear.utils.io as io
import spear.hostcalls.tools as tools
import spear.hostcalls.transform as tf

logging.basicConfig(
    level=logging.DEBUG,  # Set the desired logging level
    # Customize the log format
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    handlers=[logging.StreamHandler(stream=sys.stderr)],  # Log to stderr
)

logger = logging.getLogger(__name__)
logger.setLevel(logging.INFO)

agent = client.HostAgent()


def parse_args():
    """
    parse the command line arguments
    """
    argparser = argparse.ArgumentParser()
    argparser.add_argument("--service-addr", type=str, required=True)
    argparser.add_argument("--secret", type=int, required=True)
    args = argparser.parse_args()
    return args.service_addr, args.secret



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
        data = base64.b64decode(data).decode("utf-8")
        logger.info("Response Len: %s", len(data))
    elif isinstance(resp, client.JsonRpcErrorResp):
        raise Exception(resp)

    agent.stop()
    return "done"


if __name__ == "__main__":
    addr, secret = parse_args()
    agent.register_handler("handle", handle)
    agent.run(addr, secret)
