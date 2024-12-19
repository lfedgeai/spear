#!/usr/bin/env python3
import logging
import sys
import time

import spear.client as client
import spear.hostcalls.tools as tools
import spear.hostcalls.transform as tf
from spear.utils.tools import new_toolset
from spear.utils.chat import chat_completion

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

    ocrToolsetId = new_toolset(
        agent,
        name="py_ocr_tools",
        description="testing external toolset",
        workload_name="py_ocr_tools",
    )

    toolsetid = new_toolset(
        agent,
        name="toolset",
        description="Toolset for sending email",
    )

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
            toolset_id=toolsetid,
            cb="sleep",
        ),
    )

    if isinstance(resp, client.JsonRpcOkResp):
        logger.info("Tool created with id: %s", resp.result)
        toolid = tools.NewToolResponse.schema().load(resp.result)
        logger.info("Tool created with id: %s", toolid.tool_id)
    elif isinstance(resp, client.JsonRpcErrorResp):
        agent.stop()
        return resp.message
    else:
        agent.stop()
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

    resp = chat_completion(agent, params, toolsetid)
    logger.info("Chat completion response: %s", resp)

    agent.stop()


if __name__ == "__main__":
    agent.register_handler("handle", handle)
    agent.register_handler("sleep", sleep)
    agent.run()
