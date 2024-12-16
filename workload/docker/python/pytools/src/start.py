#!/usr/bin/env python3
import argparse
import json
import logging
import sys
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


def sleep(params):
    """
    sleep for a given number of seconds
    """
    logger.info("Sleeping for %s seconds", params["seconds"])
    time.sleep(params["seconds"])    
    return "done"


def handle(params):
    """
    handle the request
    """
    logger.info("Handling request: %s", params)
    resp = agent.exec_request(
        "tool.new",
        tools.NewToolRequest(
            name="sleep",
            description="Tools for sleeping for a given number of seconds",
            params=[
                tools.NewToolParams(
                    name="seconds",
                    type="integer",
                    description="Seconds to sleep",
                    required=True,
                ),
            ],
            cb="sleep",
        ),
    )

    toolid = None
    if isinstance(resp, client.JsonRpcOkResp):
        logger.info("Tool created with id: %s", resp.result)
        toolid = tools.NewToolResponse.schema().load(resp.result)
    elif isinstance(resp, client.JsonRpcErrorResp):
        agent.stop()
        return resp.message
    else:
        agent.stop()
        return "Unknown error"

    resp = agent.exec_request(
        "toolset.new",
        tools.NewToolsetRequest(
            name="toolset",
            description="Toolset for sending email",
            tool_ids=[toolid.tool_id],
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
        logger.debug("Builtin tools installed with id: %s", resp.result)
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
            operations=[tf.TransformOperation.LLM, tf.TransformOperation.TOOLS],
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
    elif isinstance(resp, client.JsonRpcErrorResp):
        return resp.message
    else:
        return "Unknown error"


if __name__ == "__main__":
    addr, secret = parse_args()
    agent.register_handler("handle", handle)
    agent.register_handler("sleep", sleep)
    agent.run()
