#!/usr/bin/env python3
import logging

import spear.client as client
import spear.hostcalls.tools as tools

logger = logging.getLogger(__name__)


def new_toolset(
    agent: client.HostAgent, name: str, description: str, workload_name: str = None
) -> str:
    """
    create a new toolset
    """
    req = {
        "name": name,
        "description": description,
    }
    if workload_name:
        req["workload_name"] = workload_name
    resp = agent.exec_request(
        "toolset.new",
        req,
    )

    if isinstance(resp, client.JsonRpcOkResp):
        logger.debug("Toolset created with id: %s", resp.result)
        resp = tools.NewToolsetResponse.schema().load(resp.result)
        return resp.toolset_id
    else:
        raise ValueError(f"Error creating toolset: {resp.message}")
