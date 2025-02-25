#!/usr/bin/env python3
import logging
import sys

import flatbuffers as fbs
import spear.client as client

from spear.proto.speech import ASRRequest, ASRResponse
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

DEFAULT_ASR_MODEL = "whisper-1"


def audio_asr(agent: client.HostAgent, data, model=DEFAULT_ASR_MODEL) -> str:
    """
    convert audio to text
    """
    logger.info("Testing ASR model: %s with data len: %d", model, len(data))

    builder = fbs.Builder(0)
    model_off = builder.CreateString(model)
    data_off = builder.CreateByteVector(data)

    ASRRequest.ASRRequestStart(builder)
    ASRRequest.ASRRequestAddModel(builder, model_off)
    ASRRequest.ASRRequestAddAudio(builder, data_off)
    asr_off = ASRRequest.ASRRequestEnd(builder)

    TransformRequest.StartInputTypesVector(builder, 1)
    builder.PrependInt32(TransformType.TransformType.Audio)
    input_types_off = builder.EndVector(1)

    TransformRequest.StartOutputTypesVector(builder, 1)
    builder.PrependInt32(TransformType.TransformType.Text)
    output_types_off = builder.EndVector()

    TransformRequest.StartOperationsVector(builder, 1)
    builder.PrependInt32(TransformOperation.TransformOperation.ASR)
    operations_off = builder.EndVector()

    TransformRequest.TransformRequestStart(builder)
    TransformRequest.AddInputTypes(builder, input_types_off)
    TransformRequest.AddOutputTypes(builder, output_types_off)
    TransformRequest.AddOperations(builder, operations_off)
    TransformRequest.AddParams(builder, asr_off)
    TransformRequest.AddParamsType(
        builder,
        TransformRequest_Params.TransformRequest_Params.spear_proto_speech_ASRRequest,
    )
    builder.Finish(TransformRequest.End(builder))

    data = agent.exec_request(Method.Method.Transform, builder.Output())

    resp = TransformResponse.TransformResponse.GetRootAsTransformResponse(
        data, 0)
    if (
        resp.DataType()
        != TransformResponse_Data.TransformResponse_Data.spear_proto_speech_ASRResponse
    ):
        raise ValueError("Unexpected response data type")

    asr_resp = ASRResponse.ASRResponse()
    asr_resp.Init(resp.Data().Bytes, resp.Data().Pos)

    return asr_resp.Text()
