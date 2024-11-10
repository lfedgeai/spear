#!/usr/bin/env python3
import argparse
import logging
import spear.client as client
import sys

logging.basicConfig(
    level=logging.DEBUG,  # Set the desired logging level
    # Customize the log format
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    handlers=[logging.StreamHandler(stream=sys.stderr)],  # Log to stderr
)

logger = logging.getLogger(__name__)


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
    return "Hello " + params


if __name__ == "__main__":
    addr, secret = parse_args()
    agent = client.HostAgent()
    agent.register_handler("handle", handle)
    agent.run(addr, secret)
