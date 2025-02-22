#!/usr/bin/env python3
import logging
import sys

import flatbuffers as fbs
import spear.client as client
import spear.hostcalls.transform as tf

from spear.proto.chat import (ChatCompletionRequest, ChatCompletionResponse,
                              ChatMessage, ChatMetadata, Role)
from spear.proto.chat import ToolInfo as ChatToolInfo
from spear.proto.tool import BuiltinToolInfo, InternalToolInfo, ToolInfo
from spear.proto.transform import (TransformOperation, TransformRequest,
                                   TransformRequest_Params, TransformResponse,
                                   TransformResponse_Data, TransformType)
from spear.proto.transport import Method

logging.basicConfig(
    level=logging.DEBUG,  # Set the desired logging level
    # Customize the log format
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    handlers=[logging.StreamHandler(stream=sys.stderr)],  # Log to stderr
)

logger = logging.getLogger(__name__)
logger.setLevel(logging.INFO)

DEFAULT_LLM_MODEL = "llama"  # "gpt-4o"


def chat(agent: client.HostAgent, message: str,
         model: str = DEFAULT_LLM_MODEL,
         builtin_tools: list[int] = [],
         internal_tools: list[int] = []):
    """
    handle the llm request
    """
    builder = fbs.Builder(len(message) + 2048)
    content_off = builder.CreateString(message)
    model_off = builder.CreateString(model)

    tools_off = -1
    builtin_tool_offs = []
    internal_tool_offs = []
    if len(builtin_tools) > 0:
        for tool in builtin_tools:
            assert isinstance(tool, int)
            BuiltinToolInfo.BuiltinToolInfoStart(builder)
            BuiltinToolInfo.AddToolId(builder, tool)
            tmp = BuiltinToolInfo.End(builder)
            ChatToolInfo.ToolInfoStart(builder)
            ChatToolInfo.ToolInfoAddData(builder, tmp)
            ChatToolInfo.AddDataType(
                builder,
                ToolInfo.ToolInfo.BuiltinToolInfo,
            )
            builtin_tool_offs.append(ChatToolInfo.End(builder))
    if len(internal_tools) > 0:
        for tool in internal_tools:
            assert isinstance(tool, int)
            InternalToolInfo.InternalToolInfoStart(builder)
            InternalToolInfo.AddToolId(builder, tool)
            tmp = InternalToolInfo.End(builder)
            ChatToolInfo.ToolInfoStart(builder)
            ChatToolInfo.ToolInfoAddData(builder, tmp)
            ChatToolInfo.AddDataType(
                builder,
                ToolInfo.ToolInfo.InternalToolInfo,
            )
            internal_tool_offs.append(ChatToolInfo.End(builder))

    if len(builtin_tool_offs) + len(internal_tool_offs) > 0:
        ChatCompletionRequest.StartToolsVector(
            builder, len(builtin_tool_offs) + len(internal_tool_offs))
        for off in builtin_tool_offs:
            builder.PrependUOffsetTRelative(off)
        for off in internal_tool_offs:
            builder.PrependUOffsetTRelative(off)
        tools_off = builder.EndVector()

    ChatMetadata.ChatMetadataStart(builder)
    ChatMetadata.AddRole(builder, Role.Role.User)
    metadata_off = ChatMetadata.End(builder)

    ChatMessage.ChatMessageStart(builder)
    ChatMessage.AddContent(builder, content_off)
    ChatMessage.AddMetadata(builder, metadata_off)
    msg_off = ChatMessage.End(builder)

    ChatCompletionRequest.StartMessagesVector(builder, 1)
    builder.PrependUOffsetTRelative(msg_off)
    msglist_off = builder.EndVector()

    ChatCompletionRequest.ChatCompletionRequestStart(builder)
    ChatCompletionRequest.AddMessages(builder, msglist_off)
    ChatCompletionRequest.AddModel(builder, model_off)
    if tools_off != -1:
        ChatCompletionRequest.AddTools(builder, tools_off)
    chatcomp_off = ChatCompletionRequest.End(builder)

    TransformRequest.StartInputTypesVector(builder, 1)
    builder.PrependInt32(TransformType.TransformType.Text)
    input_types_off = builder.EndVector()

    TransformRequest.StartOutputTypesVector(builder, 1)
    builder.PrependInt32(TransformType.TransformType.Text)
    output_types_off = builder.EndVector()

    TransformRequest.StartOperationsVector(builder, 1)
    builder.PrependInt32(TransformOperation.TransformOperation.LLM)
    if len(builtin_tools) > 0 or len(internal_tools) > 0:
        builder.PrependInt32(TransformOperation.TransformOperation.Tools)
    operations_off = builder.EndVector()

    TransformRequest.TransformRequestStart(builder)
    TransformRequest.AddInputTypes(builder, input_types_off)
    TransformRequest.AddOutputTypes(builder, output_types_off)
    TransformRequest.AddOperations(builder, operations_off)
    TransformRequest.AddParams(builder, chatcomp_off)
    TransformRequest.AddParamsType(
        builder,
        TransformRequest_Params.TransformRequest_Params.spear_proto_chat_ChatCompletionRequest,
    )
    builder.Finish(TransformRequest.End(builder))

    data = agent.exec_request(Method.Method.Transform, builder.Output())

    resp = TransformResponse.TransformResponse.GetRootAsTransformResponse(
        data, 0)
    if (
        resp.DataType()
        != TransformResponse_Data.TransformResponse_Data.spear_proto_chat_ChatCompletionResponse
    ):
        raise ValueError("Unexpected response data type")

    chat_resp = ChatCompletionResponse.ChatCompletionResponse()
    chat_resp.Init(resp.Data().Bytes, resp.Data().Pos)

    if chat_resp.Code() != 0:
        raise ValueError(chat_resp.Error())

    msg_len = chat_resp.MessagesLength()
    res = []
    for i in range(msg_len):
        res.append(chat_resp.Messages(i).Content().decode("utf-8"))

    return res
