#!/usr/bin/env python3
import logging
import inspect

import flatbuffers as fbs
import spear.client as client

from spear.proto.tool import (
    InternalToolCreateRequest, InternalToolCreateResponse, InternalToolCreateParamSpec)
from spear.proto.transport import Method

logger = logging.getLogger(__name__)


def register_internal_tool(agent: client.HostAgent, cb: callable,
                           name: str = None, desc: str = None) -> int:
    """
    register internal tool
    """
    builder = fbs.Builder(32)

    if name is None:
        name = cb.__name__
    tool_name_off = builder.CreateString(name)

    if desc is None:
        desc = inspect.getdoc(cb)
    tool_desc_off = builder.CreateString(desc)
    # parse all parameters from desc in the form of
    # @param name: description
    # new line is not supported yet
    param_desc = {}
    logger.info("desc: %s", desc)
    for line in desc.split("\n"):
        # trim leading and trailing spaces and tabs
        line = line.strip()
        if not line:
            continue
        if not line.startswith("@param"):
            continue
        # split by :
        name, desc = line[7:].split(":", 1)
        param_desc[name.strip()] = builder.CreateString(desc.strip())
    logger.info("param_desc: %s", param_desc)

    # create all parameters info
    sig = inspect.signature(cb)
    params = []
    names = {}
    types = {}
    for p in sig.parameters:
        names[p] = builder.CreateString(p)
        if sig.parameters[p].annotation is inspect.Parameter.empty:
            types[p] = builder.CreateString("string")
        else:
            types[p] = builder.CreateString(sig.parameters[p].annotation)
    for p in sig.parameters:
        InternalToolCreateParamSpec.InternalToolCreateParamSpecStart(builder)
        InternalToolCreateParamSpec.InternalToolCreateParamSpecAddName(
            builder, names[p])
        InternalToolCreateParamSpec.InternalToolCreateParamSpecAddType(
            builder, types[p])
        if p in param_desc:
            InternalToolCreateParamSpec.InternalToolCreateParamSpecAddDescription(
                builder, param_desc[p])
        InternalToolCreateParamSpec.InternalToolCreateParamSpecAddRequired(
            builder, sig.parameters[p].default is inspect.Parameter.empty)
        params.append(
            InternalToolCreateParamSpec.InternalToolCreateParamSpecEnd(builder))

    InternalToolCreateRequest.InternalToolCreateRequestStartParamsVector(
        builder, len(params))
    for p in reversed(params):
        builder.PrependUOffsetTRelative(p)
    params_off = builder.EndVector()

    InternalToolCreateRequest.InternalToolCreateRequestStart(builder)
    InternalToolCreateRequest.InternalToolCreateRequestAddName(
        builder, tool_name_off)
    InternalToolCreateRequest.InternalToolCreateRequestAddDescription(
        builder, tool_desc_off)
    InternalToolCreateRequest.InternalToolCreateRequestAddParams(
        builder, params_off)
    data_off = InternalToolCreateRequest.InternalToolCreateRequestEnd(builder)
    builder.Finish(data_off)

    data = agent.exec_request(
        Method.Method.InternalToolCreate,
        builder.Output(),
    )

    resp = InternalToolCreateResponse.InternalToolCreateResponse.\
        GetRootAsInternalToolCreateResponse(
            data, 0)
    return resp.ToolId()
