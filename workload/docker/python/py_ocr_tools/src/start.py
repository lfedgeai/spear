#!/usr/bin/env python3
import logging
import sys

import spear.client as client
import spear.hostcalls.tools as tools
import spear.hostcalls.transform as tf
from paddleocr import PaddleOCR

logging.basicConfig(
    level=logging.DEBUG,  # Set the desired logging level
    # Customize the log format
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    handlers=[logging.StreamHandler(stream=sys.stderr)],  # Log to stderr
)

logger = logging.getLogger(__name__)

agent = client.HostAgent()
ocr = PaddleOCR(use_angle_cls=True, lang="en")


def ocr_detect(params):
    """
    detect text in an image
    """
    logger.info("Detecting text in image: %s", params["image"])
    result = ocr.ocr(params["image"], cls=True)
    return result


def handle(params):
    """
    handle the request
    """
    logger.info("Handling request: %s", params)


if __name__ == "__main__":
    agent.register_handler("handle", handle)
    agent.register_handler("ocr_detect", ocr_detect)
    agent.run()
