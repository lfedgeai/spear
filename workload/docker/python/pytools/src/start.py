#!/usr/bin/env python3
import argparse
import logging
import sys

import spear.client as client
import spear.hostcalls.tools as tools

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
        "newtool",
        tools.NewToolRequest(
            name="sendmail",
            description="Tools for sending email",
            params=[
                tools.NewToolParams(
                    name="to",
                    type="str",
                    description="The email address to send to",
                    required=True,
                    cb="sendmail",
                ),
                tools.NewToolParams(
                    name="subject",
                    type="str",
                    description="The subject of the email",
                    required=True,
                    cb="sendmail",
                ),
                tools.NewToolParams(
                    name="message",
                    type="str",
                    description="The message of the email",
                    required=True,
                    cb="sendmail",
                ),
            ],
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
        "newtoolset",
        tools.NewToolsetRequest(
            name="toolset",
            description="Toolset for sending email",
            tool_ids=[toolid.tid],
        ),
    )
    
    agent.stop()
    if isinstance(resp, client.JsonRpcOkResp):
        logger.info("Toolset created with id: %s", resp.result)
        return tools.NewToolsetResponse.schema().load(resp.result)
    elif isinstance(resp, client.JsonRpcErrorResp):
        return resp.message
    else:
        return "Unknown error"


if __name__ == "__main__":
    addr, secret = parse_args()
    agent.register_handler("handle", handle)
    agent.run(addr, secret)
