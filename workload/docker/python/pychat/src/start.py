#!/usr/bin/env python3
import argparse
import base64
import logging
import sys

import spear.client as client
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


def handle(params):
    """
    handle the request
    """
    logger.debug("Handling request: %s", params)
    msg = tf.ChatMessageV2(
        metadata=tf.ChatMessageV2Metadata(role="user"), content=params
    )
    resp = agent.exec_request(
        "transform",
        tf.TransformRequest(
            input_types=[tf.TransformType.TEXT],
            output_types=[tf.TransformType.TEXT],
            operations=[tf.TransformOperation.LLM],
            params={
                "model": "llama", # "gpt-4o",
                "messages": [msg],
            },
        ),
    )

    agent.stop()
    if isinstance(resp, client.JsonRpcOkResp):
        resp = tf.TransformResponse.schema().load(resp.result)
        data = resp.results[0].data
        data = base64.b64decode(data).decode("utf-8")
        print(data, flush=True)
        return data
    elif isinstance(resp, client.JsonRpcErrorResp):
        return resp.message
    else:
        return "Unknown error"


if __name__ == "__main__":
    agent.register_handler("handle", handle)
    agent.run()
