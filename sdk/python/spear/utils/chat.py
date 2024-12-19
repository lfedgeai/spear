#!/usr/bin/env python3
import logging

import spear.client as client
import spear.hostcalls.transform as tf

logger = logging.getLogger(__name__)


def chat_completion(
    agent: client.HostAgent,
    prompt: str,
    toolset_id: int = -1,
    role: str = "user",
    model: str = "gpt-4o",
) -> list[tf.ChatChoice]:
    """
    get user input
    """
    resp = agent.exec_request(
        "transform",
        tf.TransformRequest(
            input_types=[tf.TransformType.TEXT],
            output_types=[tf.TransformType.TEXT],
            operations=[tf.TransformOperation.LLM, tf.TransformOperation.TOOLS],
            params={
                "model": model,
                "messages": [
                    tf.ChatMessageV2(
                        metadata=tf.ChatMessageV2Metadata(role=role),
                        content=prompt,
                    )
                ],
                "toolset_id": toolset_id,
            },
        ),
    )

    if isinstance(resp, client.JsonRpcOkResp):
        resp = tf.TransformResponse.schema().load(resp.result)
        resp = tf.ChatResponseV2.schema().load(resp.results[0].data)
        return resp.messages
    else:
        raise ValueError(f"Error: {resp.message}")
