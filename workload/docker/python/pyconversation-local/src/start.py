#!/usr/bin/env python3
import argparse
import logging
import sys
import os
import base64

import spear.client as client
import spear.utils.io as io
import spear.hostcalls.tools as tools
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


def parse_args():
    """
    parse the command line arguments
    """
    argparser = argparse.ArgumentParser()
    argparser.add_argument("--service-addr", type=str, required=True)
    argparser.add_argument("--secret", type=int, required=True)
    args = argparser.parse_args()
    return args.service_addr, args.secret


def display_chat_message(msg):
    """
    display the chat message
    """
    assert isinstance(msg, tf.ChatMessageV2)
    if msg.metadata.tool_calls:
        for tool_call in msg.metadata.tool_calls:
            print(
                f"[{msg.metadata.role}] TOOL_CALL -> {tool_call.function.name}",
                flush=True,
            )
    elif msg.content:
        print(f"[{msg.metadata.role}] {msg.content}", flush=True)


def speak_chat_message(msg):
    """
    speak the chat message
    """
    assert isinstance(msg, tf.ChatMessageV2)
    resp = agent.exec_request(
        "transform",
        tf.TransformRequest(
            input_types=[tf.TransformType.TEXT],
            output_types=[tf.TransformType.AUDIO],
            operations=[tf.TransformOperation.TEXT_TO_SPEECH],
            params={
                "model": "tts-1",
                "voice": "nova",
                "input": msg.content,
                "format": "mp3",
            },
        ),
    )
    if isinstance(resp, client.JsonRpcOkResp):
        resp = tf.TransformResponse.schema().load(resp.result)
        assert len(resp.results) == 1
        data = resp.results[0].data
        data = base64.b64decode(data).decode("utf-8")
        logger.debug("data length: %s", len(data))
        io.speak(agent, data)
    elif isinstance(resp, client.JsonRpcErrorResp):
        logger.error("Error: %s", resp.message)


def handle(params):
    """
    handle the request
    """
    logger.debug("Handling request: %s", params)

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
        logger.debug("Toolset created with id: %s", resp.result)
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

    msg_memory = []
    while True:
        user_input = io.input(agent, "(q to quit) > ")

        # trim the user input, remove space and newline
        user_input = user_input.strip()
        if not user_input:
            continue
        if user_input == "q":
            print("Quitting")
            break

        msg_memory.append(
            tf.ChatMessageV2(
                metadata=tf.ChatMessageV2Metadata(role="user"), content=user_input
            )
        )

        resp = agent.exec_request(
            "transform",
            tf.TransformRequest(
                input_types=[tf.TransformType.TEXT],
                output_types=[tf.TransformType.TEXT],
                operations=[tf.TransformOperation.LLM],
                params={
                    "model": "llama", #"gpt-4o",
                    "messages": msg_memory,
                    "toolset_id": toolsetid,
                },
            ),
        )

        new_msg_memory = []
        if isinstance(resp, client.JsonRpcOkResp):
            resp = tf.TransformResponse.schema().load(resp.result)
            # base64 decode the response string
            data = resp.results[0].data
            data = base64.b64decode(data).decode("utf-8")
            res = tf.ChatResponseV2.schema().loads(data)
            new_msg_memory = res.messages
        elif isinstance(resp, client.JsonRpcErrorResp):
            break

        tmp_msgs = new_msg_memory[len(msg_memory) :]
        for msg in tmp_msgs:
            display_chat_message(msg)
            if msg.metadata.role == "assistant" and not msg.metadata.tool_calls:
                speak_chat_message(msg)

        msg_memory = new_msg_memory

    agent.stop()
    return "done"


if __name__ == "__main__":
    addr, secret = parse_args()
    agent.register_handler("handle", handle)
    agent.run(addr, secret)
