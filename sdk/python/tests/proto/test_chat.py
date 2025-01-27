import logging
import threading
import time
import unittest

import spear.client as client
import spear.transform.chat as chat
import spear.utils.io as io
from tests.proto.server import (TEST_SERVER_DEFAULT_PORT,
                                TEST_SERVER_DEFAULT_SECRET, TestAgentServer)

logger = logging.getLogger(__name__)
logger.setLevel(logging.DEBUG)


class TestChatProto(unittest.TestCase):
    """
    Test the chat functionality
    """

    def test_basic_chat(self):
        """
        Test the basic chat functionality
        """
        server = TestAgentServer(TEST_SERVER_DEFAULT_PORT, TEST_SERVER_DEFAULT_SECRET)
        server_thread = threading.Thread(target=server.run)
        server_thread.daemon = True
        server_thread.start()

        time.sleep(2)
        agent = client.HostAgent()

        def run_cmd():
            time.sleep(3)

            resp = chat.chat(agent, "hello world")
            print(resp)
            time.sleep(2)

            resp = io.input(agent, "input")
            print(resp)
            time.sleep(2)

            agent.stop()

        t = threading.Thread(target=run_cmd)
        t.daemon = True
        t.start()

        agent.run("localhost:12345", 12345)

    def test_case2(self):
        pass


if __name__ == "__main__":
    unittest.main()
