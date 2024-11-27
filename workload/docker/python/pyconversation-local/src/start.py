#!/usr/bin/env python3
import argparse
import logging
import sys
import json
import time

import spear.client as client
import spear.hostcalls.tools as tools
import spear.hostcalls.transform as tf

logging.basicConfig(
    level=logging.DEBUG,  # Set the desired logging level
    # Customize the log format
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    handlers=[logging.StreamHandler(stream=sys.stderr)],  # Log to stderr
)

logger = logging.getLogger(__name__)

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
    logger.info("Handling request: %s", params)

    resp = agent.exec_request(
        "toolset.new",
        tools.NewToolsetRequest(
            name="toolset",
            description="Toolset for sending email",
            tool_ids=[],
        ),
    )

    toolsetid = None
    if isinstance(resp, client.JsonRpcOkResp):
        logger.info("Toolset created with id: %s", resp.result)
        resp = tools.NewToolsetResponse.schema().load(resp.result)
        toolsetid = resp.toolset_id
    elif isinstance(resp, client.JsonRpcErrorResp):
        return resp.message
    else:
        return "Unknown error"

    resp = agent.exec_request(
        "toolset.install.builtins",
        tools.ToolsetInstallBuiltinsRequest(
            toolset_id=toolsetid,
        ),
    )
    if isinstance(resp, client.JsonRpcOkResp):
        logger.info("Builtin tools installed with id: %s", resp.result)
    elif isinstance(resp, client.JsonRpcErrorResp):
        agent.stop()
        return resp.message
    else:
        agent.stop()
        return "Unknown error"

    resp = agent.exec_request(
        "transform",
        tf.TransformRequest(
            input_types=[tf.TransformType.TEXT],
            output_types=[tf.TransformType.TEXT],
            operations=[tf.TransformOperation.LLM],
            params={
                "model": "gpt-4o",
                "messages": [{"role": "user", "content": params}],
                "toolset_id": toolsetid,
            },
        ),
    )

    agent.stop()
    if isinstance(resp, client.JsonRpcOkResp):
        resp = tf.TransformResponse.schema().load(resp.result)
        return resp
    if isinstance(resp, client.JsonRpcErrorResp):
        return resp.message
    return "Unknown error"


if __name__ == "__main__":
    addr, secret = parse_args()
    agent.register_handler("handle", handle)
    agent.run(addr, secret)
