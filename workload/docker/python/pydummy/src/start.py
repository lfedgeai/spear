#!/usr/bin/env python3
import logging
import sys

import spear.client as client
import spear.transform.chat as chat

logging.basicConfig(
    level=logging.DEBUG,  # Set the desired logging level
    # Customize the log format
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    handlers=[logging.StreamHandler(stream=sys.stderr)],  # Log to stderr
)

logger = logging.getLogger(__name__)
logger.setLevel(logging.INFO)

agent = client.HostAgent()


def handle(params):
    """
    handle the request
    """
    logger.debug("Handling request: %s", params)
    test("gpt-4o")
    #test("text-embedding-ada-002")
    #test("bge-large-en-v1.5")


def test(model):
    """
    test the model
    """
    logger.info("Testing model: %s", model)

    resp = chat.chat(agent, "hi", model=model)
    logger.info(resp)

    agent.stop()
    return "done"


if __name__ == "__main__":
    agent.register_handler("handle", handle)
    agent.run()
